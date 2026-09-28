use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use sqlx::{Sqlite, SqlitePool, Transaction};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

use crate::{SqliteWriteGate, config::Config};

const QUEUE_CAPACITY: usize = 4_096;
const BATCH_CAPACITY: usize = 128;
const QUEUE_BYTE_CAPACITY: usize = 64 * 1024 * 1024;
pub const ARCHIVE_BODY_LIMIT: usize = 2 * 1024 * 1024;
const RETRY_INITIAL_DELAY: Duration = Duration::from_millis(100);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(5);

static DROPPED_EVENTS: AtomicU64 = AtomicU64::new(0);
static DROPPED_ARCHIVES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct AuditEvent {
    pub id: String,
    pub request_id: String,
    pub thread_id: Option<String>,
    pub consumer_id: String,
    pub user_id: String,
    pub request_archive: bool,
    pub provider_id: Option<String>,
    pub affinity_hash: Option<String>,
    pub affinity_source: Option<String>,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    /// Whether the request started inside DeepSeek peak hours.
    pub peak: bool,
    pub status: i64,
    pub upstream_http_version: Option<String>,
    pub first_byte_latency_ms: Option<i64>,
    pub request_bytes: i64,
    pub response_bytes: i64,
    pub request_transport_bytes: i64,
    pub response_transport_bytes: i64,
    pub downstream_accept_encoding: Option<String>,
    pub downstream_content_encoding: Option<String>,
    pub upstream_accept_encoding: Option<String>,
    pub upstream_content_encoding: Option<String>,
    pub latency_ms: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub official_cost_usd_nanos: i64,
    pub actual_cost_usd_nanos: i64,
    pub price_multiplier_nanos: i64,
    pub error: Option<String>,
    pub error_code: Option<String>,
    pub client_ip: String,
    pub created_at: i64,
    pub request_headers_json: String,
    pub request_body: Vec<u8>,
    pub request_body_truncated: bool,
    pub upstream_request_headers_json: Option<String>,
    pub response_headers_json: Option<String>,
    pub response_body: Option<Vec<u8>>,
    pub response_body_truncated: bool,
}

#[derive(Clone)]
pub struct AuditWriter {
    sender: mpsc::Sender<QueuedAudit>,
    budget: Arc<Semaphore>,
}

pub(crate) struct AuditReservation {
    permit: mpsc::OwnedPermit<QueuedAudit>,
}

enum QueuedAudit {
    Event {
        event: Box<AuditEvent>,
        _archive_budget: Option<OwnedSemaphorePermit>,
    },
    ResponseTransport {
        id: String,
        bytes: i64,
        encoding: Option<String>,
        downstream_response_headers_json: Option<String>,
    },
}

impl AuditReservation {
    pub(crate) fn send(self, event: AuditEvent, archive_budget: Option<OwnedSemaphorePermit>) {
        self.permit.send(QueuedAudit::Event {
            event: Box::new(event),
            _archive_budget: archive_budget,
        });
    }
}

impl AuditWriter {
    pub(crate) fn new(
        pool: SqlitePool,
        config: Arc<ArcSwap<Config>>,
        write_gate: SqliteWriteGate,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
        tokio::spawn(run(pool.clone(), receiver, write_gate.clone()));
        tokio::spawn(cleanup(pool, config, write_gate));
        Self {
            sender,
            budget: Arc::new(Semaphore::new(QUEUE_BYTE_CAPACITY)),
        }
    }

    pub(crate) fn try_reserve(&self) -> Option<AuditReservation> {
        let permit = match self.sender.clone().try_reserve_owned() {
            Ok(permit) => permit,
            Err(_) => return dropped(),
        };
        Some(AuditReservation { permit })
    }

    pub(crate) fn try_reserve_archive(
        &self,
        response_body_limit: usize,
    ) -> Option<OwnedSemaphorePermit> {
        self.budget
            .clone()
            .try_acquire_many_owned((ARCHIVE_BODY_LIMIT + response_body_limit + 16 * 1024) as u32)
            .ok()
            .or_else(dropped_archive)
    }

