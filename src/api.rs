use std::{collections::HashMap, net::SocketAddr, time::Duration};

use axum::{
    Json,
    extract::{ConnectInfo, Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Row, Sqlite};
use uuid::Uuid;

use crate::{
    AppError, AppState,
    auth::{UserIdentity, bearer, is_admin, require_admin, require_root},
    balancer::{self, Provider},
    config,
    crypto::consumer_secret_hash,
    midas, payments, pricing,
    resources::SystemResourcesSnapshot,
};

const PROVIDER_TEST_TIMEOUT: Duration = Duration::from_secs(30);
const PROVIDER_AUXILIARY_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn public_config(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let setup_required = setup_required(&state).await?;
    let config = state.config.load_full();
    Ok(Json(json!({
        "setup_required": setup_required,
        "auth_issuer": (!setup_required).then(|| config.auth_issuer.clone()).flatten(),
        "auth_audience": (!setup_required).then(|| config.auth_audience.clone()).flatten()
    })))
}

pub async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}

pub async fn payment_summary(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    let (official_consumed_usd_nanos, consumed_usd_nanos, provided_usd_nanos, allow_debt): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT official_consumed_usd_nanos,consumed_usd_nanos,provided_usd_nanos,allow_debt FROM users WHERE id=?",
    )
    .bind(&user.id)
    .fetch_one(&state.db)
    .await?;
    let config = state.config.load_full();
    let topup_usd_nanos = if midas::is_configured(&config) {
        midas::inbound_transfer_totals(&state, std::slice::from_ref(&user.id))
            .await?
            .remove(&user.id)
            .expect("Midas returns every requested user")
    } else {
        0
    };
    Ok(Json(json!({
        "topup_usd_nanos": topup_usd_nanos,
        "available_usd_nanos": topup_usd_nanos.saturating_add(provided_usd_nanos).saturating_sub(consumed_usd_nanos),
        "official_consumed_usd_nanos": official_consumed_usd_nanos,
        "consumed_usd_nanos": consumed_usd_nanos,
        "provided_usd_nanos": provided_usd_nanos,
        "enforcement_enabled": allow_debt == 0 && !config.allow_all_users_debt,
        "midas_configured": midas::is_configured(&config),
        "midas_fund_user_id": config.midas_fund_user_id
    })))
}

pub async fn model_prices(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.load();
    Json(json!({
        "source_url": "https://api-docs.deepseek.com/quick_start/pricing",
        "source_as_of": "2026-09-20",
        "unit": "USD per 1M tokens",
        "rows": pricing::official_model_prices(&config.available_model_ids)
    }))
}

pub async fn midas_settings(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    require_root(&user)?;
    let config = state.config.load();
    Ok(Json(json!({
        "midas_api_base": config.midas_api_base,
        "midas_fund_user_id": config.midas_fund_user_id,
        "midas_fund_api_key_configured": config.midas_fund_api_key.is_some()
    })))
}

#[derive(Deserialize)]
pub struct UpdateMidasSettings {
    midas_api_base: String,
    midas_fund_user_id: String,
    midas_fund_api_key: String,
}

pub async fn update_midas_settings(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(root): Extension<UserIdentity>,
    Json(input): Json<UpdateMidasSettings>,
) -> Result<Json<Value>, AppError> {
    require_root(&root)?;
    let api_base = validate_http_url("Midas API base", &input.midas_api_base)?;
    let current = state.config.load_full();
    let fund_user_id = if input.midas_fund_user_id.trim().is_empty() {
        current
            .midas_fund_user_id
            .clone()
            .ok_or_else(|| AppError::bad_request("Midas public wallet user ID is required"))?
    } else {
        let fund_user_id =
            required_setting("Midas public wallet user ID", &input.midas_fund_user_id)?;
        Uuid::parse_str(&fund_user_id)
            .map_err(|_| AppError::bad_request("Midas public wallet user ID must be a UUID"))?;
        fund_user_id
    };
    let api_key = if input.midas_fund_api_key.trim().is_empty() {
        current
            .midas_fund_api_key
            .clone()
            .ok_or_else(|| AppError::bad_request("Midas fund API key is required"))?
    } else {
        required_setting("Midas fund API key", &input.midas_fund_api_key)?
    };
    let now = chrono::Utc::now().timestamp();
    let values = [
        ("midas_api_base", api_base.clone()),
        ("midas_fund_user_id", fund_user_id.clone()),
        ("midas_fund_api_key", api_key.clone()),
    ];
    {
        let _write = state.write_gate.lock().await;
        let mut transaction = state.db.begin().await?;
        for (key, value) in values {
            sqlx::query("UPDATE app_meta SET value=?,updated_at=? WHERE key=?")
                .bind(value)
                .bind(now)
                .bind(key)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
    }
    let mut config = (*current).clone();
    config.midas_api_base = api_base;
    config.midas_fund_user_id = Some(fund_user_id);
    config.midas_fund_api_key = Some(api_key);
    state.config.store(std::sync::Arc::new(config));
    write_admin_audit(
        &state,
        &root.id,
        "payments.midas.settings.update",
        None,
        &peer.ip().to_string(),
    )
    .await?;
    Ok(Json(json!({"ok": true})))
}

pub async fn setup_status(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let required = setup_required(&state).await?;
    let config = state.config.load();
    Ok(Json(json!({
        "setup_required": required,
        "auth_issuer": (!required).then(|| config.auth_issuer.clone()).flatten(),
        "auth_audience": (!required).then(|| config.auth_audience.clone()).flatten()
    })))
}

#[derive(Deserialize)]
pub struct SetupInput {
    auth_issuer: String,
    auth_audience: String,
}

pub async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SetupInput>,
) -> Result<Json<Value>, AppError> {
    if !setup_required(&state).await? {
        return Err(AppError::not_found("setup is already complete"));
    }
    let issuer = input.auth_issuer.trim().trim_end_matches('/').to_owned();
    let audience = required_setting("Auth Mini audience", &input.auth_audience)?.to_owned();
    let (identity, verifier) = state
        .auth
        .verify_candidate(issuer.clone(), audience.clone(), bearer(&headers)?)
        .await?;
    let now = chrono::Utc::now().timestamp();
    {
        let _write = state.write_gate.lock().await;
        let mut transaction = state.db.begin_with("BEGIN IMMEDIATE").await?;
        let complete: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='setup_complete'")
                .fetch_one(&mut *transaction)
                .await?;
        if complete == "true" {
            return Err(AppError::not_found("setup is already complete"));
        }
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,'root',?) ON CONFLICT(id) DO UPDATE SET role='root'")
            .bind(&identity.id)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE providers SET owner_id=? WHERE owner_id IS NULL")
            .bind(&identity.id)
            .execute(&mut *transaction)
            .await?;
        for (key, value) in [
            ("auth_issuer", issuer.as_str()),
            ("auth_audience", audience.as_str()),
            ("setup_complete", "true"),
        ] {
            sqlx::query("UPDATE app_meta SET value=?,updated_at=? WHERE key=?")
                .bind(value)
                .bind(now)
                .bind(key)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
    }
    state.auth.install(verifier).await;
    let mut config = (**state.config.load()).clone();
    config.setup_complete = true;
    config.auth_issuer = Some(issuer);
    config.auth_audience = Some(audience);
    state.config.store(std::sync::Arc::new(config));
    Ok(Json(json!({"ok":true,"root_user_id":identity.id})))
}

async fn setup_required(state: &AppState) -> Result<bool, AppError> {
    let value: String = sqlx::query_scalar("SELECT value FROM app_meta WHERE key='setup_complete'")
        .fetch_one(&state.db)
        .await?;
    Ok(value != "true")
}

pub async fn me(Extension(user): Extension<UserIdentity>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({"id":user.id,"role":user.role})))
}

#[derive(Deserialize)]
pub struct CreateConsumer {
    name: String,
    #[serde(default)]
    request_archive: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyConsumerCredential {
    secret: String,
}

pub async fn verify_consumer_credential(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
    Json(input): Json<VerifyConsumerCredential>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        "SELECT secret_hash=?,is_disabled,request_archive FROM consumers WHERE id=? AND user_id=? AND is_system=0 AND is_deleted=0",
    )
    .bind(consumer_secret_hash(&input.secret))
    .bind(&id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("consumer not found"))?;
    Ok(Json(json!({
        "id": id,
        "credential_matches": row.get::<i64, _>(0) != 0,
        "is_disabled": row.get::<i64, _>(1) != 0,
        "request_archive": row.get::<i64, _>(2) != 0,
    })))
}

#[derive(Deserialize)]
pub struct UpdateConsumer {
    name: Option<String>,
    request_archive: Option<bool>,
    is_disabled: Option<bool>,
}

fn consumer_name(value: &str) -> Result<&str, AppError> {
    let name = value.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::bad_request(
            "consumer name must be 1-80 characters",
        ));
    }
    Ok(name)
}

pub async fn list_consumers(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query("SELECT id,name,prefix,created_at,last_used_at,request_archive,is_disabled FROM consumers WHERE user_id=? AND is_system=0 AND is_deleted=0 ORDER BY created_at DESC")
        .bind(&user.id).fetch_all(&state.db).await?;
    Ok(Json(Value::Array(rows.into_iter().map(|row| json!({
        "id": row.get::<String,_>(0), "name": row.get::<String,_>(1), "prefix": row.get::<String,_>(2),
        "created_at": row.get::<i64,_>(3), "last_used_at": row.get::<Option<i64>,_>(4),
        "request_archive": row.get::<i64,_>(5) != 0,
        "is_disabled": row.get::<i64,_>(6) != 0
    })).collect())))
}

