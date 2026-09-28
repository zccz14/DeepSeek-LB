use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use dashmap::{DashMap, DashSet};
use reqwest::StatusCode;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Row};

use crate::{
    AppError, AppState, SqliteWriteGate,
    concurrency::{Permit, ProviderConcurrency, ProviderLoad},
};

#[derive(Clone, FromRow, Serialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub api_key: String,
    pub status: String,
    pub manual_disabled: i64,
    pub cooldown_until: Option<i64>,
    pub rate_limit_json: Option<String>,
    pub last_error: Option<String>,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub owner_id: Option<String>,
    /// [`PROVIDER_VISIBILITY_PUBLIC`] or [`PROVIDER_VISIBILITY_PRIVATE`].
    pub visibility: String,
    pub official_provided_usd_nanos: i64,
    pub actual_provided_usd_nanos: i64,
    #[serde(skip_serializing)]
    pub is_deleted: i64,
}

#[derive(Clone)]
pub struct Balancer {
    concurrency: ProviderConcurrency,
    cursor: Arc<AtomicU64>,
    providers: Arc<ArcSwap<Vec<Provider>>>,
    affinities: Arc<DashMap<String, AffinityEntry>>,
    dirty_affinities: Arc<DashSet<String>>,
    provider_updates: Arc<DashMap<String, ProviderUpdate>>,
}

#[derive(Clone, PartialEq)]
struct AffinityEntry {
    provider_id: String,
    expires_at: i64,
    updated_at: i64,
}

#[derive(Clone, PartialEq)]
struct ProviderUpdate {
    status: Option<String>,
    cooldown_until: Option<i64>,
    rate_limit_json: String,
    last_error: Option<String>,
    last_used_at: Option<i64>,
    updated_at: i64,
}

impl ProviderUpdate {
    fn merge(&mut self, next: Self) {
        if next.status.is_some() {
            self.status = next.status;
            self.cooldown_until = next.cooldown_until;
            self.last_error = next.last_error;
        }
        self.rate_limit_json = next.rate_limit_json;
        self.last_used_at = next.last_used_at.or(self.last_used_at);
        self.updated_at = self.updated_at.max(next.updated_at);
    }
}

pub struct Lease {
    pub provider: Provider,
    pub api_key: String,
    permit: Option<Permit>,
}

/// Downstream identity a routing decision is made for.
#[derive(Clone, Copy)]
pub struct Downstream<'a> {
    /// `user_id` of the Consumer that authenticates the request.
    owner_id: &'a str,
}

impl<'a> Downstream<'a> {
    pub fn new(owner_id: &'a str) -> Self {
        Self { owner_id }
    }
}

/// Provider visibility value consumed by any Consumer of this proxy.
pub const PROVIDER_VISIBILITY_PUBLIC: &str = "public";
/// Provider visibility value consumed only by Consumers of the provider owner.
pub const PROVIDER_VISIBILITY_PRIVATE: &str = "private";

/// Validates operator-supplied provider visibility, defaulting to private.
pub fn provider_visibility(value: Option<&str>) -> Result<String, AppError> {
    let value = value.unwrap_or(PROVIDER_VISIBILITY_PRIVATE).trim();
    if [PROVIDER_VISIBILITY_PUBLIC, PROVIDER_VISIBILITY_PRIVATE].contains(&value) {
        return Ok(value.to_owned());
    }
    Err(AppError::bad_request(
        "visibility must be public or private",
    ))
}

/// Whether a downstream Consumer may be routed to a provider.
fn provider_is_visible(provider: &Provider, downstream: Downstream<'_>) -> bool {
    provider.visibility == PROVIDER_VISIBILITY_PUBLIC
        || provider.owner_id.as_deref() == Some(downstream.owner_id)
}

impl Lease {
    pub(crate) fn release(&mut self) {
        self.permit.take();
    }
}

impl Default for Balancer {
    fn default() -> Self {
        Self {
            concurrency: ProviderConcurrency::new(3),
            cursor: Arc::new(AtomicU64::new(0)),
            providers: Arc::new(ArcSwap::from_pointee(Vec::new())),
            affinities: Arc::new(DashMap::new()),
            dirty_affinities: Arc::new(DashSet::new()),
            provider_updates: Arc::new(DashMap::new()),
        }
    }
}