    pub(crate) fn record_response_transport(
        &self,
        id: String,
        bytes: i64,
        encoding: Option<String>,
        downstream_response_headers_json: Option<String>,
    ) {
        if self
            .sender
            .try_send(QueuedAudit::ResponseTransport {
                id,
                bytes,
                encoding,
                downstream_response_headers_json,
            })
            .is_err()
        {
            let _ = dropped::<()>();
        }
    }
}

fn dropped<T>() -> Option<T> {
    let count = DROPPED_EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 || count.is_power_of_two() {
        tracing::warn!(
            dropped_events = count,
            "audit queue is full; dropping request audit"
        );
    }
    None
}

fn dropped_archive<T>() -> Option<T> {
    let count = DROPPED_ARCHIVES.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 || count.is_power_of_two() {
        tracing::warn!(
            dropped_archives = count,
            "audit archive memory budget is full; dropping request diagnostics"
        );
    }
    None
}

async fn run(
    pool: SqlitePool,
    mut receiver: mpsc::Receiver<QueuedAudit>,
    write_gate: SqliteWriteGate,
) {
    while let Some(first) = receiver.recv().await {
        let mut batch = Vec::with_capacity(BATCH_CAPACITY);
        batch.push(first);
        tokio::time::sleep(Duration::from_millis(5)).await;
        while batch.len() < BATCH_CAPACITY {
            match receiver.try_recv() {
                Ok(event) => batch.push(event),
                Err(_) => break,
            }
        }
        let mut retry_delay = RETRY_INITIAL_DELAY;
        loop {
            match persist(&pool, &write_gate, &batch).await {
                Ok(()) => break,
                Err(error) => {
                    tracing::error!(events = batch.len(), retry_ms = retry_delay.as_millis(), %error, "audit batch write failed; retrying");
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = retry_delay.saturating_mul(2).min(RETRY_MAX_DELAY);
                }
            }
        }
    }
}