fn new_consumer_credential() -> (String, String) {
    let mut random = [0_u8; 32];
    rand::rng().fill_bytes(&mut random);
    let secret = format!("sk-{}", URL_SAFE_NO_PAD.encode(random));
    let prefix = secret.chars().take(11).collect::<String>();
    (secret, prefix)
}

pub async fn create_consumer(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Json(input): Json<CreateConsumer>,
) -> Result<Json<Value>, AppError> {
    let name = consumer_name(&input.name)?;
    let (secret, prefix) = new_consumer_credential();
    let id = Uuid::new_v4().to_string();
    let _write = state.write_gate.lock().await;
    sqlx::query(
        "INSERT INTO consumers(id,user_id,name,prefix,secret_hash,request_archive,created_at) VALUES(?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(name)
    .bind(&prefix)
    .bind(consumer_secret_hash(&secret))
    .bind(input.request_archive)
    .bind(chrono::Utc::now().timestamp())
    .execute(&state.db)
    .await?;
    Ok(Json(
        json!({"id":id,"name":name,"prefix":prefix,"secret":secret,"request_archive":input.request_archive,"is_disabled":false}),
    ))
}

pub async fn update_consumer(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
    Json(input): Json<UpdateConsumer>,
) -> Result<Json<Value>, AppError> {
    if input.name.is_none() && input.request_archive.is_none() && input.is_disabled.is_none() {
        return Err(AppError::bad_request("consumer update is empty"));
    }
    let current = sqlx::query(
        "SELECT name,request_archive,is_disabled FROM consumers WHERE id=? AND user_id=? AND is_system=0 AND is_deleted=0",
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("consumer not found"))?;
    let name = match input.name {
        Some(name) => consumer_name(&name)?.to_owned(),
        None => current.get(0),
    };
    let request_archive = input
        .request_archive
        .unwrap_or_else(|| current.get::<i64, _>(1) != 0);
    let is_disabled = input
        .is_disabled
        .unwrap_or_else(|| current.get::<i64, _>(2) != 0);
    let _write = state.write_gate.lock().await;
    let updated = sqlx::query(
        "UPDATE consumers SET name=?,request_archive=?,is_disabled=? WHERE id=? AND user_id=? AND is_deleted=0",
    )
    .bind(&name)
    .bind(request_archive)
    .bind(is_disabled)
    .bind(&id)
    .bind(&user.id)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::not_found("consumer not found"));
    }
    Ok(Json(
        json!({"id":id,"name":name,"request_archive":request_archive,"is_disabled":is_disabled}),
    ))
}

pub async fn rotate_consumer(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let (secret, prefix) = new_consumer_credential();
    let _write = state.write_gate.lock().await;
    let result = sqlx::query("UPDATE consumers SET prefix=?,secret_hash=? WHERE id=? AND user_id=? AND is_system=0 AND is_deleted=0")
        .bind(&prefix)
        .bind(consumer_secret_hash(&secret))
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("consumer not found"));
    }
    Ok(Json(json!({"id":id,"prefix":prefix,"secret":secret})))
}

pub async fn delete_consumer(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let _write = state.write_gate.lock().await;
    let result = sqlx::query("UPDATE consumers SET is_deleted=1 WHERE id=? AND user_id=? AND is_system=0 AND is_deleted=0")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("consumer not found"));
    }
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
pub struct CreateProvider {
    #[serde(default)]
    name: String,
    api_key: String,
    #[serde(default)]
    visibility: Option<String>,
}

pub async fn list_providers(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    let providers = if is_admin(&user) {
        sqlx::query_as::<_, Provider>(
            "SELECT p.* FROM providers p WHERE p.is_deleted=0 ORDER BY p.created_at DESC",
        )
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, Provider>(
            "SELECT p.* FROM providers p WHERE p.owner_id=? AND p.is_deleted=0 ORDER BY p.created_at DESC",
        )
        .bind(&user.id)
        .fetch_all(&state.db)
        .await?
    };
    let scope = if is_admin(&user) {
        ProviderUsageScope::All
    } else {
        ProviderUsageScope::Owner(&user.id)
    };
    let usage = provider_usage(&state, scope).await?;
    let concurrency_limit = state.config.load().provider_concurrency_limit;
    Ok(Json(Value::Array(
        providers
            .into_iter()
            .map(|provider| {
                let load = state.balancer.load(&provider.id);
                let mut value = serde_json::to_value(provider).expect("provider serializes");
                value["inflight"] = json!(load.inflight);
                value["queued"] = json!(load.queued);
                value["concurrency_limit"] = json!(concurrency_limit);
                value["usage"] = value["id"]
                    .as_str()
                    .and_then(|id| usage.get(id))
                    .map(|usage| serde_json::to_value(usage).expect("usage serializes"))
                    .unwrap_or_else(|| json!(null));
                value
            })
            .collect(),
    )))
}

#[derive(Serialize, Default)]
struct ProviderUsage {
    requests: i64,
    errors: i64,
    peak_requests: i64,
    input_tokens: i64,
    output_tokens: i64,
    cached_tokens: i64,
    actual_cost_usd_nanos: i64,
}

enum ProviderUsageScope<'a> {
    All,
    Owner(&'a str),
}

async fn provider_usage(
    state: &AppState,
    scope: ProviderUsageScope<'_>,
) -> Result<HashMap<String, ProviderUsage>, AppError> {
    let since = chrono::Utc::now().timestamp() - 7 * 24 * 60 * 60;
    let sql = "SELECT c.provider_id,COUNT(*),COALESCE(SUM(CASE WHEN c.error IS NOT NULL OR c.error_code IS NOT NULL OR c.status>=400 THEN 1 ELSE 0 END),0),COALESCE(SUM(c.peak),0),COALESCE(SUM(c.input_tokens),0),COALESCE(SUM(c.output_tokens),0),COALESCE(SUM(c.cached_tokens),0),COALESCE(SUM(c.actual_cost_usd_nanos),0) FROM api_calls c JOIN providers p ON p.id=c.provider_id WHERE c.provider_id IS NOT NULL AND c.created_at>=?";
    let rows = match scope {
        ProviderUsageScope::All => {
            sqlx::query(&format!("{sql} GROUP BY c.provider_id"))
                .bind(since)
                .fetch_all(&state.db)
                .await?
        }
        ProviderUsageScope::Owner(owner_id) => {
            sqlx::query(&format!("{sql} AND p.owner_id=? GROUP BY c.provider_id"))
                .bind(since)
                .bind(owner_id)
                .fetch_all(&state.db)
                .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>(0),
                ProviderUsage {
                    requests: row.get(1),
                    errors: row.get(2),
                    peak_requests: row.get(3),
                    input_tokens: row.get(4),
                    output_tokens: row.get(5),
                    cached_tokens: row.get(6),
                    actual_cost_usd_nanos: row.get(7),
                },
            )
        })
        .collect())
}

pub async fn create_provider(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Json(input): Json<CreateProvider>,
) -> Result<Json<Value>, AppError> {
    let result = insert_provider(
        &state,
        &user.id,
        &input.name,
        &input.api_key,
        &balancer::provider_visibility(input.visibility.as_deref())?,
    )
    .await;
    let action = if result.is_ok() {
        "provider.create"
    } else {
        "provider.create.failed"
    };
    write_admin_audit(
        &state,
        &user.id,
        action,
        result
            .as_ref()
            .ok()
            .and_then(|value| value.0.get("id"))
            .and_then(Value::as_str),
        &peer.ip().to_string(),
    )
    .await?;
    result
}

async fn insert_provider(
    state: &AppState,
    owner_id: &str,
    name: &str,
    api_key: &str,
    visibility: &str,
) -> Result<Json<Value>, AppError> {
    let api_key = provider_api_key(api_key)?;
    let id = Uuid::new_v4().to_string();
    let name = provider_name(name, &id);
    let now = chrono::Utc::now().timestamp();
    {
        let _write = state.write_gate.lock().await;
        sqlx::query("INSERT INTO providers(id,name,api_key,status,created_at,updated_at,owner_id,visibility) VALUES(?,?,?,'active',?,?,?,?)")
            .bind(&id)
            .bind(&name)
            .bind(&api_key)
            .bind(now)
            .bind(now)
            .bind(owner_id)
            .bind(visibility)
            .execute(&state.db)
            .await?;
    }
    state.balancer.reload_providers(&state.db).await?;
    Ok(Json(json!({
        "id": id,
        "name": name,
        "owner_id": owner_id,
        "status": "active",
        "visibility": visibility
    })))
}

fn provider_api_key(api_key: &str) -> Result<String, AppError> {
    let api_key = api_key.trim();
    if api_key.is_empty() || api_key.len() > 256 {
        return Err(AppError::bad_request("api_key must be 1-256 characters"));
    }
    Ok(api_key.to_owned())
}

fn provider_name(name: &str, id: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        id.to_owned()
    } else {
        name.to_owned()
    }
}

async fn require_provider_manager(
    state: &AppState,
    user: &UserIdentity,
    provider_id: &str,
) -> Result<(), AppError> {
    if is_admin(user) {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM providers WHERE id=? AND is_deleted=0)",
        )
        .bind(provider_id)
        .fetch_one(&state.db)
        .await?;
        return exists
            .then_some(())
            .ok_or_else(|| AppError::not_found("provider not found"));
    }
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM providers WHERE id=? AND owner_id=? AND is_deleted=0)",
    )
    .bind(provider_id)
    .bind(&user.id)
    .fetch_one(&state.db)
    .await?;
    owned
        .then_some(())
        .ok_or_else(|| AppError::not_found("provider not found"))
}

#[derive(Deserialize)]
pub struct ProviderUpdate {
    name: Option<String>,
    enabled: Option<bool>,
    visibility: Option<String>,
}