impl Balancer {
    pub fn pin_affinity(&self, state: &AppState, key: &str, provider_id: &str) {
        self.remember_affinity(
            key,
            provider_id,
            chrono::Utc::now().timestamp(),
            state.config.load().affinity_ttl_seconds,
        );
    }

    pub fn affinity_provider_id(&self, key: &str) -> Option<String> {
        let hash = affinity_hash(key);
        let entry = self.affinities.get(&hash)?;
        (entry.expires_at > chrono::Utc::now().timestamp()).then(|| entry.provider_id.clone())
    }

    pub fn set_concurrency_limit(&self, limit: usize) {
        self.concurrency.set_limit(limit);
    }

    pub async fn select_provider(
        &self,
        provider_id: &str,
        downstream: Downstream<'_>,
    ) -> Result<Lease, AppError> {
        let permit = self.concurrency.acquire(provider_id).await;
        let provider = self
            .providers
            .load()
            .iter()
            .find(|provider| {
                provider.id == provider_id
                    && provider.is_deleted == 0
                    && provider_is_visible(provider, downstream)
            })
            .cloned()
            .ok_or_else(|| {
                AppError::unavailable_with_reason(
                    "provider is no longer available for this downstream",
                    "provider_not_available",
                )
            })?;
        Ok(Self::lease(provider, permit))
    }

    pub async fn select(
        &self,
        state: &AppState,
        affinity: Option<&str>,
        excluded: Option<&str>,
        downstream: Downstream<'_>,
    ) -> Result<Lease, AppError> {
        loop {
            let now = chrono::Utc::now().timestamp();
            let chosen = {
                let providers = self.providers.load();
                let eligible = eligible_providers(&providers, excluded, downstream, now);
                if eligible.is_empty() {
                    return Err(AppError::unavailable_with_reason(
                        "no available DeepSeek provider",
                        "provider_pool_empty",
                    ));
                }
                affinity
                    .and_then(|key| self.affinity_provider(key, &eligible, now))
                    .unwrap_or_else(|| self.least_inflight(&eligible))
            };
            let permit = self.concurrency.acquire(&chosen.id).await;
            // INVARIANT: Queuing can outlive credential updates, disablement or deletion.
            // Re-read the current provider before allowing any upstream traffic.
            let provider = eligible_providers(
                &self.providers.load(),
                excluded,
                downstream,
                chrono::Utc::now().timestamp(),
            )
            .into_iter()
            .find(|provider| provider.id == chosen.id);
            if let Some(provider) = provider {
                if let Some(key) = affinity {
                    self.remember_affinity(
                        key,
                        &provider.id,
                        now,
                        state.config.load().affinity_ttl_seconds,
                    );
                }
                return Ok(Self::lease(provider, permit));
            }
            // A recovered exact pool or revoked fallback permission invalidates a queued choice.
            // Drop this permit before selecting or waiting on another provider.
            drop(permit);
        }
    }

    fn lease(provider: Provider, permit: Permit) -> Lease {
        Lease {
            api_key: provider.api_key.clone(),
            provider,
            permit: Some(permit),
        }
    }

    fn affinity_provider(&self, key: &str, eligible: &[Provider], now: i64) -> Option<Provider> {
        let hash = affinity_hash(key);
        let entry = self.affinities.get(&hash)?;
        (entry.expires_at > now)
            .then(|| {
                eligible
                    .iter()
                    .find(|provider| provider.id == entry.provider_id)
                    .cloned()
            })
            .flatten()
    }

    fn least_inflight(&self, providers: &[Provider]) -> Provider {
        let start = self.cursor.fetch_add(1, Ordering::Relaxed) as usize % providers.len();
        providers
            .iter()
            .cycle()
            .skip(start)
            .take(providers.len())
            .min_by_key(|provider| {
                let load = self.concurrency.load(&provider.id);
                load.inflight + load.queued
            })
            .expect("non-empty provider list")
            .clone()
    }

    pub fn inflight(&self, provider_id: &str) -> usize {
        self.concurrency.load(provider_id).inflight
    }

    pub(crate) fn load(&self, provider_id: &str) -> ProviderLoad {
        self.concurrency.load(provider_id)
    }

    fn remember_affinity(&self, key: &str, provider_id: &str, now: i64, ttl: i64) {
        let hash = affinity_hash(key);
        self.affinities.insert(
            hash.clone(),
            AffinityEntry {
                provider_id: provider_id.to_owned(),
                expires_at: now + ttl,
                updated_at: now,
            },
        );
        self.dirty_affinities.insert(hash);
    }