async fn persist(
    pool: &SqlitePool,
    write_gate: &SqliteWriteGate,
    batch: &[QueuedAudit],
) -> Result<(), sqlx::Error> {
    let _write = write_gate.lock().await;
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut touched_keys = std::collections::HashSet::new();
    for queued in batch {
        match queued {
            QueuedAudit::Event { event, .. } => {
                insert(&mut transaction, event).await?;
                touched_keys.insert(event.consumer_id.as_str());
            }
            QueuedAudit::ResponseTransport {
                id,
                bytes,
                encoding,
                downstream_response_headers_json,
            } => {
                sqlx::query("UPDATE api_calls SET response_transport_bytes=?,downstream_content_encoding=? WHERE id=?")
                    .bind(bytes)
                    .bind(encoding)
                    .bind(id)
                    .execute(&mut *transaction)
                    .await?;
                if let Some(headers) = downstream_response_headers_json {
                    sqlx::query("UPDATE request_archives SET downstream_response_headers_json=? WHERE api_call_id=?")
                        .bind(headers)
                        .bind(id)
                        .execute(&mut *transaction)
                        .await?;
                }
            }
        }
    }
    let used_at = chrono::Utc::now().timestamp();
    for consumer_id in touched_keys {
        sqlx::query("UPDATE consumers SET last_used_at=? WHERE id=?")
            .bind(used_at)
            .bind(consumer_id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await
}

async fn insert(
    transaction: &mut Transaction<'_, Sqlite>,
    event: &AuditEvent,
) -> Result<(), sqlx::Error> {
    let official_cost_usd_nanos = event.official_cost_usd_nanos.max(0);
    let actual_cost_usd_nanos = event.actual_cost_usd_nanos.max(0);
    let (official_consumed_usd_before_nanos, actual_consumed_usd_before_nanos): (i64, i64) =
        sqlx::query_as(
            "SELECT official_consumed_usd_nanos,consumed_usd_nanos FROM users WHERE id=?",
        )
        .bind(&event.user_id)
        .fetch_one(&mut **transaction)
        .await?;
    let official_consumed_usd_after_nanos =
        official_consumed_usd_before_nanos.saturating_add(official_cost_usd_nanos);
    let actual_consumed_usd_after_nanos =
        actual_consumed_usd_before_nanos.saturating_add(actual_cost_usd_nanos);
    sqlx::query("UPDATE users SET official_consumed_usd_nanos=?,consumed_usd_nanos=? WHERE id=?")
        .bind(official_consumed_usd_after_nanos)
        .bind(actual_consumed_usd_after_nanos)
        .bind(&event.user_id)
        .execute(&mut **transaction)
        .await?;
    let (
        official_provided_usd_before_nanos,
        official_provided_usd_after_nanos,
        actual_provided_usd_before_nanos,
        actual_provided_usd_after_nanos,
    ) = provider_provided_balances(
        transaction,
        event.provider_id.as_deref(),
        official_cost_usd_nanos,
        actual_cost_usd_nanos,
    )
    .await?;

    sqlx::query(
        "INSERT INTO api_calls(id,request_id,thread_id,consumer_id,user_id,provider_id,affinity_hash,affinity_source,method,path,model,reasoning_effort,peak,status,first_byte_latency_ms,request_bytes,response_bytes,request_transport_bytes,response_transport_bytes,downstream_accept_encoding,downstream_content_encoding,upstream_accept_encoding,upstream_content_encoding,latency_ms,input_tokens,output_tokens,cached_tokens,official_cost_usd_nanos,actual_cost_usd_nanos,price_multiplier_nanos,official_consumed_usd_before_nanos,official_consumed_usd_after_nanos,actual_consumed_usd_before_nanos,actual_consumed_usd_after_nanos,official_provided_usd_before_nanos,official_provided_usd_after_nanos,actual_provided_usd_before_nanos,actual_provided_usd_after_nanos,error,client_ip,created_at,upstream_http_version,error_code) VALUES(
            ?,?,?,?,?,?,
            (SELECT id FROM providers WHERE id=?),
            ?,?,?,?,?,?,?,
            ?,?,?,?,?,?,?,
            ?,?,?,?,?,?,?,
            ?,?,?,?,?,?,?,
            ?,?,?,?,?,?,?,
            ?
        )",
    )
        .bind(&event.id)
        .bind(&event.request_id)
        .bind(&event.thread_id)
        .bind(&event.consumer_id)
        .bind(&event.user_id)
        .bind(&event.provider_id)
        .bind(&event.affinity_hash)
        .bind(&event.affinity_source)
        .bind(&event.method)
        .bind(&event.path)
        .bind(&event.model)
        .bind(&event.reasoning_effort)
        .bind(event.peak)
        .bind(event.status)
        .bind(event.first_byte_latency_ms)
        .bind(event.request_bytes)
        .bind(event.response_bytes)
        .bind(event.request_transport_bytes)
        .bind(event.response_transport_bytes)
        .bind(&event.downstream_accept_encoding)
        .bind(&event.downstream_content_encoding)
        .bind(&event.upstream_accept_encoding)
        .bind(&event.upstream_content_encoding)
        .bind(event.latency_ms)
        .bind(event.input_tokens)
        .bind(event.output_tokens)
        .bind(event.cached_tokens)
        .bind(official_cost_usd_nanos)
        .bind(actual_cost_usd_nanos)
        .bind(event.price_multiplier_nanos)
        .bind(official_consumed_usd_before_nanos)
        .bind(official_consumed_usd_after_nanos)
        .bind(actual_consumed_usd_before_nanos)
        .bind(actual_consumed_usd_after_nanos)
        .bind(official_provided_usd_before_nanos)
        .bind(official_provided_usd_after_nanos)
        .bind(actual_provided_usd_before_nanos)
        .bind(actual_provided_usd_after_nanos)
        .bind(&event.error)
        .bind(&event.client_ip)
        .bind(event.created_at)
        .bind(&event.upstream_http_version)
        .bind(&event.error_code)
        .execute(&mut **transaction)
        .await?;
    if event.request_archive {
        sqlx::query("INSERT INTO request_archives(api_call_id,request_headers_json,upstream_request_headers_json,request_body,request_body_truncated,response_headers_json,downstream_response_headers_json,response_body,response_body_truncated,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(&event.id)
            .bind(&event.request_headers_json)
            .bind(&event.upstream_request_headers_json)
            .bind(&event.request_body)
            .bind(event.request_body_truncated)
            .bind(&event.response_headers_json)
            .bind(Option::<String>::None)
            .bind(&event.response_body)
            .bind(event.response_body_truncated)
            .bind(event.created_at)
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

async fn provider_provided_balances(
    transaction: &mut Transaction<'_, Sqlite>,
    provider_id: Option<&str>,
    official_cost_usd_nanos: i64,
    actual_cost_usd_nanos: i64,
) -> Result<(i64, i64, i64, i64), sqlx::Error> {
    let Some(provider_id) = provider_id else {
        return Ok((0, 0, 0, 0));
    };
    let Some((owner_id, official_before, actual_before)) = sqlx::query_as::<_, (Option<String>, i64, i64)>(
        "SELECT owner_id,official_provided_usd_nanos,actual_provided_usd_nanos FROM providers WHERE id=?",
    )
    .bind(provider_id)
    .fetch_optional(&mut **transaction)
    .await?
    else {
        return Ok((0, 0, 0, 0));
    };
    let official_after = official_before.saturating_add(official_cost_usd_nanos);
    let actual_after = actual_before.saturating_add(actual_cost_usd_nanos);
    sqlx::query(
        "UPDATE providers SET official_provided_usd_nanos=?,actual_provided_usd_nanos=? WHERE id=?",
    )
    .bind(official_after)
    .bind(actual_after)
    .bind(provider_id)
    .execute(&mut **transaction)
    .await?;
    if let Some(owner_id) = owner_id {
        let provided_before: i64 =
            sqlx::query_scalar("SELECT provided_usd_nanos FROM users WHERE id=?")
                .bind(&owner_id)
                .fetch_one(&mut **transaction)
                .await?;
        sqlx::query("UPDATE users SET provided_usd_nanos=? WHERE id=?")
            .bind(provided_before.saturating_add(actual_cost_usd_nanos))
            .bind(owner_id)
            .execute(&mut **transaction)
            .await?;
    }
    Ok((official_before, official_after, actual_before, actual_after))
}

async fn cleanup(pool: SqlitePool, config: Arc<ArcSwap<Config>>, write_gate: SqliteWriteGate) {
    let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
    loop {
        interval.tick().await;
        if let Err(error) = delete_expired(
            &pool,
            &write_gate,
            config.load().request_archive_retention_days,
            chrono::Utc::now().timestamp(),
        )
        .await
        {
            tracing::error!(%error, "request archive cleanup failed");
        }
    }
}

async fn delete_expired(
    pool: &SqlitePool,
    write_gate: &SqliteWriteGate,
    retention_days: i64,
    now: i64,
) -> Result<u64, sqlx::Error> {
    let _write = write_gate.lock().await;
    let cutoff = now - retention_days * 24 * 60 * 60;
    Ok(
        sqlx::query("DELETE FROM request_archives WHERE created_at<?")
            .bind(cutoff)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(
        id: &str,
        user_id: &str,
        consumer_id: &str,
        official_cost_usd_nanos: i64,
        actual_cost_usd_nanos: i64,
    ) -> AuditEvent {
        AuditEvent {
            id: id.to_owned(),
            request_id: id.to_owned(),
            thread_id: None,
            consumer_id: consumer_id.to_owned(),
            user_id: user_id.to_owned(),
            request_archive: false,
            provider_id: None,
            affinity_hash: None,
            affinity_source: None,
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            model: Some("deepseek-flash".to_owned()),
            reasoning_effort: None,
            peak: false,
            status: 200,
            upstream_http_version: None,
            first_byte_latency_ms: None,
            request_bytes: 0,
            response_bytes: 0,
            request_transport_bytes: 0,
            response_transport_bytes: 0,
            downstream_accept_encoding: None,
            downstream_content_encoding: None,
            upstream_accept_encoding: None,
            upstream_content_encoding: None,
            latency_ms: 1,
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
            official_cost_usd_nanos,
            actual_cost_usd_nanos,
            price_multiplier_nanos: 100_000_000,
            error: None,
            error_code: None,
            client_ip: "127.0.0.1".to_owned(),
            created_at: 1,
            request_headers_json: "[]".to_owned(),
            request_body: Vec::new(),
            request_body_truncated: false,
            upstream_request_headers_json: None,
            response_headers_json: None,
            response_body: None,
            response_body_truncated: false,
        }
    }

    async fn seed_user_and_consumer(pool: &SqlitePool) {
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('user','user',0)")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('key','user','key','sk-test','hash',0)")
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn audit_persists_the_peak_tariff_flag() {
        let pool = crate::db::connect_memory().await.unwrap();
        seed_user_and_consumer(&pool).await;
        let mut transaction = pool.begin().await.unwrap();
        let mut peak = event("peak", "user", "key", 10, 1);
        peak.peak = true;
        insert(&mut transaction, &peak).await.unwrap();
        insert(&mut transaction, &event("off-peak", "user", "key", 10, 1))
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        let rows: Vec<(String, i64)> = sqlx::query_as("SELECT id,peak FROM api_calls ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(
            rows,
            vec![("off-peak".to_owned(), 0), ("peak".to_owned(), 1)]
        );
    }

    #[tokio::test]
    async fn audit_persists_nonnegative_user_consumption_and_per_call_balances() {
        let pool = crate::db::connect_memory().await.unwrap();
        seed_user_and_consumer(&pool).await;
        let mut transaction = pool.begin().await.unwrap();
        insert(&mut transaction, &event("first", "user", "key", 125, 12))
            .await
            .unwrap();
        insert(&mut transaction, &event("second", "user", "key", 0, 0))
            .await
            .unwrap();
        insert(&mut transaction, &event("third", "user", "key", -10, -5))
            .await
            .unwrap();
        insert(&mut transaction, &event("fourth", "user", "key", 75, 7))
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        let balances: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT id,official_consumed_usd_before_nanos,official_consumed_usd_after_nanos,actual_consumed_usd_before_nanos,actual_consumed_usd_after_nanos FROM api_calls ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            balances,
            vec![
                ("first".to_owned(), 0, 125, 0, 12),
                ("fourth".to_owned(), 125, 200, 12, 19),
                ("second".to_owned(), 125, 125, 12, 12),
                ("third".to_owned(), 125, 125, 12, 12),
            ]
        );
        let consumed_usd_nanos: (i64, i64) = sqlx::query_as(
            "SELECT official_consumed_usd_nanos,consumed_usd_nanos FROM users WHERE id='user'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(consumed_usd_nanos, (200, 19));
    }

    #[tokio::test]
    async fn audit_aggregates_nonnegative_provider_value_for_the_provider_owner() {
        let pool = crate::db::connect_memory().await.unwrap();
        for user_id in ["caller", "owner"] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,'user',0)")
                .bind(user_id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('key','caller','key','sk-test','hash',0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO providers(id,name,api_key,owner_id,created_at,updated_at) VALUES('provider','provider','sk-deepseek','owner',0,0)")
            .execute(&pool)
            .await
            .unwrap();
        let mut transaction = pool.begin().await.unwrap();
        for (id, official_cost_usd_nanos, actual_cost_usd_nanos) in
            [("first", 100, 12), ("second", 50, 7), ("third", -10, -5)]
        {
            let mut event = event(
                id,
                "caller",
                "key",
                official_cost_usd_nanos,
                actual_cost_usd_nanos,
            );
            event.provider_id = Some("provider".to_owned());
            insert(&mut transaction, &event).await.unwrap();
        }
        transaction.commit().await.unwrap();

        let balances: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT id,official_provided_usd_before_nanos,official_provided_usd_after_nanos,actual_provided_usd_before_nanos,actual_provided_usd_after_nanos FROM api_calls ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            balances,
            vec![
                ("first".to_owned(), 0, 100, 0, 12),
                ("second".to_owned(), 100, 150, 12, 19),
                ("third".to_owned(), 150, 150, 19, 19),
            ]
        );
        let provider: (i64, i64) = sqlx::query_as(
            "SELECT official_provided_usd_nanos,actual_provided_usd_nanos FROM providers WHERE id='provider'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(provider, (150, 19));
        let owner_provided: i64 =
            sqlx::query_scalar("SELECT provided_usd_nanos FROM users WHERE id='owner'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let caller_provided: i64 =
            sqlx::query_scalar("SELECT provided_usd_nanos FROM users WHERE id='caller'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(owner_provided, 19);
        assert_eq!(caller_provided, 0);
    }

    #[tokio::test]
    async fn response_transport_queues_bytes_and_archived_headers() {
        let pool = crate::db::connect_memory().await.unwrap();
        seed_user_and_consumer(&pool).await;
        let write_gate = SqliteWriteGate::default();
        let mut archived = event("archived", "user", "key", 0, 0);
        archived.request_archive = true;
        archived.request_headers_json =
            r#"[["authorization","Bearer sk-secret"],["accept","*/*"]]"#.to_owned();
        let batch = vec![
            QueuedAudit::Event {
                event: Box::new(archived),
                _archive_budget: None,
            },
            QueuedAudit::ResponseTransport {
                id: "archived".to_owned(),
                bytes: 4_096,
                encoding: Some("gzip".to_owned()),
                downstream_response_headers_json: Some(
                    r#"[["content-encoding","gzip"]]"#.to_owned(),
                ),
            },
        ];
        persist(&pool, &write_gate, &batch).await.unwrap();

        let (bytes, encoding): (i64, Option<String>) = sqlx::query_as(
            "SELECT response_transport_bytes,downstream_content_encoding FROM api_calls WHERE id='archived'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(bytes, 4_096);
        assert_eq!(encoding.as_deref(), Some("gzip"));
        let headers: Option<String> = sqlx::query_scalar(
            "SELECT downstream_response_headers_json FROM request_archives WHERE api_call_id='archived'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(headers.as_deref(), Some(r#"[["content-encoding","gzip"]]"#));
    }

    #[tokio::test]
    async fn retention_cleanup_removes_only_expired_diagnostics() {
        let pool = crate::db::connect_memory().await.unwrap();
        seed_user_and_consumer(&pool).await;
        for (id, created_at) in [("expired", 0_i64), ("retained", 200_000_i64)] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,created_at) VALUES(?,?, 'key','user','POST','/v1/chat/completions',200,1,?)")
                .bind(id)
                .bind(id)
                .bind(created_at)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO request_archives(api_call_id,request_headers_json,request_body,request_body_truncated,response_body_truncated,created_at) VALUES(?,'[]',X'',0,0,?)")
                .bind(id)
                .bind(created_at)
                .execute(&pool)
                .await
                .unwrap();
        }

        assert_eq!(
            delete_expired(&pool, &SqliteWriteGate::default(), 1, 200_000)
                .await
                .unwrap(),
            1
        );
        let ids: Vec<String> = sqlx::query_scalar("SELECT api_call_id FROM request_archives")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(ids, vec!["retained"]);
    }

    #[tokio::test]
    async fn audit_and_provider_maintenance_share_the_write_gate() {
        let path = std::env::temp_dir().join(format!(
            "deepseek-lb-audit-maintenance-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::connect_test_file(&path).await.unwrap();
        seed_user_and_consumer(&pool).await;

        let write_gate = SqliteWriteGate::default();
        let batch = vec![QueuedAudit::Event {
            event: Box::new(event("call", "user", "key", 1, 1)),
            _archive_budget: None,
        }];
        let balancer = crate::balancer::Balancer::default();
        let (audit, maintenance) = tokio::join!(
            persist(&pool, &write_gate, &batch),
            balancer.maintain(&pool, &write_gate)
        );
        audit.unwrap();
        maintenance.unwrap();

        let calls: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_calls")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(calls, 1);
        pool.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