pub async fn update_provider(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
    Json(input): Json<ProviderUpdate>,
) -> Result<Json<Value>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    let operation: Result<Json<Value>, AppError> = async {
        let visibility = input
            .visibility
            .as_deref()
            .map(|value| balancer::provider_visibility(Some(value)))
            .transpose()?;
        if input.name.is_some() || visibility.is_some() {
            let name = input.name.as_deref().map(|name| provider_name(name, &id));
            let _write = state.write_gate.lock().await;
            sqlx::query("UPDATE providers SET name=COALESCE(?,name),visibility=COALESCE(?,visibility),updated_at=? WHERE id=? AND is_deleted=0")
                .bind(name)
                .bind(visibility)
                .bind(chrono::Utc::now().timestamp())
                .bind(&id)
                .execute(&state.db)
                .await?;
        }
        if let Some(enabled) = input.enabled {
            let (disabled, status) = if enabled { (0, "active") } else { (1, "disabled") };
            let _write = state.write_gate.lock().await;
            sqlx::query("UPDATE providers SET manual_disabled=?,status=?,cooldown_until=NULL,last_error=NULL,updated_at=? WHERE id=? AND is_deleted=0")
                .bind(disabled).bind(status).bind(chrono::Utc::now().timestamp()).bind(&id).execute(&state.db).await?;
        }
        state.balancer.reload_providers(&state.db).await?;
        Ok(Json(json!({"ok":true})))
    }
    .await;
    let action = if operation.is_ok() {
        "provider.update"
    } else {
        "provider.update.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    operation
}

pub async fn read_provider_key(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<(HeaderMap, Json<Value>), AppError> {
    require_provider_manager(&state, &user, &id).await?;
    require_admin(&user)?;
    let result: Result<(HeaderMap, Json<Value>), AppError> = async {
        let row: (String, String) =
            sqlx::query_as("SELECT name,api_key FROM providers WHERE id=? AND is_deleted=0")
                .bind(&id)
                .fetch_optional(&state.db)
                .await?
                .ok_or_else(|| AppError::not_found("provider not found"))?;
        let mut response_headers = HeaderMap::new();
        response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        Ok((
            response_headers,
            Json(json!({"name":row.0,"api_key":row.1})),
        ))
    }
    .await;
    let action = if result.is_ok() {
        "provider.key.read"
    } else {
        "provider.key.read.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    result
}

#[derive(Deserialize)]
pub struct ReplaceProviderKey {
    name: Option<String>,
    api_key: String,
}

pub async fn replace_provider_key(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
    Json(input): Json<ReplaceProviderKey>,
) -> Result<Json<Value>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    require_admin(&user)?;
    let result = replace_provider_key_values(
        &state,
        &id,
        input.name.as_deref().map(str::trim),
        &input.api_key,
    )
    .await;
    let action = if result.is_ok() {
        "provider.key.update"
    } else {
        "provider.key.update.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    result
}

async fn replace_provider_key_values(
    state: &AppState,
    id: &str,
    name: Option<&str>,
    api_key: &str,
) -> Result<Json<Value>, AppError> {
    let api_key = provider_api_key(api_key)?;
    let name = match name {
        Some(name) => provider_name(name, id),
        None => sqlx::query_scalar("SELECT name FROM providers WHERE id=? AND is_deleted=0")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("provider not found"))?,
    };
    let updated = {
        let _write = state.write_gate.lock().await;
        sqlx::query("UPDATE providers SET name=?,api_key=?,status=CASE WHEN manual_disabled=1 THEN 'disabled' ELSE 'active' END,cooldown_until=NULL,last_error=NULL,updated_at=? WHERE id=? AND is_deleted=0")
            .bind(&name).bind(&api_key)
            .bind(chrono::Utc::now().timestamp()).bind(id).execute(&state.db).await?
    };
    if updated.rows_affected() == 0 {
        return Err(AppError::not_found("provider not found"));
    }
    state.balancer.reload_providers(&state.db).await?;
    Ok(Json(json!({"ok":true,"name":name})))
}

pub async fn delete_provider(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    let result: Result<Json<Value>, AppError> = async {
        let deleted = {
            let _write = state.write_gate.lock().await;
            sqlx::query("UPDATE providers SET is_deleted=1,status='disabled',manual_disabled=1,updated_at=? WHERE id=? AND is_deleted=0")
                .bind(chrono::Utc::now().timestamp())
                .bind(&id)
                .execute(&state.db)
                .await?
        };
        if deleted.rows_affected() == 0 {
            return Err(AppError::not_found("provider not found"));
        }
        state.balancer.forget_provider(&id);
        state.balancer.reload_providers(&state.db).await?;
        Ok(Json(json!({"ok":true})))
    }
    .await;
    let action = if result.is_ok() {
        "provider.delete"
    } else {
        "provider.delete.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    result
}

pub async fn test_provider(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    let row: (String, String) =
        sqlx::query_as("SELECT name,api_key FROM providers WHERE id=? AND is_deleted=0")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("provider not found"))?;
    let config = state.config.load_full();
    let model = config
        .available_model_ids
        .first()
        .cloned()
        .unwrap_or_else(|| "deepseek-flash".to_owned());
    let payload = json!({
        "model": model,
        "messages": [{"role":"user","content":"ping"}],
        "max_tokens": 1,
        "stream": false
    });
    let started = std::time::Instant::now();
    let response = state
        .client
        .post(format!("{}/chat/completions", config.upstream_base))
        .header("authorization", format!("Bearer {}", row.1))
        .json(&payload)
        .timeout(PROVIDER_TEST_TIMEOUT)
        .send()
        .await;
    let latency_ms = started.elapsed().as_millis();
    let value = match response {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            json!({
                "ok": status < 400,
                "status": status,
                "latency_ms": latency_ms,
                "model": model,
                "error": (status >= 400).then(|| provider_error_message(&body)),
            })
        }
        Err(error) => json!({
            "ok": false,
            "status": Value::Null,
            "latency_ms": latency_ms,
            "model": model,
            "error": Some(error.to_string()),
        }),
    };
    Ok(Json(value))
}

pub async fn provider_balance(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    let api_key: String =
        sqlx::query_scalar("SELECT api_key FROM providers WHERE id=? AND is_deleted=0")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("provider not found"))?;
    let config = state.config.load_full();
    let response = state
        .client
        .get(format!("{}/user/balance", config.upstream_base))
        .header("authorization", format!("Bearer {api_key}"))
        .timeout(PROVIDER_AUXILIARY_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            AppError::upstream(502, format!("DeepSeek balance request failed: {error}"))
        })?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|_| AppError::upstream(502, "DeepSeek returned invalid balance JSON"))?;
    if !status.is_success() {
        return Err(AppError::upstream(
            status.as_u16(),
            provider_error_message(&body.to_string()),
        ));
    }
    Ok(Json(json!({
        "is_available": body.get("is_available"),
        "balance_infos": body.get("balance_infos"),
    })))
}

fn provider_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(400).collect())
}

#[derive(Deserialize)]
pub struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct AuditQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    user_id: Option<String>,
    consumer: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    status: Option<AuditStatus>,
    error_code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditStatus {
    Success,
    Error,
}

const AUDIT_SUCCESS_CONDITION: &str = "((c.status<400 AND c.error IS NULL AND c.error_code IS NULL) OR (c.status=499 AND c.error='client_cancelled' AND c.error_code IS NULL))";
const AUDIT_ERROR_CONDITION: &str = "((c.status>=400 OR c.error IS NOT NULL OR c.error_code IS NOT NULL) AND NOT (c.status=499 AND c.error='client_cancelled' AND c.error_code IS NULL))";

#[derive(Deserialize)]
pub struct UsageQuery {
    period: Option<UsagePeriod>,
}

#[derive(Deserialize)]
enum UsagePeriod {
    #[serde(rename = "24h")]
    Last24Hours,
    #[serde(rename = "7d")]
    Last7Days,
}

impl UsagePeriod {
    fn seconds(&self) -> i64 {
        match self {
            Self::Last24Hours => 24 * 60 * 60,
            Self::Last7Days => 7 * 24 * 60 * 60,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Last24Hours => "24h",
            Self::Last7Days => "7d",
        }
    }
}

