use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use tokio::sync::Notify;

#[derive(Clone)]
pub(crate) struct ProviderConcurrency(Arc<Mutex<Queues>>);

struct Queues {
    limit: usize,
    providers: HashMap<String, ProviderQueue>,
}

#[derive(Default)]
struct ProviderQueue {
    inflight: usize,
    waiting: VecDeque<Arc<Notify>>,
}

impl ProviderQueue {
    fn wake_next(&self, limit: usize) {
        if self.inflight < limit
            && let Some(next) = self.waiting.front()
        {
            next.notify_one();
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProviderLoad {
    pub inflight: usize,
    pub queued: usize,
}

pub(crate) struct Permit {
    queues: ProviderConcurrency,
    provider_id: String,
    ready: Arc<Notify>,
    acquired: bool,
}

impl ProviderConcurrency {
    pub fn new(limit: usize) -> Self {
        assert!(limit > 0);
        Self(Arc::new(Mutex::new(Queues {
            limit,
            providers: HashMap::new(),
        })))
    }

    pub fn set_limit(&self, limit: usize) {
        assert!(limit > 0);
        let mut queues = self.0.lock().expect("provider queues lock");
        queues.limit = limit;
        for queue in queues.providers.values() {
            queue.wake_next(limit);
        }
    }

    pub fn load(&self, provider_id: &str) -> ProviderLoad {
        let queues = self.0.lock().expect("provider queues lock");
        queues
            .providers
            .get(provider_id)
            .map(|queue| ProviderLoad {
                inflight: queue.inflight,
                queued: queue.waiting.len(),
            })
            .unwrap_or_default()
    }

    pub async fn acquire(&self, provider_id: &str) -> Permit {
        let ready = Arc::new(Notify::new());
        let mut permit = Permit {
            queues: self.clone(),
            provider_id: provider_id.to_owned(),
            ready: ready.clone(),
            acquired: false,
        };
        self.0
            .lock()
            .expect("provider queues lock")
            .providers
            .entry(provider_id.to_owned())
            .or_default()
            .waiting
            .push_back(ready.clone());
        loop {
            let notified = ready.notified();
            {
                let mut queues = self.0.lock().expect("provider queues lock");
                let limit = queues.limit;
                let queue = queues
                    .providers
                    .get_mut(provider_id)
                    .expect("queued provider");
                if queue.inflight < limit
                    && queue
                        .waiting
                        .front()
                        .is_some_and(|next| Arc::ptr_eq(next, &ready))
                {
                    queue.waiting.pop_front();
                    queue.inflight += 1;
                    permit.acquired = true;
                    queue.wake_next(limit);
                    return permit;
                }
            }
            notified.await;
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut queues = self.queues.0.lock().expect("provider queues lock");
        let limit = queues.limit;
        let queue = queues
            .providers
            .get_mut(&self.provider_id)
            .expect("queued provider");
        if self.acquired {
            queue.inflight -= 1;
        } else {
            queue
                .waiting
                .retain(|ready| !Arc::ptr_eq(ready, &self.ready));
        }
        queue.wake_next(limit);
        if queue.inflight == 0 && queue.waiting.is_empty() {
            queues.providers.remove(&self.provider_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::poll;

    #[tokio::test]
    async fn defaults_to_three_per_provider_and_admits_waiters_in_order() {
        let queues = ProviderConcurrency::new(3);
        let first = queues.acquire("a").await;
        let second = queues.acquire("a").await;
        let third = queues.acquire("a").await;
        let other = queues.acquire("b").await;
        let mut fourth = Box::pin(queues.acquire("a"));
        let mut fifth = Box::pin(queues.acquire("a"));
        assert!(poll!(&mut fourth).is_pending());
        assert!(poll!(&mut fifth).is_pending());
        assert_eq!(
            queues.load("a"),
            ProviderLoad {
                inflight: 3,
                queued: 2
            }
        );
        assert_eq!(
            queues.load("b"),
            ProviderLoad {
                inflight: 1,
                queued: 0
            }
        );
        drop(first);
        assert!(poll!(&mut fifth).is_pending());
        let fourth = fourth.await;
        assert_eq!(
            queues.load("a"),
            ProviderLoad {
                inflight: 3,
                queued: 1
            }
        );
        drop(second);
        let fifth = fifth.await;
        drop((third, fourth, fifth, other));
        assert!(queues.0.lock().unwrap().providers.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_requests_never_exceed_the_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let queues = ProviderConcurrency::new(3);
        let running = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..200 {
            let queues = queues.clone();
            let running = running.clone();
            let maximum = maximum.clone();
            tasks.spawn(async move {
                let _permit = queues.acquire("a").await;
                let active = running.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(active, Ordering::SeqCst);
                tokio::task::yield_now().await;
                running.fetch_sub(1, Ordering::SeqCst);
            });
        }
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
        assert!(maximum.load(Ordering::SeqCst) <= 3);
        assert_eq!(queues.load("a"), ProviderLoad::default());
    }

    #[tokio::test]
    async fn cancellation_removes_waiters_and_does_not_lose_the_wakeup() {
        let queues = ProviderConcurrency::new(1);
        let first = queues.acquire("a").await;
        let mut cancelled = Box::pin(queues.acquire("a"));
        let mut next = Box::pin(queues.acquire("a"));
        assert!(poll!(&mut cancelled).is_pending());
        assert!(poll!(&mut next).is_pending());
        drop(first);
        drop(cancelled);
        assert_eq!(
            queues.load("a"),
            ProviderLoad {
                inflight: 0,
                queued: 1
            }
        );
        drop(next.await);
        assert_eq!(queues.load("a"), ProviderLoad::default());
    }

    #[tokio::test]
    async fn resizing_wakes_existing_queues_and_drains_before_lower_limit() {
        let queues = ProviderConcurrency::new(1);
        let first = queues.acquire("a").await;
        let other = queues.acquire("b").await;
        let mut second = Box::pin(queues.acquire("a"));
        let mut other_second = Box::pin(queues.acquire("b"));
        assert!(poll!(&mut second).is_pending());
        assert!(poll!(&mut other_second).is_pending());
        queues.set_limit(2);
        let second = second.await;
        let other_second = other_second.await;
        queues.set_limit(1);
        let mut third = Box::pin(queues.acquire("a"));
        assert!(poll!(&mut third).is_pending());
        drop(first);
        assert!(poll!(&mut third).is_pending());
        drop(second);
        drop((third.await, other, other_second));
        assert!(queues.0.lock().unwrap().providers.is_empty());
    }
}