    pub async fn hydrate(&self, pool: &sqlx::SqlitePool) -> Result<(), AppError> {
        self.reload_providers(pool).await?;
        let now = chrono::Utc::now().timestamp();
        let rows = sqlx::query("SELECT affinity_hash,provider_id,expires_at,updated_at FROM affinities WHERE expires_at>?")
            .bind(now)
            .fetch_all(pool)
            .await?;
        self.affinities.clear();
        for row in rows {
            self.affinities.insert(
                row.get(0),
                AffinityEntry {
                    provider_id: row.get(1),
                    expires_at: row.get(2),
                    updated_at: row.get(3),
                },
            );
        }
        Ok(())
    }

    pub async fn reload_providers(&self, pool: &sqlx::SqlitePool) -> Result<(), AppError> {
        let providers = sqlx::query_as::<_, Provider>(
            "SELECT * FROM providers WHERE is_deleted=0 ORDER BY created_at,id",
        )
        .fetch_all(pool)
        .await?;
        self.providers.store(Arc::new(providers));
        Ok(())
    }

    pub fn forget_provider(&self, provider_id: &str) {
        let mut providers = (**self.providers.load()).clone();
        providers.retain(|provider| provider.id != provider_id);
        self.providers.store(Arc::new(providers));
        self.provider_updates.remove(provider_id);

        let hashes = self
            .affinities
            .iter()
            .filter(|entry| entry.value().provider_id == provider_id)
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        for hash in hashes {
            let removed = self
                .affinities
                .remove_if(&hash, |_, entry| entry.provider_id == provider_id)
                .is_some();
            if removed {
                self.dirty_affinities.remove(&hash);
                if self.affinities.contains_key(&hash) {
                    self.dirty_affinities.insert(hash);
                }
            }
        }
    }