pub async fn usage(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<Value>, AppError> {
    let period = query.period.unwrap_or(UsagePeriod::Last24Hours);
    let since = chrono::Utc::now().timestamp() - period.seconds();
    Ok(Json(json!({
        "period": period.label(),
        "since": since,
        "rows": usage_rows(&state, &user, since).await?
    })))
}

async fn usage_rows(
    state: &AppState,
    user: &crate::auth::UserIdentity,
    since: i64,
) -> Result<Vec<Value>, AppError> {
    let sql = "SELECT c.user_id,k.id,k.name,k.prefix,COALESCE(NULLIF(c.model,''),'unknown'),date(c.created_at,'unixepoch'),COUNT(c.id),COALESCE(SUM(c.input_tokens),0),COALESCE(SUM(c.cached_tokens),0),COALESCE(SUM(c.output_tokens),0),COALESCE(SUM(c.request_transport_bytes+c.response_transport_bytes),0),COALESCE(SUM(c.official_cost_usd_nanos),0),COALESCE(SUM(c.actual_cost_usd_nanos),0),COALESCE(SUM(c.peak),0) FROM api_calls c JOIN consumers k ON k.id=c.consumer_id WHERE c.created_at>=?";
    let group = " GROUP BY c.user_id,k.id,k.name,k.prefix,COALESCE(NULLIF(c.model,''),'unknown'),date(c.created_at,'unixepoch') ORDER BY date(c.created_at,'unixepoch') ASC";
    let rows = if is_admin(user) {
        sqlx::query(&format!("{sql}{group}"))
            .bind(since)
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query(&format!("{sql} AND c.user_id=?{group}"))
            .bind(since)
            .bind(&user.id)
            .fetch_all(&state.db)
            .await?
    };
    Ok(rows.into_iter().map(|row| {
        json!({"user_id":row.get::<String,_>(0),"consumer_id":row.get::<String,_>(1),"consumer_name":row.get::<String,_>(2),"consumer_prefix":row.get::<String,_>(3),"model":row.get::<String,_>(4),"date":row.get::<String,_>(5),"requests":row.get::<i64,_>(6),"input_tokens":row.get::<i64,_>(7),"cached_tokens":row.get::<i64,_>(8),"output_tokens":row.get::<i64,_>(9),"network_transport_bytes":row.get::<i64,_>(10),"official_cost_usd_nanos":row.get::<i64,_>(11),"actual_cost_usd_nanos":row.get::<i64,_>(12),"peak_requests":row.get::<i64,_>(13)})
    }).collect())
}

pub async fn audit(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Query(filters): Query<AuditQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = filters.limit.unwrap_or(100).clamp(1, 500);
    let offset = filters.offset.unwrap_or(0).max(0);

    let mut total_query = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(*) FROM api_calls c JOIN consumers k ON k.id=c.consumer_id LEFT JOIN providers p ON p.id=c.provider_id",
    );
    append_audit_filters(&mut total_query, &filters, &user);
    let total: i64 = total_query
        .build_query_scalar()
        .fetch_one(&state.db)
        .await?;

    let mut rows_query = QueryBuilder::<Sqlite>::new(
        "SELECT c.id,c.request_id,c.thread_id,c.user_id,k.name,c.provider_id,p.name,c.path,c.method,c.model,c.reasoning_effort,c.peak,c.status,c.latency_ms,c.input_tokens,c.output_tokens,c.cached_tokens,c.error,c.client_ip,c.created_at,c.first_byte_latency_ms,c.request_bytes,c.response_bytes,c.request_transport_bytes,c.response_transport_bytes,c.official_cost_usd_nanos,c.actual_cost_usd_nanos,c.price_multiplier_nanos,c.official_consumed_usd_before_nanos,c.official_consumed_usd_after_nanos,c.actual_consumed_usd_before_nanos,c.actual_consumed_usd_after_nanos,c.official_provided_usd_before_nanos,c.official_provided_usd_after_nanos,c.actual_provided_usd_before_nanos,c.actual_provided_usd_after_nanos,c.error_code FROM api_calls c JOIN consumers k ON k.id=c.consumer_id LEFT JOIN providers p ON p.id=c.provider_id",
    );
    append_audit_filters(&mut rows_query, &filters, &user);
    rows_query
        .push(" ORDER BY c.created_at DESC, c.id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let rows = rows_query.build().fetch_all(&state.db).await?;
    Ok(Json(json!({"rows": rows.into_iter().map(|row| {
        json!({
            "id":row.get::<String,_>(0),
            "request_id":row.get::<String,_>(1),
            "thread_id":row.get::<Option<String>,_>(2),
            "user_id":row.get::<String,_>(3),
            "consumer_name":row.get::<String,_>(4),
            "provider_id":row.get::<Option<String>,_>(5),
            "provider_name":row.get::<Option<String>,_>(6),
            "path":row.get::<String,_>(7),
            "method":row.get::<String,_>(8),
            "model":row.get::<Option<String>,_>(9),
            "reasoning_effort":row.get::<Option<String>,_>(10),
            "peak":row.get::<i64,_>(11) != 0,
            "status":row.get::<i64,_>(12),
            "latency_ms":row.get::<i64,_>(13),
            "input_tokens":row.get::<i64,_>(14),
            "output_tokens":row.get::<i64,_>(15),
            "cached_tokens":row.get::<i64,_>(16),
            "error":row.get::<Option<String>,_>(17),
            "client_ip":row.get::<Option<String>,_>(18),
            "created_at":row.get::<i64,_>(19),
            "first_byte_latency_ms":row.get::<Option<i64>,_>(20),
            "request_bytes":row.get::<i64,_>(21),
            "response_bytes":row.get::<i64,_>(22),
            "request_transport_bytes":row.get::<i64,_>(23),
            "response_transport_bytes":row.get::<i64,_>(24),
            "official_cost_usd_nanos":row.get::<i64,_>(25),
            "actual_cost_usd_nanos":row.get::<i64,_>(26),
            "price_multiplier_nanos":row.get::<i64,_>(27),
            "official_consumed_usd_before_nanos":row.get::<i64,_>(28),
            "official_consumed_usd_after_nanos":row.get::<i64,_>(29),
            "actual_consumed_usd_before_nanos":row.get::<i64,_>(30),
            "actual_consumed_usd_after_nanos":row.get::<i64,_>(31),
            "official_provided_usd_before_nanos":row.get::<i64,_>(32),
            "official_provided_usd_after_nanos":row.get::<i64,_>(33),
            "actual_provided_usd_before_nanos":row.get::<i64,_>(34),
            "actual_provided_usd_after_nanos":row.get::<i64,_>(35),
            "error_code":row.get::<Option<String>,_>(36)
        })
    }).collect::<Vec<_>>(),"total":total})))
}

fn append_audit_filters(
    query: &mut QueryBuilder<Sqlite>,
    filters: &AuditQuery,
    user: &UserIdentity,
) {
    query.push(" WHERE 1=1");
    if !is_admin(user) {
        query.push(" AND c.user_id=").push_bind(user.id.clone());
    }
    append_audit_text_filter(query, "c.user_id", filters.user_id.as_deref());
    append_audit_text_filter(query, "k.name", filters.consumer.as_deref());
    append_audit_text_filter(query, "p.name", filters.provider.as_deref());
    append_audit_text_filter(query, "c.model", filters.model.as_deref());
    if let Some(code) = filters
        .error_code
        .as_deref()
        .filter(|code| !code.is_empty())
    {
        query.push(" AND c.error_code=").push_bind(code.to_owned());
    }
    if let Some(status) = &filters.status {
        match status {
            AuditStatus::Success => query.push(" AND ").push(AUDIT_SUCCESS_CONDITION),
            AuditStatus::Error => query.push(" AND ").push(AUDIT_ERROR_CONDITION),
        };
    }
}

fn append_audit_text_filter(
    query: &mut QueryBuilder<Sqlite>,
    column: &'static str,
    value: Option<&str>,
) {
    let Some(value) = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    else {
        return;
    };
    query
        .push(" AND instr(lower(COALESCE(")
        .push(column)
        .push(",'')),lower(")
        .push_bind(value)
        .push("))>0");
}

fn visible_archive_headers(headers: Option<String>, admin: bool) -> Option<String> {
    if admin {
        return headers;
    }
    headers.map(|headers| {
        serde_json::from_str::<Vec<(String, String)>>(&headers)
            .map(|headers| {
                headers
                    .into_iter()
                    .filter(|(name, _)| !sensitive_header(name))
                    .collect::<Vec<_>>()
            })
            .and_then(|headers| serde_json::to_string(&headers))
            .unwrap_or_else(|_| "[]".to_owned())
    })
}

fn sensitive_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "api-key"
            | "x-api-key"
            | "x-auth"
            | "x-credential"
            | "access-key"
            | "x-session-id"
            | "session-id"
            | "session_id"
    ) || name.ends_with("-api-key")
        || name.ends_with("-token")
        || name.contains("secret")
}

pub async fn audit_detail(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let admin = is_admin(&user);
    let columns = "c.id,c.request_id,c.thread_id,c.user_id,k.name,c.provider_id,p.name,c.method,c.path,c.model,c.reasoning_effort,c.peak,c.status,c.latency_ms,c.input_tokens,c.output_tokens,c.cached_tokens,c.error,c.client_ip,c.affinity_hash,c.affinity_source,c.created_at,a.api_call_id,a.request_headers_json,a.request_body,a.request_body_truncated,a.response_headers_json,a.response_body,a.response_body_truncated,c.consumer_id,c.first_byte_latency_ms,c.request_bytes,c.response_bytes,c.request_transport_bytes,c.response_transport_bytes,c.downstream_accept_encoding,c.downstream_content_encoding,c.upstream_accept_encoding,c.upstream_content_encoding,a.upstream_request_headers_json,a.downstream_response_headers_json,c.upstream_http_version,c.official_cost_usd_nanos,c.actual_cost_usd_nanos,c.price_multiplier_nanos,c.official_consumed_usd_before_nanos,c.official_consumed_usd_after_nanos,c.actual_consumed_usd_before_nanos,c.actual_consumed_usd_after_nanos,c.official_provided_usd_before_nanos,c.official_provided_usd_after_nanos,c.actual_provided_usd_before_nanos,c.actual_provided_usd_after_nanos,COALESCE(a.bodies_deleted,0),c.error_code";
    let from = " FROM api_calls c JOIN consumers k ON k.id=c.consumer_id LEFT JOIN providers p ON p.id=c.provider_id LEFT JOIN request_archives a ON a.api_call_id=c.id WHERE c.id=?";
    let (sql, scope) = if admin {
        (format!("SELECT {columns}{from}"), None)
    } else {
        (
            format!("SELECT {columns}{from} AND c.user_id=?"),
            Some(user.id.clone()),
        )
    };
    let mut query = sqlx::query(&sql).bind(&id);
    if let Some(scope) = scope {
        query = query.bind(scope);
    }
    let row = query
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("audit event not found"))?;
    let thread_id = row.get::<Option<String>, _>(2);
    let consumer_id = row.get::<String, _>(29);
    let navigation_rows = if let Some(thread_id) = &thread_id {
        let (sql, scope) = if admin {
            (
                "SELECT id,request_id,created_at FROM api_calls WHERE thread_id=? AND consumer_id=? ORDER BY created_at,id",
                None,
            )
        } else {
            (
                "SELECT id,request_id,created_at FROM api_calls WHERE thread_id=? AND consumer_id=? AND user_id=? ORDER BY created_at,id",
                Some(user.id.clone()),
            )
        };
        let mut query = sqlx::query(sql).bind(thread_id).bind(consumer_id);
        if let Some(scope) = scope {
            query = query.bind(scope);
        }
        query.fetch_all(&state.db).await?
    } else {
        Vec::new()
    };
    let position = navigation_rows
        .iter()
        .position(|item| item.get::<String, _>(0) == id);
    let navigation = |item: Option<&sqlx::sqlite::SqliteRow>| {
        item.map(|item| {
            json!({
                "id":item.get::<String,_>(0),
                "request_id":item.get::<String,_>(1),
                "created_at":item.get::<i64,_>(2)
            })
        })
    };
    let previous = position
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| navigation_rows.get(index));
    let next = position.and_then(|index| navigation_rows.get(index + 1));
    let request_headers = visible_archive_headers(row.get::<Option<String>, _>(23), admin);
    let upstream_request_headers = visible_archive_headers(row.get::<Option<String>, _>(39), admin);
    let response_headers = visible_archive_headers(row.get::<Option<String>, _>(26), admin);
    let downstream_response_headers =
        visible_archive_headers(row.get::<Option<String>, _>(40), admin);
    let archive_available = row.get::<Option<String>, _>(22).is_some();
    let bodies_available = archive_available && row.get::<i64, _>(52) == 0;
    let mut detail = json!({
        "id":row.get::<String,_>(0),
        "request_id":row.get::<String,_>(1),
        "thread_id":thread_id,
        "user_id":row.get::<String,_>(3),
        "consumer_name":row.get::<String,_>(4),
        "provider_id":row.get::<Option<String>,_>(5),
        "provider_name":row.get::<Option<String>,_>(6),
        "method":row.get::<String,_>(7),
        "path":row.get::<String,_>(8),
        "model":row.get::<Option<String>,_>(9),
        "reasoning_effort":row.get::<Option<String>,_>(10),
        "peak":row.get::<i64,_>(11) != 0,
        "status":row.get::<i64,_>(12),
        "latency_ms":row.get::<i64,_>(13),
        "input_tokens":row.get::<i64,_>(14),
        "output_tokens":row.get::<i64,_>(15),
        "cached_tokens":row.get::<i64,_>(16),
        "error":row.get::<Option<String>,_>(17),
        "client_ip":row.get::<Option<String>,_>(18),
        "affinity_hash":row.get::<Option<String>,_>(19),
        "affinity_source":row.get::<Option<String>,_>(20),
        "created_at":row.get::<i64,_>(21),
        "first_byte_latency_ms":row.get::<Option<i64>,_>(30),
        "request_bytes":row.get::<i64,_>(31),
        "response_bytes":row.get::<i64,_>(32),
        "request_transport_bytes":row.get::<i64,_>(33),
        "response_transport_bytes":row.get::<i64,_>(34),
        "downstream_accept_encoding":row.get::<Option<String>,_>(35),
        "downstream_content_encoding":row.get::<Option<String>,_>(36),
        "upstream_accept_encoding":row.get::<Option<String>,_>(37),
        "upstream_content_encoding":row.get::<Option<String>,_>(38),
        "upstream_http_version":row.get::<Option<String>,_>(41),
        "official_cost_usd_nanos":row.get::<i64,_>(42),
        "actual_cost_usd_nanos":row.get::<i64,_>(43),
        "price_multiplier_nanos":row.get::<i64,_>(44),
        "official_consumed_usd_before_nanos":row.get::<i64,_>(45),
        "official_consumed_usd_after_nanos":row.get::<i64,_>(46),
        "actual_consumed_usd_before_nanos":row.get::<i64,_>(47),
        "actual_consumed_usd_after_nanos":row.get::<i64,_>(48),
        "official_provided_usd_before_nanos":row.get::<i64,_>(49),
        "official_provided_usd_after_nanos":row.get::<i64,_>(50),
        "actual_provided_usd_before_nanos":row.get::<i64,_>(51),
        "actual_provided_usd_after_nanos":row.get::<i64,_>(52),
        "request_headers":request_headers,
        "upstream_request_headers":upstream_request_headers,
        "request_body":row.get::<Option<Vec<u8>>,_>(24).map(|body| String::from_utf8_lossy(&body).into_owned()),
        "request_body_truncated":row.get::<Option<i64>,_>(25).unwrap_or_default() != 0,
        "response_headers":response_headers,
        "downstream_response_headers":downstream_response_headers,
        "response_body":row.get::<Option<Vec<u8>>,_>(27).map(|body| String::from_utf8_lossy(&body).into_owned()),
        "response_body_truncated":row.get::<Option<i64>,_>(28).unwrap_or_default() != 0,
        "previous":navigation(previous),
        "next":navigation(next)
    });
    detail["error_code"] = row
        .get::<Option<String>, _>(53)
        .map(Value::String)
        .unwrap_or(Value::Null);
    detail["archive_available"] = Value::Bool(archive_available);
    detail["bodies_available"] = Value::Bool(bodies_available);
    Ok(Json(detail))
}

pub async fn delete_audit_bodies(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let admin = is_admin(&user);
    let mut query = QueryBuilder::<Sqlite>::new(
        "UPDATE request_archives SET request_body=X'',request_body_truncated=0,response_body=NULL,response_body_truncated=0,bodies_deleted=1 WHERE api_call_id=",
    );
    query.push_bind(&id);
    query.push(" AND bodies_deleted=0");
    if !admin {
        query
            .push(" AND EXISTS(SELECT 1 FROM api_calls WHERE api_calls.id=request_archives.api_call_id AND api_calls.user_id=")
            .push_bind(user.id.clone())
            .push(")");
    }
    let result = {
        let _write = state.write_gate.lock().await;
        query.build().execute(&state.db).await?
    };
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("audit diagnostic bodies not found"));
    }
    if admin {
        write_admin_audit(
            &state,
            &user.id,
            "audit.bodies.delete",
            Some(&id),
            &peer.ip().to_string(),
        )
        .await?;
    }
    Ok(Json(json!({"ok":true})))
}

pub async fn list_admin_audit(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Query(page): Query<Page>,
) -> Result<Json<Value>, AppError> {
    require_admin(&user)?;
    let rows = sqlx::query("SELECT id,admin_user_id,action,target_id,client_ip,created_at FROM admin_audit ORDER BY created_at DESC LIMIT ? OFFSET ?")
        .bind(page.limit.unwrap_or(100).clamp(1, 500))
        .bind(page.offset.unwrap_or(0).max(0))
        .fetch_all(&state.db).await?;
    Ok(Json(Value::Array(rows.into_iter().map(|row| {
        json!({"id":row.get::<String,_>(0),"admin_user_id":row.get::<String,_>(1),"action":row.get::<String,_>(2),"target_id":row.get::<Option<String>,_>(3),"client_ip":row.get::<Option<String>,_>(4),"created_at":row.get::<i64,_>(5)})
    }).collect())))
}

pub async fn dashboard(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    let keys: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM consumers WHERE user_id=? AND is_system=0 AND is_deleted=0",
    )
    .bind(&user.id)
    .fetch_one(&state.db)
    .await?;
    let providers: i64 = if is_admin(&user) {
        sqlx::query_scalar("SELECT COUNT(*) FROM providers WHERE is_deleted=0")
            .fetch_one(&state.db)
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM providers WHERE owner_id=? AND is_deleted=0")
            .bind(&user.id)
            .fetch_one(&state.db)
            .await?
    };
    let since = chrono::Utc::now().timestamp() - 86400;
    let calls: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM api_calls WHERE user_id=? AND created_at>?")
            .bind(&user.id)
            .bind(since)
            .fetch_one(&state.db)
            .await?;
    let errors_query = "SELECT COUNT(*) FROM api_calls c WHERE c.user_id=? AND ".to_owned()
        + AUDIT_ERROR_CONDITION
        + " AND c.created_at>?";
    let errors: i64 = sqlx::query_scalar(&errors_query)
        .bind(&user.id)
        .bind(since)
        .fetch_one(&state.db)
        .await?;
    let peak: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM api_calls WHERE user_id=? AND peak=1 AND created_at>?",
    )
    .bind(&user.id)
    .bind(since)
    .fetch_one(&state.db)
    .await?;
    let (input_tokens_24h, output_tokens_24h, cached_tokens_24h): (i64, i64, i64) =
        sqlx::query_as(
            "SELECT COALESCE(SUM(input_tokens),0),COALESCE(SUM(output_tokens),0),COALESCE(SUM(cached_tokens),0) FROM api_calls WHERE user_id=? AND created_at>?",
        )
        .bind(&user.id)
        .bind(since)
        .fetch_one(&state.db)
        .await?;
    let (official_cost_usd_nanos_24h, actual_cost_usd_nanos_24h): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(official_cost_usd_nanos),0),COALESCE(SUM(actual_cost_usd_nanos),0) FROM api_calls WHERE user_id=? AND created_at>?",
    )
    .bind(&user.id)
    .bind(since)
    .fetch_one(&state.db)
    .await?;
    let (official_consumed_usd_nanos, consumed_usd_nanos): (i64, i64) = sqlx::query_as(
        "SELECT official_consumed_usd_nanos,consumed_usd_nanos FROM users WHERE id=?",
    )
    .bind(&user.id)
    .fetch_one(&state.db)
    .await?;
    let config = state.config.load();
    Ok(Json(json!({
        "active_consumers":keys,
        "active_providers":providers,
        "calls_24h":calls,
        "errors_24h":errors,
        "peak_calls_24h":peak,
        "input_tokens_24h":input_tokens_24h,
        "output_tokens_24h":output_tokens_24h,
        "cached_tokens_24h":cached_tokens_24h,
        "official_cost_usd_nanos_24h":official_cost_usd_nanos_24h,
        "actual_cost_usd_nanos_24h":actual_cost_usd_nanos_24h,
        "official_consumed_usd_nanos":official_consumed_usd_nanos,
        "consumed_usd_nanos":consumed_usd_nanos,
        "peak_now":pricing::is_peak(chrono::Utc::now().timestamp()),
        "available_model_ids":config.available_model_ids
    })))
}