    pub(crate) fn start_maintenance(&self, pool: sqlx::SqlitePool, write_gate: SqliteWriteGate) {
        let balancer = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                if let Err(error) = balancer.maintain(&pool, &write_gate).await {
                    tracing::error!(%error, "provider maintenance failed");
                }
            }
        });
    }

    pub(crate) async fn maintain(
        &self,
        pool: &sqlx::SqlitePool,
        write_gate: &SqliteWriteGate,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().timestamp();
        let updates = self
            .provider_updates
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect::<Vec<_>>();
        let affinities = self
            .dirty_affinities
            .iter()
            .filter_map(|hash| {
                self.affinities
                    .get(hash.key())
                    .map(|entry| (hash.key().clone(), entry.clone()))
            })
            .collect::<Vec<_>>();
        let _write = write_gate.lock().await;
        let mut transaction = pool.begin().await?;
        for (id, update) in &updates {
            sqlx::query("UPDATE providers SET status=CASE WHEN manual_disabled=1 THEN 'disabled' WHEN ? IS NULL THEN status ELSE ? END,cooldown_until=CASE WHEN manual_disabled=1 THEN NULL WHEN ? IS NULL THEN cooldown_until ELSE ? END,rate_limit_json=?,last_error=CASE WHEN ? IS NULL THEN last_error ELSE ? END,last_used_at=COALESCE(?,last_used_at),updated_at=? WHERE id=? AND is_deleted=0")
                .bind(&update.status).bind(&update.status).bind(&update.status).bind(update.cooldown_until)
                .bind(&update.rate_limit_json).bind(&update.status).bind(&update.last_error)
                .bind(update.last_used_at).bind(update.updated_at).bind(id)
                .execute(&mut *transaction).await?;
        }
        let mut persisted_affinities = Vec::with_capacity(affinities.len());
        for (hash, entry) in affinities {
            let persisted = sqlx::query("INSERT INTO affinities(affinity_hash,provider_id,expires_at,updated_at) SELECT ?,?,?,? FROM providers WHERE id=? AND is_deleted=0 ON CONFLICT(affinity_hash) DO UPDATE SET provider_id=excluded.provider_id,expires_at=excluded.expires_at,updated_at=excluded.updated_at")
                .bind(&hash).bind(&entry.provider_id).bind(entry.expires_at).bind(entry.updated_at)
                .bind(&entry.provider_id).execute(&mut *transaction).await?.rows_affected() > 0;
            persisted_affinities.push((hash, entry, persisted));
        }
        sqlx::query("UPDATE providers SET status='active',cooldown_until=NULL,last_error=NULL,updated_at=? WHERE is_deleted=0 AND manual_disabled=0 AND status='cooldown' AND cooldown_until<=?")
            .bind(now).bind(now).execute(&mut *transaction).await?;
        sqlx::query("DELETE FROM affinities WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        for (id, update) in updates {
            if self
                .provider_updates
                .get(&id)
                .is_some_and(|current| *current == update)
            {
                self.provider_updates.remove(&id);
            }
        }
        for (hash, entry, persisted) in persisted_affinities {
            if self
                .affinities
                .get(&hash)
                .is_some_and(|current| *current == entry)
            {
                if !persisted {
                    self.affinities
                        .remove_if(&hash, |_, current| *current == entry);
                }
                self.dirty_affinities.remove(&hash);
                if self.affinities.contains_key(&hash) && !persisted {
                    self.dirty_affinities.insert(hash);
                }
            }
        }
        self.affinities.retain(|_, entry| entry.expires_at > now);
        self.recover_cached_providers(now);
        Ok(())
    }

    fn recover_cached_providers(&self, now: i64) {
        let mut providers = (**self.providers.load()).clone();
        for provider in &mut providers {
            if provider.manual_disabled == 0
                && provider.status == "cooldown"
                && provider.cooldown_until.is_some_and(|until| until <= now)
            {
                provider.status = "active".to_owned();
                provider.cooldown_until = None;
                provider.last_error = None;
                provider.updated_at = now;
            }
        }
        self.providers.store(Arc::new(providers));
    }

    fn observe(&self, provider_id: &str, update: ProviderUpdate) {
        let mut providers = (**self.providers.load()).clone();
        if let Some(provider) = providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
        {
            if provider.manual_disabled == 0
                && let Some(status) = &update.status
            {
                provider.status = status.clone();
                provider.cooldown_until = update.cooldown_until;
                provider.last_error = update.last_error.clone();
            }
            provider.rate_limit_json = Some(update.rate_limit_json.clone());
            provider.last_used_at = update.last_used_at.or(provider.last_used_at);
            provider.updated_at = update.updated_at;
        }
        self.providers.store(Arc::new(providers));
        self.provider_updates
            .entry(provider_id.to_owned())
            .and_modify(|current| current.merge(update.clone()))
            .or_insert(update);
    }
}

fn eligible_providers(
    providers: &[Provider],
    excluded: Option<&str>,
    downstream: Downstream<'_>,
    now: i64,
) -> Vec<Provider> {
    providers
        .iter()
        .filter(|provider| provider_is_available(provider, now))
        .filter(|provider| excluded != Some(provider.id.as_str()))
        .filter(|provider| provider_is_visible(provider, downstream))
        .cloned()
        .collect()
}

fn provider_is_available(provider: &Provider, now: i64) -> bool {
    provider.is_deleted == 0
        && provider.manual_disabled == 0
        && (provider.status == "active"
            || (provider.status == "cooldown"
                && provider.cooldown_until.is_some_and(|until| until <= now)))
}

pub fn affinity_hash(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

pub async fn track_response(
    state: &AppState,
    provider_id: &str,
    status: StatusCode,
    body: &[u8],
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();
    let error = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|response| response.get("error").cloned());
    let rate_json = error
        .as_ref()
        .map(serde_json::Value::to_string)
        .unwrap_or_else(|| "{}".to_owned());
    let update = match status.as_u16() {
        401 | 403 => ProviderUpdate {
            status: Some("auth_error".to_owned()),
            cooldown_until: None,
            rate_limit_json: rate_json,
            last_error: Some("upstream API key was rejected".to_owned()),
            last_used_at: Some(now),
            updated_at: now,
        },
        402 => ProviderUpdate {
            status: Some("balance_error".to_owned()),
            cooldown_until: None,
            rate_limit_json: rate_json,
            last_error: Some("upstream balance is insufficient".to_owned()),
            last_used_at: Some(now),
            updated_at: now,
        },
        429 => {
            let cooldown_until = now + RATE_LIMIT_COOLDOWN_SECONDS;
            ProviderUpdate {
                status: Some("cooldown".to_owned()),
                cooldown_until: Some(cooldown_until),
                rate_limit_json: rate_json.clone(),
                last_error: Some("rate limit reached".to_owned()),
                last_used_at: Some(now),
                updated_at: now,
            }
        }
        _ => ProviderUpdate {
            status: None,
            cooldown_until: None,
            rate_limit_json: rate_json,
            last_error: None,
            last_used_at: Some(now),
            updated_at: now,
        },
    };
    state.balancer.observe(provider_id, update);
    Ok(())
}

// ASSUMPTION: DeepSeek 429 responses carry no reset time, so a rejected request is treated as a
// short-lived saturation signal and the provider is cooled down for a fixed window before it is
// eligible again. If the window is too short the provider simply receives another 429 and cools
// down again; if it is too long other providers absorb the traffic in the meantime.
const RATE_LIMIT_COOLDOWN_SECONDS: i64 = 30;

// tests replaced below
#[cfg(test)]
mod tests {
    use super::*;
    use crate::concurrency::ProviderLoad;
    use serde_json::json;

    /// Owner of the providers and Consumer used by routing tests.
    const TEST_OWNER: &str = "test-owner";

    fn test_downstream() -> Downstream<'static> {
        Downstream::new(TEST_OWNER)
    }

    async fn seed_test_owner(state: &AppState) {
        sqlx::query("INSERT OR IGNORE INTO users(id,role,created_at) VALUES(?,'user',0)")
            .bind(TEST_OWNER)
            .execute(&state.db)
            .await
            .unwrap();
    }

    async fn seed_provider(state: &AppState, id: &str, api_key: &str, owner_id: &str) {
        sqlx::query("INSERT INTO providers(id,name,api_key,status,owner_id,created_at,updated_at) VALUES(?,?,?,'active',?,?,?)")
            .bind(id)
            .bind(id)
            .bind(api_key)
            .bind(owner_id)
            .bind(0)
            .bind(0)
            .execute(&state.db)
            .await
            .unwrap();
    }

    #[test]
    fn affinity_is_hashed_before_storage() {
        assert_ne!(affinity_hash("session-secret"), "session-secret");
    }

    #[test]
    fn rate_limit_tracking_uses_a_fixed_cooldown_window() {
        assert_eq!(RATE_LIMIT_COOLDOWN_SECONDS, 30);
    }

    #[tokio::test]
    async fn queued_requests_recheck_api_keys_and_skip_deleted_providers() {
        use futures_util::poll;
        let state = crate::test_state("http://upstream.invalid").await;
        seed_test_owner(&state).await;
        for id in ["a", "b"] {
            seed_provider(&state, id, "sk-old", TEST_OWNER).await;
        }
        state.balancer.reload_providers(&state.db).await.unwrap();
        state.balancer.set_concurrency_limit(1);
        state.balancer.pin_affinity(&state, "session", "a");
        let first = state
            .balancer
            .select(&state, Some("session"), None, test_downstream())
            .await
            .unwrap();
        let mut waiting = Box::pin(state.balancer.select(
            &state,
            Some("session"),
            None,
            test_downstream(),
        ));
        assert!(poll!(&mut waiting).is_pending());
        sqlx::query("UPDATE providers SET api_key='sk-new' WHERE id='a'")
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
        drop(first);
        let current = waiting.await.unwrap();
        assert_eq!(current.provider.id, "a");
        assert_eq!(current.api_key, "sk-new");
        let mut waiting = Box::pin(state.balancer.select(
            &state,
            Some("session"),
            None,
            test_downstream(),
        ));
        assert!(poll!(&mut waiting).is_pending());
        state.balancer.forget_provider("a");
        drop(current);
        let rerouted = waiting.await.unwrap();
        assert_eq!(rerouted.provider.id, "b");
        assert_eq!(state.balancer.load("a"), ProviderLoad::default());
        assert!(
            state
                .balancer
                .select_provider("a", test_downstream())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn persistent_affinity_reuses_provider_and_cooldown_reallocates() {
        let state = crate::test_state("http://upstream.invalid").await;
        seed_test_owner(&state).await;
        let now = chrono::Utc::now().timestamp();
        for (id, created) in [("provider-a", now), ("provider-b", now + 1)] {
            sqlx::query("INSERT INTO providers(id,name,api_key,status,owner_id,created_at,updated_at) VALUES(?,?,?,'active',?,?,?)")
                .bind(id)
                .bind(id)
                .bind(format!("sk-{id}"))
                .bind(TEST_OWNER)
                .bind(created)
                .bind(created)
                .execute(&state.db)
                .await
                .unwrap();
        }
        state.balancer.reload_providers(&state.db).await.unwrap();
        let first = state
            .balancer
            .select(&state, Some("session-1"), None, test_downstream())
            .await
            .unwrap();
        let first_id = first.provider.id.clone();
        drop(first);
        let sticky = state
            .balancer
            .select(&state, Some("session-1"), None, test_downstream())
            .await
            .unwrap();
        assert_eq!(sticky.provider.id, first_id);
        drop(sticky);
        sqlx::query("UPDATE providers SET status='cooldown',cooldown_until=? WHERE id=?")
            .bind(now + 60)
            .bind(&first_id)
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
        let reallocated = state
            .balancer
            .select(&state, Some("session-1"), None, test_downstream())
            .await
            .unwrap();
        assert_ne!(reallocated.provider.id, first_id);
    }

    #[tokio::test]
    async fn rate_limit_cooldown_is_persisted_and_cleared_after_the_window() {
        let state = crate::test_state("http://upstream.invalid").await;
        let now = chrono::Utc::now().timestamp();
        seed_test_owner(&state).await;
        seed_provider(&state, "provider", "sk-provider", TEST_OWNER).await;
        state.balancer.reload_providers(&state.db).await.unwrap();

        let body = serde_json::to_vec(&json!({
            "error": {
                "message": "You are sending requests too quickly.",
                "type": "rate_limit_reached",
            }
        }))
        .unwrap();
        track_response(&state, "provider", StatusCode::TOO_MANY_REQUESTS, &body)
            .await
            .unwrap();
        track_response(&state, "provider", StatusCode::OK, &[])
            .await
            .unwrap();
        state
            .balancer
            .maintain(&state.db, &state.write_gate)
            .await
            .unwrap();

        let (status, last_error, cooldown_until): (String, Option<String>, Option<i64>) =
            sqlx::query_as(
                "SELECT status,last_error,cooldown_until FROM providers WHERE id='provider'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(status, "cooldown");
        assert_eq!(last_error.as_deref(), Some("rate limit reached"));
        let cooldown_until = cooldown_until.expect("a 429 opens a cooldown window");
        assert!(
            (now + RATE_LIMIT_COOLDOWN_SECONDS..=now + RATE_LIMIT_COOLDOWN_SECONDS + 1)
                .contains(&cooldown_until),
            "unexpected cooldown window: {cooldown_until}"
        );

        sqlx::query("UPDATE providers SET cooldown_until=?,updated_at=? WHERE id='provider'")
            .bind(now - 1)
            .bind(now + 1)
            .execute(&state.db)
            .await
            .unwrap();
        state
            .balancer
            .maintain(&state.db, &state.write_gate)
            .await
            .unwrap();

        let (status, cooldown_until, last_error): (String, Option<i64>, Option<String>) =
            sqlx::query_as(
                "SELECT status,cooldown_until,last_error FROM providers WHERE id='provider'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(status, "active");
        assert_eq!(cooldown_until, None);
        assert_eq!(last_error, None);
    }

    #[tokio::test]
    async fn insufficient_balance_marks_provider_balance_error() {
        let state = crate::test_state("http://upstream.invalid").await;
        seed_test_owner(&state).await;
        seed_provider(&state, "provider", "sk-provider", TEST_OWNER).await;
        state.balancer.reload_providers(&state.db).await.unwrap();

        let body = serde_json::to_vec(&json!({
            "error": { "message": "Insufficient Balance", "type": "insufficient_balance" }
        }))
        .unwrap();
        track_response(&state, "provider", StatusCode::PAYMENT_REQUIRED, &body)
            .await
            .unwrap();
        state
            .balancer
            .maintain(&state.db, &state.write_gate)
            .await
            .unwrap();

        let status: String = sqlx::query_scalar("SELECT status FROM providers WHERE id='provider'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(status, "balance_error");
        assert!(
            state
                .balancer
                .select(&state, None, None, test_downstream())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn private_providers_serve_only_the_owner_consumers() {
        let state = crate::test_state("http://upstream.invalid").await;
        let now = chrono::Utc::now().timestamp();
        for owner_id in ["owner-a", "owner-b"] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,'user',?)")
                .bind(owner_id)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }
        for (provider_id, owner_id, visibility) in [
            ("provider-a", "owner-a", PROVIDER_VISIBILITY_PRIVATE),
            ("provider-b", "owner-b", PROVIDER_VISIBILITY_PUBLIC),
        ] {
            sqlx::query("INSERT INTO providers(id,name,api_key,status,created_at,updated_at,owner_id,visibility) VALUES(?,?,?,'active',?,?,?,?)")
                .bind(provider_id)
                .bind(provider_id)
                .bind(format!("sk-{provider_id}"))
                .bind(now)
                .bind(now)
                .bind(owner_id)
                .bind(visibility)
                .execute(&state.db)
                .await
                .unwrap();
        }
        state.balancer.reload_providers(&state.db).await.unwrap();

        async fn selected(state: &AppState, excluded: Option<&str>, owner_id: &str) -> String {
            let downstream = Downstream::new(owner_id);
            state
                .balancer
                .select(state, None, excluded, downstream)
                .await
                .expect("provider pool is never empty for this downstream")
                .provider
                .id
        }

        assert_eq!(
            state
                .balancer
                .select_provider("provider-a", Downstream::new("owner-a"))
                .await
                .unwrap()
                .provider
                .id,
            "provider-a"
        );
        assert!(
            state
                .balancer
                .select_provider("provider-a", Downstream::new("owner-b"))
                .await
                .is_err()
        );
        assert_eq!(selected(&state, None, "owner-a").await, "provider-a");
        assert_eq!(selected(&state, None, "owner-b").await, "provider-b");
        assert_eq!(selected(&state, None, "stranger").await, "provider-b");
        assert_eq!(
            selected(&state, Some("provider-a"), "owner-a").await,
            "provider-b"
        );
    }

    #[tokio::test]
    async fn deleted_provider_dirt_is_discarded_without_blocking_maintenance() {
        let pool = crate::db::connect_memory().await.unwrap();
        let balancer = Balancer::default();
        let now = chrono::Utc::now().timestamp();
        for id in ["deleted", "survivor"] {
            sqlx::query("INSERT INTO providers(id,name,api_key,status,created_at,updated_at) VALUES(?,?,?,'active',?,?)")
                .bind(id)
                .bind(id)
                .bind(format!("sk-{id}"))
                .bind(now)
                .bind(now)
                .execute(&pool)
                .await
                .unwrap();
        }
        balancer.reload_providers(&pool).await.unwrap();
        balancer.remember_affinity("before-delete", "deleted", now, 3_600);
        balancer.observe(
            "deleted",
            ProviderUpdate {
                status: Some("cooldown".to_owned()),
                cooldown_until: Some(now + 60),
                rate_limit_json: "before-delete".to_owned(),
                last_error: None,
                last_used_at: Some(now),
                updated_at: now,
            },
        );

        sqlx::query("UPDATE providers SET is_deleted=1 WHERE id='deleted'")
            .execute(&pool)
            .await
            .unwrap();
        balancer.forget_provider("deleted");
        assert!(balancer.affinities.is_empty());
        assert!(balancer.dirty_affinities.is_empty());
        assert!(!balancer.provider_updates.contains_key("deleted"));

        balancer.remember_affinity("concurrent-select", "deleted", now, 3_600);
        for (id, rate_limit_json) in [("deleted", "stale"), ("survivor", "committed")] {
            balancer.observe(
                id,
                ProviderUpdate {
                    status: Some("cooldown".to_owned()),
                    cooldown_until: Some(now + 60),
                    rate_limit_json: rate_limit_json.to_owned(),
                    last_error: None,
                    last_used_at: Some(now),
                    updated_at: now,
                },
            );
        }

        balancer
            .maintain(&pool, &crate::SqliteWriteGate::default())
            .await
            .unwrap();
        let deleted_affinities: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM affinities WHERE provider_id='deleted'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(deleted_affinities, 0);
        assert!(balancer.affinities.is_empty());
        assert!(balancer.dirty_affinities.is_empty());
        assert!(balancer.provider_updates.is_empty());
        let survivor: (String, String) =
            sqlx::query_as("SELECT status,rate_limit_json FROM providers WHERE id='survivor'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(survivor, ("cooldown".to_owned(), "committed".to_owned()));
    }
}