pub async fn system_resources(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<SystemResourcesSnapshot>, AppError> {
    require_admin(&user)?;
    let snapshot = state.resources.lock().await.sample().await?;
    Ok(Json(snapshot))
}

pub async fn vacuum_system_database(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<SystemResourcesSnapshot>, AppError> {
    require_admin(&user)?;
    let result = state.resources.lock().await.vacuum().await;
    let action = if result.is_ok() {
        "system.database.vacuum"
    } else {
        "system.database.vacuum.failed"
    };
    write_admin_audit(&state, &user.id, action, None, &peer.ip().to_string()).await?;
    Ok(Json(result?))
}

pub async fn settings(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    let config = state.config.load();
    Ok(Json(json!({
        "role":user.role,
        "auth_issuer":config.auth_issuer,
        "auth_audience":config.auth_audience,
        "upstream_base":config.upstream_base,
        "available_model_ids":config.available_model_ids,
        "allow_all_users_debt":config.allow_all_users_debt,
        "response_body_limit":config.response_body_limit,
        "affinity_ttl_seconds":config.affinity_ttl_seconds,
        "provider_concurrency_limit":config.provider_concurrency_limit,
        "request_archive_retention_days":config.request_archive_retention_days,
        "model_price_multiplier":payments::format_usd_nanos(config.model_price_multiplier_nanos),
        "midas_api_base":config.midas_api_base,
        "midas_fund_user_id":config.midas_fund_user_id,
        "midas_fund_api_key_configured":config.midas_fund_api_key.is_some()
    })))
}

pub async fn provider_audit(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<Value>, AppError> {
    require_admin(&user)?;
    let period = query.period.unwrap_or(UsagePeriod::Last7Days);
    let until = chrono::Utc::now().timestamp();
    let since = until - period.seconds();
    let sql = "SELECT (c.created_at / 3600) * 3600 AS hour_start,c.provider_id,p.name,COALESCE(NULLIF(c.model,''),'unknown'),SUM(CASE WHEN NULLIF(c.error_code,'') IS NULL AND c.error IS NULL AND c.status<400 THEN 1 ELSE 0 END),SUM(CASE WHEN c.error_code IS NOT NULL OR c.error IS NOT NULL OR c.status>=400 THEN 1 ELSE 0 END),COALESCE(SUM(c.input_tokens),0),COALESCE(SUM(c.actual_cost_usd_nanos),0),COALESCE(SUM(c.peak),0) FROM api_calls c LEFT JOIN providers p ON p.id=c.provider_id WHERE c.created_at>=? AND c.created_at<=? GROUP BY hour_start,c.provider_id,p.name,COALESCE(NULLIF(c.model,''),'unknown') ORDER BY hour_start DESC,p.name,COALESCE(NULLIF(c.model,''),'unknown')";
    let rows = sqlx::query(sql)
        .bind(since)
        .bind(until)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({
        "since": since,
        "until": until,
        "period": period.label(),
        "rows": rows.into_iter().map(|row| {
            let successful = row.get::<i64, _>(4);
            let failed = row.get::<i64, _>(5);
            let requests = successful + failed;
            let success_rate = if requests == 0 { 0.0 } else { successful as f64 / requests as f64 };
            let failure_rate = if requests == 0 { 0.0 } else { failed as f64 / requests as f64 };
            json!({
                "hour_start": row.get::<i64, _>(0),
                "provider_id": row.get::<Option<String>, _>(1),
                "provider_name": row.get::<Option<String>, _>(2),
                "model": row.get::<String, _>(3),
                "requests": requests,
                "successful_requests": successful,
                "failed_requests": failed,
                "success_rate": success_rate,
                "failure_rate": failure_rate,
                "input_tokens": row.get::<i64, _>(6),
                "actual_cost_usd_nanos": row.get::<i64, _>(7),
                "peak_requests": row.get::<i64, _>(8),
            })
        }).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
pub struct UpdateAvailableModelIds {
    available_model_ids: Vec<String>,
}

pub async fn update_available_model_ids(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Json(input): Json<UpdateAvailableModelIds>,
) -> Result<Json<Value>, AppError> {
    require_admin(&user)?;
    let model_ids = config::normalize_available_model_ids(input.available_model_ids)
        .map_err(AppError::bad_request)?;
    let value = serde_json::to_string(&model_ids)?;
    let now = chrono::Utc::now().timestamp();
    {
        let _write = state.write_gate.lock().await;
        sqlx::query("UPDATE app_meta SET value=?,updated_at=? WHERE key='available_model_ids'")
            .bind(value)
            .bind(now)
            .execute(&state.db)
            .await?;
    }
    let mut config = (**state.config.load()).clone();
    config.available_model_ids = model_ids;
    state.config.store(std::sync::Arc::new(config));
    write_admin_audit(
        &state,
        &user.id,
        "settings.available_models.update",
        None,
        &peer.ip().to_string(),
    )
    .await?;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
pub struct UpdateProviderConcurrency {
    provider_concurrency_limit: usize,
}

pub async fn update_provider_concurrency(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Json(input): Json<UpdateProviderConcurrency>,
) -> Result<Json<Value>, AppError> {
    require_admin(&user)?;
    if input.provider_concurrency_limit == 0 {
        return Err(AppError::bad_request(
            "provider_concurrency_limit must be a positive integer",
        ));
    }
    {
        let _write = state.write_gate.lock().await;
        sqlx::query(
            "UPDATE app_meta SET value=?,updated_at=? WHERE key='provider_concurrency_limit'",
        )
        .bind(input.provider_concurrency_limit.to_string())
        .bind(chrono::Utc::now().timestamp())
        .execute(&state.db)
        .await?;
        let mut config = (**state.config.load()).clone();
        config.provider_concurrency_limit = input.provider_concurrency_limit;
        state.config.store(std::sync::Arc::new(config));
        state
            .balancer
            .set_concurrency_limit(input.provider_concurrency_limit);
    }
    write_admin_audit(
        &state,
        &user.id,
        "settings.provider_concurrency.update",
        None,
        &peer.ip().to_string(),
    )
    .await?;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
pub struct UpdateSettings {
    upstream_base: String,
    allow_all_users_debt: bool,
    response_body_limit: usize,
    affinity_ttl_seconds: i64,
    request_archive_retention_days: i64,
    model_price_multiplier: String,
}

pub async fn update_settings(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(root): Extension<UserIdentity>,
    Json(input): Json<UpdateSettings>,
) -> Result<Json<Value>, AppError> {
    require_root(&root)?;
    let upstream_base = validate_http_url("upstream base", &input.upstream_base)?;
    let model_price_multiplier_nanos =
        payments::parse_model_price_multiplier_nanos(&input.model_price_multiplier)?;
    if !(1_024..=16 * 1_024 * 1_024).contains(&input.response_body_limit)
        || !(60..=2_592_000).contains(&input.affinity_ttl_seconds)
        || !(1..=365).contains(&input.request_archive_retention_days)
    {
        return Err(AppError::bad_request(
            "one or more numeric settings are outside the allowed range",
        ));
    }
    let values = [
        ("upstream_base", upstream_base.clone()),
        (
            "allow_all_users_debt",
            input.allow_all_users_debt.to_string(),
        ),
        ("response_body_limit", input.response_body_limit.to_string()),
        (
            "affinity_ttl_seconds",
            input.affinity_ttl_seconds.to_string(),
        ),
        (
            "request_archive_retention_days",
            input.request_archive_retention_days.to_string(),
        ),
        (
            "model_price_multiplier_nanos",
            model_price_multiplier_nanos.to_string(),
        ),
    ];
    let now = chrono::Utc::now().timestamp();
    {
        let _write = state.write_gate.lock().await;
        let mut transaction = state.db.begin().await?;
        for (key, value) in values {
            sqlx::query("UPDATE app_meta SET value=?,updated_at=? WHERE key=?")
                .bind(value)
                .bind(now)
                .bind(key)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
    }
    let mut config = (**state.config.load()).clone();
    config.upstream_base = upstream_base;
    config.allow_all_users_debt = input.allow_all_users_debt;
    config.response_body_limit = input.response_body_limit;
    config.affinity_ttl_seconds = input.affinity_ttl_seconds;
    config.request_archive_retention_days = input.request_archive_retention_days;
    config.model_price_multiplier_nanos = model_price_multiplier_nanos;
    state.config.store(std::sync::Arc::new(config));
    write_admin_audit(
        &state,
        &root.id,
        "settings.update",
        None,
        &peer.ip().to_string(),
    )
    .await?;
    Ok(Json(json!({"ok":true})))
}

fn validate_http_url(name: &str, value: &str) -> Result<String, AppError> {
    let normalized = value.trim().trim_end_matches('/');
    let parsed = url::Url::parse(normalized)
        .map_err(|_| AppError::bad_request(format!("{name} must be an absolute URL")))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AppError::bad_request(format!(
            "{name} must use HTTP or HTTPS"
        )));
    }
    Ok(normalized.to_owned())
}

fn required_setting(name: &str, value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 200 {
        return Err(AppError::bad_request(format!(
            "{name} must be 1-200 characters"
        )));
    }
    Ok(value.to_owned())
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    require_admin(&user)?;
    let rows = sqlx::query(
        r#"
        SELECT
            u.id,u.role,u.allow_debt,u.created_at,
            u.consumed_usd_nanos,u.provided_usd_nanos
        FROM users u
        ORDER BY u.created_at,u.id
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    let user_ids = rows
        .iter()
        .map(|row| row.get::<String, _>(0))
        .collect::<Vec<_>>();
    let topups = if midas::is_configured(&state.config.load_full()) {
        midas::inbound_transfer_totals(&state, &user_ids).await?
    } else {
        std::collections::HashMap::new()
    };
    Ok(Json(Value::Array(
        rows.into_iter()
            .map(|row| {
                let id = row.get::<String, _>(0);
                let topup_usd_nanos = topups.get(&id).copied().unwrap_or(0);
                let consumed_usd_nanos = row.get::<i64, _>(4);
                let provided_usd_nanos = row.get::<i64, _>(5);
                json!({
                    "id":id,
                    "role":row.get::<String,_>(1),
                    "allow_debt":row.get::<i64,_>(2) != 0,
                    "created_at":row.get::<i64,_>(3),
                    "topup_usd_nanos":topup_usd_nanos,
                    "consumed_usd_nanos":consumed_usd_nanos,
                    "provided_usd_nanos":provided_usd_nanos,
                    "available_usd_nanos":topup_usd_nanos.saturating_add(provided_usd_nanos).saturating_sub(consumed_usd_nanos)
                })
            })
            .collect(),
    )))
}

#[derive(Deserialize)]
pub struct UpdateUser {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    allow_debt: Option<bool>,
}

pub async fn update_user(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(admin): Extension<UserIdentity>,
    Path(id): Path<String>,
    Json(input): Json<UpdateUser>,
) -> Result<Json<Value>, AppError> {
    require_admin(&admin)?;
    if input.role.is_none() && input.allow_debt.is_none() {
        return Err(AppError::bad_request("role or allow_debt is required"));
    }
    if let Some(role) = input.role {
        require_root(&admin)?;
        if !matches!(role.as_str(), "admin" | "user") {
            return Err(AppError::bad_request("role must be admin or user"));
        }
        let result = {
            let _write = state.write_gate.lock().await;
            sqlx::query("UPDATE users SET role=? WHERE id=? AND role<>'root'")
                .bind(role)
                .bind(&id)
                .execute(&state.db)
                .await?
        };
        if result.rows_affected() == 0 {
            return Err(AppError::not_found(
                "user not found or root role is immutable",
            ));
        }
        state.balancer.reload_providers(&state.db).await?;
        write_admin_audit(
            &state,
            &admin.id,
            "user.role.update",
            Some(&id),
            &peer.ip().to_string(),
        )
        .await?;
    }
    if let Some(allow_debt) = input.allow_debt {
        let result = {
            let _write = state.write_gate.lock().await;
            sqlx::query("UPDATE users SET allow_debt=? WHERE id=?")
                .bind(allow_debt)
                .bind(&id)
                .execute(&state.db)
                .await?
        };
        if result.rows_affected() == 0 {
            return Err(AppError::not_found("user not found"));
        }
        write_admin_audit(
            &state,
            &admin.id,
            "user.allow_debt.update",
            Some(&id),
            &peer.ip().to_string(),
        )
        .await?;
    }
    Ok(Json(json!({"ok":true})))
}

async fn write_admin_audit(
    state: &AppState,
    admin_user_id: &str,
    action: &str,
    target_id: Option<&str>,
    client_ip: &str,
) -> Result<(), AppError> {
    let _write = state.write_gate.lock().await;
    sqlx::query(
        "INSERT INTO admin_audit(id,admin_user_id,action,target_id,client_ip,created_at) VALUES(?,?,?,?,?,?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(admin_user_id)
    .bind(action)
    .bind(target_id)
    .bind(client_ip)
    .bind(chrono::Utc::now().timestamp())
    .execute(&state.db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;

    use super::*;

    fn root_identity() -> UserIdentity {
        UserIdentity {
            id: "root".to_owned(),
            email: None,
            name: None,
            role: "root".to_owned(),
        }
    }

    fn user_identity() -> UserIdentity {
        UserIdentity {
            id: "user".to_owned(),
            email: None,
            name: None,
            role: "user".to_owned(),
        }
    }

    fn peer() -> ConnectInfo<SocketAddr> {
        ConnectInfo("127.0.0.1:8080".parse().unwrap())
    }

    async fn seed_users(state: &AppState) {
        for (id, role) in [("root", "root"), ("user", "user")] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,?,0)")
                .bind(id)
                .bind(role)
                .execute(&state.db)
                .await
                .unwrap();
        }
    }

    async fn seed_consumer(state: &AppState, id: &str, owner: &str, secret: &str) {
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES(?,?,?,?,?,0)")
            .bind(id)
            .bind(owner)
            .bind(id)
            .bind(id)
            .bind(consumer_secret_hash(secret))
            .execute(&state.db)
            .await
            .unwrap();
    }

    async fn seed_provider(state: &AppState, id: &str, owner: &str) {
        sqlx::query("INSERT INTO providers(id,name,api_key,status,created_at,updated_at,owner_id,visibility) VALUES(?,?,?,'active',0,0,?,?)")
            .bind(id)
            .bind(id)
            .bind(format!("sk-{id}"))
            .bind(owner)
            .bind(balancer::PROVIDER_VISIBILITY_PRIVATE)
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
    }

    #[tokio::test]
    async fn provider_lifecycle_round_trips_one_api_key() {
        let state = crate::test_state("http://upstream.invalid").await;
        seed_users(&state).await;

        let created = create_provider(
            State(state.clone()),
            peer(),
            Extension(root_identity()),
            Json(CreateProvider {
                name: "primary".to_owned(),
                api_key: "sk-deepseek-primary".to_owned(),
                visibility: Some("public".to_owned()),
            }),
        )
        .await
        .unwrap();
        let id = created.0["id"].as_str().unwrap().to_owned();
        assert_eq!(created.0["visibility"], "public");

        let listed = list_providers(State(state.clone()), Extension(root_identity()))
            .await
            .unwrap();
        let provider = &listed.0[0];
        assert_eq!(provider["name"], "primary");
        assert_eq!(
            provider["api_key"],
            json!(null),
            "the key never leaves the server"
        );
        assert_eq!(provider["usage"], json!(null));

        let _ = update_provider(
            State(state.clone()),
            peer(),
            Extension(root_identity()),
            Path(id.clone()),
            Json(ProviderUpdate {
                name: Some("renamed".to_owned()),
                enabled: Some(false),
                visibility: Some("private".to_owned()),
            }),
        )
        .await
        .unwrap();
        let (name, status, disabled, visibility): (String, String, i64, String) = sqlx::query_as(
            "SELECT name,status,manual_disabled,visibility FROM providers WHERE id=?",
        )
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(name, "renamed");
        assert_eq!(status, "disabled");
        assert_eq!(disabled, 1);
        assert_eq!(visibility, "private");

        let (headers, key) = read_provider_key(
            State(state.clone()),
            peer(),
            Extension(root_identity()),
            Path(id.clone()),
        )
        .await
        .unwrap();
        assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
        assert_eq!(key.0["api_key"], "sk-deepseek-primary");

        let _ = replace_provider_key(
            State(state.clone()),
            peer(),
            Extension(root_identity()),
            Path(id.clone()),
            Json(ReplaceProviderKey {
                name: None,
                api_key: "sk-deepseek-rotated".to_owned(),
            }),
        )
        .await
        .unwrap();
        let (api_key, status, disabled): (String, String, i64) =
            sqlx::query_as("SELECT api_key,status,manual_disabled FROM providers WHERE id=?")
                .bind(&id)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(api_key, "sk-deepseek-rotated");
        assert_eq!(
            status, "disabled",
            "a manual disable survives a key rotation"
        );
        assert_eq!(disabled, 1);

        let _ = delete_provider(
            State(state.clone()),
            peer(),
            Extension(root_identity()),
            Path(id.clone()),
        )
        .await
        .unwrap();
        let deleted: i64 = sqlx::query_scalar("SELECT is_deleted FROM providers WHERE id=?")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        let listed = list_providers(State(state.clone()), Extension(root_identity()))
            .await
            .unwrap();
        assert!(listed.0.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn provider_usage_and_load_are_reported_per_provider() {
        let state = crate::test_state("http://upstream.invalid").await;
        seed_users(&state).await;
        seed_consumer(&state, "consumer", "user", "sk-consumer").await;
        seed_provider(&state, "provider", "user").await;
        for (id, peak, tokens, cost, error) in [
            ("first", 1_i64, 10_i64, 5_i64, None),
            ("second", 0, 20, 7, Some("boom")),
        ] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,peak,status,latency_ms,input_tokens,output_tokens,actual_cost_usd_nanos,error,created_at) VALUES(?,?, 'consumer','user','provider','POST','/v1/chat/completions','deepseek-flash',?,200,10,?,0,?,?,unixepoch())")
                .bind(id)
                .bind(id)
                .bind(peak)
                .bind(tokens)
                .bind(cost)
                .bind(error)
                .execute(&state.db)
                .await
                .unwrap();
        }

        let listed = list_providers(State(state.clone()), Extension(user_identity()))
            .await
            .unwrap();
        let usage = &listed.0[0]["usage"];
        assert_eq!(usage["requests"], 2);
        assert_eq!(usage["errors"], 1);
        assert_eq!(usage["peak_requests"], 1);
        assert_eq!(usage["input_tokens"], 30);
        assert_eq!(usage["actual_cost_usd_nanos"], 12);
        assert_eq!(listed.0[0]["inflight"], 0);
        assert_eq!(listed.0[0]["queued"], 0);
        assert_eq!(listed.0[0]["concurrency_limit"], 3);

        // Another user never sees a private provider of someone else.
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('other','user',0)")
            .execute(&state.db)
            .await
            .unwrap();
        let hidden = list_providers(
            State(state.clone()),
            Extension(UserIdentity {
                id: "other".to_owned(),
                email: None,
                name: None,
                role: "user".to_owned(),
            }),
        )
        .await
        .unwrap();
        assert!(hidden.0.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn consumer_lifecycle_never_carries_the_degradation_switch() {
        let state = crate::test_state("http://upstream.invalid").await;
        seed_users(&state).await;

        let created = create_consumer(
            State(state.clone()),
            Extension(user_identity()),
            Json(CreateConsumer {
                name: "app".to_owned(),
                request_archive: true,
            }),
        )
        .await
        .unwrap();
        let id = created.0["id"].as_str().unwrap().to_owned();
        let secret = created.0["secret"].as_str().unwrap().to_owned();
        assert!(secret.starts_with("sk-"));
        assert_eq!(created.0["is_disabled"], false);

        let verified = verify_consumer_credential(
            State(state.clone()),
            Extension(user_identity()),
            Path(id.clone()),
            Json(VerifyConsumerCredential {
                secret: secret.clone(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(verified.0["credential_matches"], true);
        assert_eq!(verified.0["request_archive"], true);

        let _ = update_consumer(
            State(state.clone()),
            Extension(user_identity()),
            Path(id.clone()),
            Json(UpdateConsumer {
                name: Some("app-2".to_owned()),
                request_archive: Some(false),
                is_disabled: Some(true),
            }),
        )
        .await
        .unwrap();
        let listed = list_consumers(State(state.clone()), Extension(user_identity()))
            .await
            .unwrap();
        assert_eq!(listed.0[0]["name"], "app-2");
        assert_eq!(listed.0[0]["is_disabled"], true);

        let rotated = rotate_consumer(
            State(state.clone()),
            Extension(user_identity()),
            Path(id.clone()),
        )
        .await
        .unwrap();
        assert_ne!(rotated.0["secret"], secret);
        let old_secret_matches: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM consumers WHERE id=? AND secret_hash=?")
                .bind(&id)
                .bind(consumer_secret_hash(&secret))
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(old_secret_matches, 0);

        let _ = delete_consumer(
            State(state.clone()),
            Extension(user_identity()),
            Path(id.clone()),
        )
        .await
        .unwrap();
        let deleted: i64 = sqlx::query_scalar("SELECT is_deleted FROM consumers WHERE id=?")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
    }

    #[tokio::test]
    async fn settings_updates_persist_into_app_meta_and_config() {
        let state = crate::test_state("http://upstream.invalid").await;
        seed_users(&state).await;

        let _ = update_settings(
            State(state.clone()),
            peer(),
            Extension(root_identity()),
            Json(UpdateSettings {
                upstream_base: "https://api.deepseek.com/".to_owned(),
                allow_all_users_debt: true,
                response_body_limit: 4_194_304,
                affinity_ttl_seconds: 7_200,
                request_archive_retention_days: 3,
                model_price_multiplier: "1.2".to_owned(),
            }),
        )
        .await
        .unwrap();
        let config = state.config.load_full();
        assert_eq!(config.upstream_base, "https://api.deepseek.com");
        assert!(config.allow_all_users_debt);
        assert_eq!(config.response_body_limit, 4_194_304);
        assert_eq!(config.affinity_ttl_seconds, 7_200);
        assert_eq!(config.request_archive_retention_days, 3);
        assert_eq!(config.model_price_multiplier_nanos, 1_200_000_000);

        let _ = update_available_model_ids(
            State(state.clone()),
            peer(),
            Extension(root_identity()),
            Json(UpdateAvailableModelIds {
                available_model_ids: vec![
                    " deepseek-flash ".to_owned(),
                    "deepseek-v4-pro".to_owned(),
                ],
            }),
        )
        .await
        .unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='available_model_ids'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(stored, r#"["deepseek-flash","deepseek-v4-pro"]"#);
        assert_eq!(
            state.config.load().available_model_ids,
            vec!["deepseek-flash".to_owned(), "deepseek-v4-pro".to_owned()]
        );

        let _ = update_provider_concurrency(
            State(state.clone()),
            peer(),
            Extension(root_identity()),
            Json(UpdateProviderConcurrency {
                provider_concurrency_limit: 8,
            }),
        )
        .await
        .unwrap();
        assert_eq!(state.config.load().provider_concurrency_limit, 8);
        let audit_actions: Vec<String> =
            sqlx::query_scalar("SELECT action FROM admin_audit ORDER BY created_at, action")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(
            audit_actions,
            vec![
                "settings.available_models.update",
                "settings.provider_concurrency.update",
                "settings.update",
            ]
        );
    }

    #[tokio::test]
    async fn audit_rows_expose_peak_pricing_and_filters() {
        let state = crate::test_state("http://upstream.invalid").await;
        seed_users(&state).await;
        seed_consumer(&state, "consumer", "user", "sk-consumer").await;
        seed_provider(&state, "provider", "user").await;
        for (id, peak) in [("peak-call", 1_i64), ("off-peak-call", 0)] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,peak,status,latency_ms,actual_cost_usd_nanos,created_at) VALUES(?,?, 'consumer','user','provider','POST','/v1/chat/completions','deepseek-flash',?,200,10,7,unixepoch())")
                .bind(id)
                .bind(id)
                .bind(peak)
                .execute(&state.db)
                .await
                .unwrap();
        }

        let listed = audit(
            State(state.clone()),
            Extension(user_identity()),
            Query(AuditQuery {
                limit: None,
                offset: None,
                user_id: None,
                consumer: None,
                provider: Some("provider".to_owned()),
                model: Some("deepseek-flash".to_owned()),
                status: None,
                error_code: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(listed.0["total"], 2);
        let rows = listed.0["rows"].as_array().unwrap();
        assert_eq!(rows[0]["peak"], true, "newest row first");
        assert_eq!(rows[0]["provider_name"], "provider");
        assert_eq!(rows[1]["peak"], false);

        let errors = audit(
            State(state.clone()),
            Extension(user_identity()),
            Query(AuditQuery {
                limit: None,
                offset: None,
                user_id: None,
                consumer: None,
                provider: None,
                model: None,
                status: Some(AuditStatus::Error),
                error_code: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(errors.0["total"], 0);

        let dashboard = dashboard(State(state.clone()), Extension(user_identity()))
            .await
            .unwrap();
        assert_eq!(dashboard.0["calls_24h"], 2);
        assert_eq!(dashboard.0["peak_calls_24h"], 1);
        assert_eq!(
            dashboard.0["active_providers"], 1,
            "the caller owns one provider"
        );
        assert_eq!(dashboard.0["available_model_ids"][0], "deepseek-flash");
    }

    #[tokio::test]
    async fn model_prices_only_list_available_models() {
        let state = crate::test_state("http://upstream.invalid").await;
        sqlx::query(
            "UPDATE app_meta SET value='[\"deepseek-v4-pro\"]' WHERE key='available_model_ids'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let mut config = (**state.config.load()).clone();
        config.available_model_ids = vec!["deepseek-v4-pro".to_owned()];
        state.config.store(std::sync::Arc::new(config));

        let prices = model_prices(State(state.clone())).await;
        let rows = prices.0["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["model"], "deepseek-v4-pro");
        assert!(
            rows[0]["peak"]["cache_hit_usd_nanos"].as_i64().unwrap()
                < rows[0]["peak"]["cache_miss_usd_nanos"].as_i64().unwrap()
        );
        assert!(
            rows[0]["off_peak"]["output_usd_nanos"].as_i64().unwrap()
                < rows[0]["peak"]["output_usd_nanos"].as_i64().unwrap(),
            "off-peak is cheaper than peak"
        );
    }

    #[tokio::test]
    async fn provider_audit_groups_requests_by_hour_and_provider() {
        let state = crate::test_state("http://upstream.invalid").await;
        seed_users(&state).await;
        seed_consumer(&state, "consumer", "user", "sk-consumer").await;
        seed_provider(&state, "provider", "user").await;
        for (id, status, error_code) in
            [("ok", 200_i64, None), ("failed", 500, Some("server_error"))]
        {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,peak,status,latency_ms,input_tokens,actual_cost_usd_nanos,error_code,created_at) VALUES(?,?, 'consumer','user','provider','POST','/v1/chat/completions','deepseek-flash',0,?,10,11,3,?,unixepoch())")
                .bind(id)
                .bind(id)
                .bind(status)
                .bind(error_code)
                .execute(&state.db)
                .await
                .unwrap();
        }

        let report = provider_audit(
            State(state.clone()),
            Extension(root_identity()),
            Query(UsageQuery { period: None }),
        )
        .await
        .unwrap();
        let rows = report.0["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["requests"], 2);
        assert_eq!(rows[0]["successful_requests"], 1);
        assert_eq!(rows[0]["failed_requests"], 1);
        assert_eq!(rows[0]["input_tokens"], 22);
        assert_eq!(rows[0]["actual_cost_usd_nanos"], 6);
        assert_eq!(rows[0]["provider_name"], "provider");
    }
}
