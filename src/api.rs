use std::{collections::HashSet, net::SocketAddr, time::Duration};

use axum::{
    Json,
    extract::{ConnectInfo, Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::{StreamExt, stream};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite};
use uuid::Uuid;

use crate::{
    AppError, AppState,
    auth::{UserIdentity, bearer, is_admin, require_admin, require_root},
    balancer::{self, Provider},
    config,
    crypto::consumer_secret_hash,
    identity, midas, oauth, payments, proxy,
    resources::SystemResourcesSnapshot,
};

const PROVIDER_CAPACITY_POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);
const PROVIDER_CAPACITY_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const PROVIDER_CAPACITY_POLL_CONCURRENCY: usize = 8;
const PROVIDER_CAPACITY_HISTORY_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

fn validated_proxy_url(value: Option<&str>) -> Result<Option<String>, AppError> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    let Some(value) = value else { return Ok(None) };
    let url = url::Url::parse(value)
        .map_err(|_| AppError::bad_request("invalid provider HTTP proxy URL"))?;
    if url.scheme() != "http" || url.host_str().is_none() {
        return Err(AppError::bad_request(
            "provider proxy must be an http:// URL",
        ));
    }
    Ok(Some(value.to_owned()))
}

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
        "source_url": "https://developers.openai.com/api/docs/pricing",
        "source_as_of": "2026-09-28",
        "unit": "USD per 1M tokens",
        "rows": proxy::official_model_prices(&config.available_model_ids)
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
    #[serde(default)]
    intercept_degradation: bool,
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
    intercept_degradation: Option<bool>,
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
    let rows = sqlx::query("SELECT id,name,prefix,created_at,last_used_at,request_archive,intercept_degradation,is_disabled FROM consumers WHERE user_id=? AND is_system=0 AND is_deleted=0 ORDER BY created_at DESC")
        .bind(&user.id).fetch_all(&state.db).await?;
    Ok(Json(Value::Array(rows.into_iter().map(|row| json!({
        "id": row.get::<String,_>(0), "name": row.get::<String,_>(1), "prefix": row.get::<String,_>(2),
        "created_at": row.get::<i64,_>(3), "last_used_at": row.get::<Option<i64>,_>(4),
        "request_archive": row.get::<i64,_>(5) != 0,
        "intercept_degradation": row.get::<i64,_>(6) != 0,
        "is_disabled": row.get::<i64,_>(7) != 0
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
        "INSERT INTO consumers(id,user_id,name,prefix,secret_hash,request_archive,intercept_degradation,created_at) VALUES(?,?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(name)
    .bind(&prefix)
    .bind(consumer_secret_hash(&secret))
    .bind(input.request_archive)
    .bind(input.intercept_degradation)
    .bind(chrono::Utc::now().timestamp())
    .execute(&state.db)
    .await?;
    Ok(Json(
        json!({"id":id,"name":name,"prefix":prefix,"secret":secret,"request_archive":input.request_archive,"intercept_degradation":input.intercept_degradation}),
    ))
}

pub async fn update_consumer(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
    Json(input): Json<UpdateConsumer>,
) -> Result<Json<Value>, AppError> {
    if input.name.is_none()
        && input.request_archive.is_none()
        && input.intercept_degradation.is_none()
        && input.is_disabled.is_none()
    {
        return Err(AppError::bad_request("consumer update is empty"));
    }
    let current = sqlx::query(
        "SELECT name,request_archive,is_disabled,intercept_degradation FROM consumers WHERE id=? AND user_id=? AND is_system=0 AND is_deleted=0",
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
    let intercept_degradation = input
        .intercept_degradation
        .unwrap_or_else(|| current.get::<i64, _>(3) != 0);
    let _write = state.write_gate.lock().await;
    let updated = sqlx::query(
        "UPDATE consumers SET name=?,request_archive=?,is_disabled=?,intercept_degradation=? WHERE id=? AND user_id=? AND is_deleted=0",
    )
    .bind(&name)
    .bind(request_archive)
    .bind(is_disabled)
    .bind(intercept_degradation)
    .bind(&id)
    .bind(&user.id)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::not_found("consumer not found"));
    }
    Ok(Json(
        json!({"id":id,"name":name,"request_archive":request_archive,"is_disabled":is_disabled,"intercept_degradation":intercept_degradation}),
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
    access_key: String,
    refresh_key: String,
    #[serde(default)]
    originator: Option<String>,
    #[serde(default)]
    allow_other_originator: bool,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    http_proxy_url: Option<String>,
}

#[derive(FromRow, Serialize)]
pub struct ProviderCircuitEvent {
    id: String,
    provider_id: String,
    cause: String,
    rate_limit_json: String,
    opened_at: i64,
    cooldown_until: i64,
    closed_at: Option<i64>,
    resolution: Option<String>,
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
    Ok(Json(Value::Array(
        providers
            .into_iter()
            .map(|provider| {
                let load = state.balancer.load(&provider.id);
                let proxy_configured = provider.http_proxy_url.is_some();
                let mut value = serde_json::to_value(provider).expect("provider serializes");
                value["inflight"] = json!(load.inflight);
                value["queued"] = json!(load.queued);
                value["concurrency_limit"] = json!(state.config.load().provider_concurrency_limit);
                value["http_proxy_configured"] = json!(proxy_configured);
                value
            })
            .collect(),
    )))
}

pub async fn list_provider_circuit_summaries(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    let rows = if is_admin(&user) {
        sqlx::query_as::<_, ProviderCircuitEvent>("SELECT e.id,e.provider_id,e.cause,e.rate_limit_json,e.opened_at,e.cooldown_until,e.closed_at,e.resolution FROM provider_circuit_events e JOIN providers p ON p.id=e.provider_id WHERE p.is_deleted=0 AND e.id=(SELECT latest.id FROM provider_circuit_events latest WHERE latest.provider_id=e.provider_id ORDER BY latest.opened_at DESC,latest.id DESC LIMIT 1) ORDER BY e.opened_at DESC,e.id DESC")
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as::<_, ProviderCircuitEvent>("SELECT e.id,e.provider_id,e.cause,e.rate_limit_json,e.opened_at,e.cooldown_until,e.closed_at,e.resolution FROM provider_circuit_events e JOIN providers p ON p.id=e.provider_id WHERE p.owner_id=? AND p.is_deleted=0 AND e.id=(SELECT latest.id FROM provider_circuit_events latest WHERE latest.provider_id=e.provider_id ORDER BY latest.opened_at DESC,latest.id DESC LIMIT 1) ORDER BY e.opened_at DESC,e.id DESC")
            .bind(&user.id)
            .fetch_all(&state.db)
            .await?
    };
    let mut providers = serde_json::Map::new();
    for event in rows {
        providers.insert(
            event.provider_id.clone(),
            serde_json::to_value(event).expect("circuit event serializes"),
        );
    }
    Ok(Json(json!({"providers": providers})))
}

pub async fn list_provider_circuit_events(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProviderCircuitEvent>>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    let events = sqlx::query_as::<_, ProviderCircuitEvent>("SELECT id,provider_id,cause,rate_limit_json,opened_at,cooldown_until,closed_at,resolution FROM provider_circuit_events WHERE provider_id=? ORDER BY opened_at DESC,id DESC LIMIT 100")
        .bind(&id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(events))
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
        ProviderCredentials {
            access: &input.access_key,
            refresh: &input.refresh_key,
            expires_at: None,
        },
        input.http_proxy_url.as_deref(),
        ProviderRouting {
            originator: &identity::provider_originator(input.originator.as_deref())?,
            allow_other_originator: input.allow_other_originator,
            visibility: &balancer::provider_visibility(input.visibility.as_deref())?,
        },
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

/// OAuth credentials a new provider is registered with.
struct ProviderCredentials<'a> {
    access: &'a str,
    refresh: &'a str,
    expires_at: Option<i64>,
}

/// Routing policy a new provider is registered with.
struct ProviderRouting<'a> {
    originator: &'a str,
    allow_other_originator: bool,
    visibility: &'a str,
}

async fn insert_provider(
    state: &AppState,
    owner_id: &str,
    name: &str,
    credentials: ProviderCredentials<'_>,
    http_proxy_url: Option<&str>,
    routing: ProviderRouting<'_>,
) -> Result<Json<Value>, AppError> {
    let ProviderCredentials {
        access,
        refresh,
        expires_at,
    } = credentials;
    let ProviderRouting {
        originator,
        allow_other_originator,
        visibility,
    } = routing;
    if access.trim().is_empty() || refresh.trim().is_empty() {
        return Err(AppError::bad_request(
            "access_key and refresh_key are required",
        ));
    }
    let account_id = oauth::account_id_from_jwt(access)?;
    let id = Uuid::new_v4().to_string();
    let name = provider_name(name, &id);
    let now = chrono::Utc::now().timestamp();
    let expires_at = expires_at.or_else(|| oauth::expires_at_from_jwt(access));
    {
        let _write = state.write_gate.lock().await;
        let http_proxy_url = validated_proxy_url(http_proxy_url)?;
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,expires_at,status,created_at,updated_at,owner_id,http_proxy_url,originator,allow_other_originator,visibility) VALUES(?,?,?,?,?,?,'active',?,?,?,?,?,?,?)")
            .bind(&id).bind(&name).bind(&account_id).bind(access.trim())
            .bind(refresh.trim()).bind(expires_at).bind(now).bind(now).bind(owner_id).bind(http_proxy_url)
            .bind(originator)
            .bind(allow_other_originator)
            .bind(visibility)
            .execute(&state.db).await?;
    }
    state.balancer.reload_providers(&state.db).await?;
    Ok(Json(
        json!({"id":id,"name":name,"account_id":account_id,"owner_id":owner_id,"status":"active","originator":originator,"allow_other_originator":allow_other_originator,"visibility":visibility}),
    ))
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
    refresh: Option<bool>,
    http_proxy_url: Option<String>,
    allow_other_originator: Option<bool>,
    /// Rejected when present: a provider keeps the originator its credentials were authorized for.
    originator: Option<String>,
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
        if input.originator.is_some() {
            return Err(AppError::bad_request(
                "originator cannot be changed after a provider is created",
            ));
        }
        let visibility = input
            .visibility
            .as_deref()
            .map(|value| balancer::provider_visibility(Some(value)))
            .transpose()?;
        if input.name.is_some() || visibility.is_some() || input.allow_other_originator.is_some() {
            let name = input.name.as_deref().map(|name| provider_name(name, &id));
            let _write = state.write_gate.lock().await;
            sqlx::query("UPDATE providers SET name=COALESCE(?,name),visibility=COALESCE(?,visibility),allow_other_originator=COALESCE(?,allow_other_originator),updated_at=? WHERE id=? AND is_deleted=0")
                .bind(name)
                .bind(visibility)
                .bind(input.allow_other_originator)
                .bind(chrono::Utc::now().timestamp())
                .bind(&id)
                .execute(&state.db)
                .await?;
        }
        if let Some(enabled) = input.enabled {
            let (disabled, status) = if enabled { (0, "active") } else { (1, "disabled") };
            let _write = state.write_gate.lock().await;
            sqlx::query("UPDATE providers SET manual_disabled=?,status=?,cooldown_until=NULL,updated_at=? WHERE id=? AND is_deleted=0")
                .bind(disabled).bind(status).bind(chrono::Utc::now().timestamp()).bind(&id).execute(&state.db).await?;
        }
        if input.http_proxy_url.is_some() {
            let value = validated_proxy_url(input.http_proxy_url.as_deref())?;
            let _write = state.write_gate.lock().await;
            sqlx::query("UPDATE providers SET http_proxy_url=?,updated_at=? WHERE id=? AND is_deleted=0")
                .bind(value).bind(chrono::Utc::now().timestamp()).bind(&id).execute(&state.db).await?;
            state.proxy_clients.remove(&id);
        }
        if input.refresh.unwrap_or(false) {
            refresh_provider(&state, &id).await?;
        }
        state.balancer.reload_providers(&state.db).await?;
        Ok(Json(json!({"ok":true})))
    }.await;
    let action = if operation.is_ok() {
        "provider.update"
    } else {
        "provider.update.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    operation
}

#[derive(Serialize)]
pub struct ProviderProxyHealth {
    proxy_configured: bool,
    lb_to_proxy_ms: Option<u128>,
    proxy_to_openai_ms: Option<u128>,
    location: Option<Value>,
    error: Option<String>,
}

pub async fn provider_proxy_health(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<ProviderProxyHealth>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    let proxy_url: Option<String> =
        sqlx::query_scalar("SELECT http_proxy_url FROM providers WHERE id=? AND is_deleted=0")
            .bind(&id)
            .fetch_one(&state.db)
            .await?;
    let Some(proxy_url) = proxy_url else {
        return Ok(Json(ProviderProxyHealth {
            proxy_configured: false,
            lb_to_proxy_ms: None,
            proxy_to_openai_ms: None,
            location: None,
            error: None,
        }));
    };
    let proxy = url::Url::parse(&proxy_url)
        .map_err(|_| AppError::bad_request("invalid provider HTTP proxy URL"))?;
    let host = proxy
        .host_str()
        .ok_or_else(|| AppError::bad_request("invalid provider HTTP proxy URL"))?;
    let address = format!("{}:{}", host, proxy.port_or_known_default().unwrap_or(80));
    let started = std::time::Instant::now();
    let stream = tokio::net::TcpStream::connect(address).await;
    let lb_to_proxy_ms = started.elapsed().as_millis();
    if let Err(error) = stream {
        return Ok(Json(ProviderProxyHealth {
            proxy_configured: true,
            lb_to_proxy_ms: None,
            proxy_to_openai_ms: None,
            location: None,
            error: Some(error.to_string()),
        }));
    }
    let provider: Provider = sqlx::query_as("SELECT * FROM providers WHERE id=? AND is_deleted=0")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    let started = std::time::Instant::now();
    let result = state
        .provider_client(&provider)?
        .get(&state.config.load().upstream_base)
        .send()
        .await;
    let proxy_to_openai_ms = started.elapsed().as_millis();
    let error = result.err().map(|error| error.to_string());
    let location = if error.is_none() {
        match state.provider_client(&provider)?.get("https://ipinfo.io/json").send().await {
            Ok(response) => response.json::<Value>().await.ok().map(|value| json!({"ip":value.get("ip"),"city":value.get("city"),"region":value.get("region"),"country":value.get("country"),"org":value.get("org")})),
            Err(_) => None,
        }
    } else {
        None
    };
    Ok(Json(ProviderProxyHealth {
        proxy_configured: true,
        lb_to_proxy_ms: Some(lb_to_proxy_ms),
        proxy_to_openai_ms: error.is_none().then_some(proxy_to_openai_ms),
        location,
        error,
    }))
}

async fn refresh_provider(state: &AppState, id: &str) -> Result<(), AppError> {
    let lock = state
        .refresh_locks
        .entry(id.to_owned())
        .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let _guard = lock.lock().await;
    let row =
        sqlx::query("SELECT refresh_token,account_id FROM providers WHERE id=? AND is_deleted=0")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("provider not found"))?;
    let refresh: String = row.get(0);
    let token = oauth::refresh(state, &refresh).await?;
    let account_id =
        oauth::account_id_from_jwt(&token.access_token).unwrap_or_else(|_| row.get::<String, _>(1));
    let now = chrono::Utc::now().timestamp();
    let updated = {
        let _write = state.write_gate.lock().await;
        sqlx::query("UPDATE providers SET access_token=?,refresh_token=?,account_id=?,expires_at=?,status=CASE WHEN manual_disabled=1 THEN 'disabled' ELSE 'active' END,cooldown_until=NULL,last_error=NULL,updated_at=? WHERE id=? AND refresh_token=? AND is_deleted=0")
            .bind(&token.access_token)
            .bind(&token.refresh_token)
            .bind(account_id).bind(now + token.expires_in).bind(now).bind(id).bind(refresh).execute(&state.db).await?
    };
    if updated.rows_affected() == 0 {
        return Err(AppError::unavailable(
            "provider credential changed during refresh",
        ));
    }
    state.balancer.reload_providers(&state.db).await?;
    Ok(())
}

pub async fn read_provider_tokens(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<(HeaderMap, Json<Value>), AppError> {
    require_provider_manager(&state, &user, &id).await?;
    require_admin(&user)?;
    let result: Result<(HeaderMap, Json<Value>), AppError> = async {
        let row: (String, String) = sqlx::query_as(
            "SELECT access_token,refresh_token FROM providers WHERE id=? AND is_deleted=0",
        )
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("provider not found"))?;
        let mut response_headers = HeaderMap::new();
        response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        Ok((
            response_headers,
            Json(json!({"access_key":row.0,"refresh_key":row.1})),
        ))
    }
    .await;
    let action = if result.is_ok() {
        "provider.tokens.read"
    } else {
        "provider.tokens.read.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    result
}

#[derive(Deserialize)]
pub struct ReplaceProviderTokens {
    name: Option<String>,
    access_key: String,
    refresh_key: String,
}

pub async fn replace_provider_tokens(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
    Json(input): Json<ReplaceProviderTokens>,
) -> Result<Json<Value>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    require_admin(&user)?;
    let result = replace_provider_token_values(
        &state,
        &id,
        input.name.as_deref().map(str::trim),
        input.access_key.trim(),
        input.refresh_key.trim(),
    )
    .await;
    let action = if result.is_ok() {
        "provider.tokens.update"
    } else {
        "provider.tokens.update.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    result
}

async fn replace_provider_token_values(
    state: &AppState,
    id: &str,
    name: Option<&str>,
    access_token: &str,
    refresh_token: &str,
) -> Result<Json<Value>, AppError> {
    if access_token.is_empty() || refresh_token.is_empty() {
        return Err(AppError::bad_request(
            "access_key and refresh_key are required",
        ));
    }
    let account_id = oauth::account_id_from_jwt(access_token)?;
    let name = match name {
        Some(name) => provider_name(name, id),
        None => sqlx::query_scalar("SELECT name FROM providers WHERE id=? AND is_deleted=0")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("provider not found"))?,
    };
    let expires_at = oauth::expires_at_from_jwt(access_token);
    let updated = {
        let _write = state.write_gate.lock().await;
        sqlx::query("UPDATE providers SET name=?,access_token=?,refresh_token=?,account_id=?,expires_at=?,status=CASE WHEN manual_disabled=1 THEN 'disabled' ELSE 'active' END,cooldown_until=NULL,last_error=NULL,updated_at=? WHERE id=? AND is_deleted=0")
            .bind(&name).bind(access_token).bind(refresh_token).bind(&account_id).bind(expires_at)
            .bind(chrono::Utc::now().timestamp()).bind(id).execute(&state.db).await?
    };
    if updated.rows_affected() == 0 {
        return Err(AppError::not_found("provider not found"));
    }
    state.balancer.reload_providers(&state.db).await?;
    Ok(Json(json!({"ok":true,"name":name,"account_id":account_id})))
}

pub async fn list_provider_usage(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    let ids = if is_admin(&user) {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM providers WHERE is_deleted=0 ORDER BY created_at DESC",
        )
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM providers WHERE owner_id=? AND is_deleted=0 ORDER BY created_at DESC",
        )
        .bind(&user.id)
        .fetch_all(&state.db)
        .await?
    };
    let mut providers = serde_json::Map::new();
    for id in ids {
        let value = match provider_usage_value(&state, &id).await {
            Ok(usage) => json!({"usage": usage}),
            Err(error) => json!({"error": error.message()}),
        };
        providers.insert(id, value);
    }
    Ok(Json(json!({"providers": providers})))
}

pub fn start_provider_capacity_polling(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(PROVIDER_CAPACITY_POLL_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(error) = poll_provider_capacity(&state).await {
                tracing::error!(%error, "provider capacity polling failed");
            }
        }
    });
}

async fn poll_provider_capacity(state: &AppState) -> Result<(), AppError> {
    let ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM providers WHERE is_deleted=0 ORDER BY created_at,id",
    )
    .fetch_all(&state.db)
    .await?;
    let results = stream::iter(ids.into_iter().map(|id| async {
        let result = match tokio::time::timeout(
            PROVIDER_CAPACITY_REQUEST_TIMEOUT,
            provider_usage_value(state, &id),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AppError::upstream(504, "provider Usage API timed out")),
        };
        (id, result)
    }))
    .buffer_unordered(PROVIDER_CAPACITY_POLL_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;
    for (id, result) in results {
        match result {
            Ok(usage) => record_provider_capacity_usage(state, &id, &usage).await?,
            Err(error) => record_provider_capacity_error(state, &id, error.message()).await?,
        }
    }
    record_provider_capacity_history(state).await?;
    Ok(())
}

async fn record_provider_capacity_usage(
    state: &AppState,
    provider_id: &str,
    usage: &Value,
) -> Result<(), AppError> {
    let (upstream_user_id, plan_type, remaining_basis_points) = provider_capacity_values(usage);
    let last_error = remaining_basis_points
        .is_none()
        .then_some("provider Usage API did not return a primary window used_percent");
    let now = chrono::Utc::now().timestamp();
    let _write = state.write_gate.lock().await;
    sqlx::query(
        "INSERT INTO provider_capacity_snapshots(provider_id,upstream_user_id,plan_type,remaining_basis_points,last_success_at,last_attempt_at,last_error) SELECT ?,?,?,?,?,?,? WHERE EXISTS(SELECT 1 FROM providers WHERE id=? AND is_deleted=0) ON CONFLICT(provider_id) DO UPDATE SET upstream_user_id=excluded.upstream_user_id,plan_type=excluded.plan_type,remaining_basis_points=excluded.remaining_basis_points,last_success_at=excluded.last_success_at,last_attempt_at=excluded.last_attempt_at,last_error=excluded.last_error",
    )
    .bind(provider_id)
    .bind(upstream_user_id)
    .bind(plan_type)
    .bind(remaining_basis_points)
    .bind(now)
    .bind(now)
    .bind(last_error)
    .bind(provider_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

async fn record_provider_capacity_error(
    state: &AppState,
    provider_id: &str,
    error: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();
    let _write = state.write_gate.lock().await;
    sqlx::query(
        "INSERT INTO provider_capacity_snapshots(provider_id,last_attempt_at,last_error) SELECT ?,?,? WHERE EXISTS(SELECT 1 FROM providers WHERE id=? AND is_deleted=0) ON CONFLICT(provider_id) DO UPDATE SET last_attempt_at=excluded.last_attempt_at,last_error=excluded.last_error",
    )
    .bind(provider_id)
    .bind(now)
    .bind(error)
    .bind(provider_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

fn provider_capacity_values(usage: &Value) -> (Option<String>, Option<String>, Option<i64>) {
    let upstream_user_id = usage
        .get("user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|user_id| !user_id.is_empty())
        .map(str::to_owned);
    let plan_type = usage
        .get("plan_type")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let remaining_basis_points = usage
        .pointer("/rate_limit/primary_window/used_percent")
        .and_then(Value::as_f64)
        .filter(|used_percent| used_percent.is_finite())
        .map(|used_percent| ((100.0 - used_percent.clamp(0.0, 100.0)) * 100.0).round() as i64);
    (upstream_user_id, plan_type, remaining_basis_points)
}

fn plus_equivalent_multiplier(plan_type: Option<&str>) -> Option<i64> {
    match plan_type?.trim().to_ascii_lowercase().as_str() {
        "plus" => Some(1),
        "prolite" => Some(5),
        "pro" => Some(20),
        _ => None,
    }
}

struct ProviderCapacitySummary {
    provider_count: i64,
    included_provider_count: i64,
    plus_equivalent_remaining_basis_points: i64,
    last_sampled_at: Option<i64>,
}

async fn provider_capacity_summary(state: &AppState) -> Result<ProviderCapacitySummary, AppError> {
    let rows = sqlx::query(
        "SELECT p.id,s.upstream_user_id,s.plan_type,s.remaining_basis_points,s.last_success_at,s.last_error FROM providers p LEFT JOIN provider_capacity_snapshots s ON s.provider_id=p.id WHERE p.is_deleted=0 ORDER BY p.created_at,p.id",
    )
    .fetch_all(&state.db)
    .await?;
    let mut plus_equivalent_remaining_basis_points = 0;
    let mut included_provider_count = 0;
    let mut last_sampled_at = None;
    let mut seen_upstream_user_ids = HashSet::new();
    let provider_count = rows.len() as i64;
    for row in rows {
        let provider_id = row.get::<String, _>(0);
        let upstream_user_id = row.get::<Option<String>, _>(1);
        let plan_type = row.get::<Option<String>, _>(2);
        let remaining_basis_points = row.get::<Option<i64>, _>(3);
        let last_success_at = row.get::<Option<i64>, _>(4);
        let last_error = row.get::<Option<String>, _>(5);
        last_sampled_at = last_sampled_at.max(last_success_at);
        let dedupe_key = upstream_user_id
            .as_deref()
            .map(|user_id| format!("user:{user_id}"))
            .unwrap_or_else(|| format!("provider:{provider_id}"));
        if !seen_upstream_user_ids.insert(dedupe_key) {
            continue;
        }
        let provider_plus_equivalent_remaining_basis_points = remaining_basis_points
            .zip(plus_equivalent_multiplier(plan_type.as_deref()))
            .filter(|_| last_error.is_none())
            .map(|(remaining, multiplier)| remaining * multiplier);
        if let Some(remaining) = provider_plus_equivalent_remaining_basis_points {
            included_provider_count += 1;
            plus_equivalent_remaining_basis_points += remaining;
        }
    }
    Ok(ProviderCapacitySummary {
        provider_count,
        included_provider_count,
        plus_equivalent_remaining_basis_points,
        last_sampled_at,
    })
}

async fn record_provider_capacity_history(state: &AppState) -> Result<(), AppError> {
    let summary = provider_capacity_summary(state).await?;
    if summary.included_provider_count == 0 {
        return Ok(());
    }
    let sampled_at = chrono::Utc::now().timestamp();
    let _write = state.write_gate.lock().await;
    sqlx::query(
        "INSERT INTO provider_capacity_history(sampled_at,plus_equivalent_remaining_basis_points,included_provider_count,provider_count) VALUES(?,?,?,?) ON CONFLICT(sampled_at) DO UPDATE SET plus_equivalent_remaining_basis_points=excluded.plus_equivalent_remaining_basis_points,included_provider_count=excluded.included_provider_count,provider_count=excluded.provider_count",
    )
    .bind(sampled_at)
    .bind(summary.plus_equivalent_remaining_basis_points)
    .bind(summary.included_provider_count)
    .bind(summary.provider_count)
    .execute(&state.db)
    .await?;
    Ok(())
}

pub async fn provider_capacity(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let summary = provider_capacity_summary(&state).await?;
    let oldest_sampled_at =
        chrono::Utc::now().timestamp() - PROVIDER_CAPACITY_HISTORY_WINDOW_SECONDS;
    let history = sqlx::query(
        "SELECT sampled_at,plus_equivalent_remaining_basis_points FROM provider_capacity_history WHERE sampled_at>=? ORDER BY sampled_at",
    )
    .bind(oldest_sampled_at)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "sampled_at":row.get::<i64,_>(0),
            "plus_equivalent_remaining_basis_points":row.get::<i64,_>(1)
        })
    })
    .collect::<Vec<_>>();
    Ok(Json(json!({
        "provider_count":summary.provider_count,
        "included_provider_count":summary.included_provider_count,
        "plus_equivalent_remaining_basis_points":summary.plus_equivalent_remaining_basis_points,
        "last_sampled_at":summary.last_sampled_at,
        "history":history
    })))
}

pub async fn list_provider_rate_limit_resets(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    let ids = if is_admin(&user) {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM providers WHERE is_deleted=0 ORDER BY created_at DESC",
        )
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM providers WHERE owner_id=? AND is_deleted=0 ORDER BY created_at DESC",
        )
        .bind(&user.id)
        .fetch_all(&state.db)
        .await?
    };
    let mut providers = serde_json::Map::new();
    for id in ids {
        let value = match provider_rate_limit_resets_value(&state, &id).await {
            Ok(resets) => json!({"resets": resets}),
            Err(error) => json!({"error": error.message()}),
        };
        providers.insert(id, value);
    }
    Ok(Json(json!({"providers": providers})))
}

pub async fn test_provider(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    let result = provider_usage(&state, &id).await;
    let action = if result.is_ok() {
        "provider.test"
    } else {
        "provider.test.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    result
}

async fn provider_usage(state: &AppState, id: &str) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"ok":true,"usage":provider_usage_value(state, id).await?}),
    ))
}

async fn provider_usage_value(state: &AppState, id: &str) -> Result<Value, AppError> {
    let row: Provider = sqlx::query_as("SELECT * FROM providers WHERE id=? AND is_deleted=0")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("provider not found"))?;
    let mut usage_url = url::Url::parse(&state.config.load().upstream_base)?;
    usage_url.set_path("/backend-api/wham/usage");
    usage_url.set_query(None);
    let user_agent = state
        .config
        .load()
        .upstream_user_agent_for(&row.originator)
        .map(str::to_owned);
    let mut request = state
        .provider_client(&row)?
        .get(usage_url)
        .bearer_auth(&row.access_token)
        .header("chatgpt-account-id", &row.account_id)
        .header(identity::ORIGINATOR_HEADER, &row.originator);
    if let Some(user_agent) = user_agent {
        request = request.header(header::USER_AGENT, user_agent);
    }
    let response = request
        .send()
        .await
        .map_err(|_| AppError::upstream(502, "provider Usage API request failed"))?;
    if !response.status().is_success() {
        return Err(AppError::upstream(
            502,
            format!("provider Usage API returned {}", response.status()),
        ));
    }
    let usage = response
        .json::<Value>()
        .await
        .map_err(|_| AppError::upstream(502, "provider Usage API returned invalid JSON"))?;
    Ok(usage)
}

async fn provider_rate_limit_resets_value(state: &AppState, id: &str) -> Result<Value, AppError> {
    let row: (String, String, String) = sqlx::query_as(
        "SELECT access_token,account_id,originator FROM providers WHERE id=? AND is_deleted=0",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("provider not found"))?;
    let mut resets_url = url::Url::parse(&state.config.load().upstream_base)?;
    resets_url.set_path("/backend-api/wham/rate-limit-reset-credits");
    resets_url.set_query(None);
    let user_agent = state
        .config
        .load()
        .upstream_user_agent_for(&row.2)
        .map(str::to_owned);
    let mut request = state
        .client
        .get(resets_url)
        .bearer_auth(&row.0)
        .header("chatgpt-account-id", &row.1)
        .header(identity::ORIGINATOR_HEADER, &row.2);
    if let Some(user_agent) = user_agent {
        request = request.header(header::USER_AGENT, user_agent);
    }
    let response = request
        .send()
        .await
        .map_err(|_| AppError::upstream(502, "provider rate-limit reset API request failed"))?;
    if !response.status().is_success() {
        return Err(AppError::upstream(
            502,
            format!(
                "provider rate-limit reset API returned {}",
                response.status()
            ),
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|_| AppError::upstream(502, "provider rate-limit reset API returned invalid JSON"))
}

#[derive(Deserialize)]
pub struct ConsumeProviderRateLimitReset {
    credit_id: Option<String>,
    redeem_request_id: String,
}

pub async fn consume_provider_rate_limit_reset(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Path(id): Path<String>,
    Json(input): Json<ConsumeProviderRateLimitReset>,
) -> Result<Json<Value>, AppError> {
    require_provider_manager(&state, &user, &id).await?;
    let result = consume_provider_rate_limit_reset_value(&state, &id, input).await;
    let action = if result.is_ok() {
        "provider.rate_limit_reset.consume"
    } else {
        "provider.rate_limit_reset.consume.failed"
    };
    write_admin_audit(&state, &user.id, action, Some(&id), &peer.ip().to_string()).await?;
    result.map(Json)
}

async fn consume_provider_rate_limit_reset_value(
    state: &AppState,
    id: &str,
    input: ConsumeProviderRateLimitReset,
) -> Result<Value, AppError> {
    let redeem_request_id = input.redeem_request_id.trim();
    if Uuid::parse_str(redeem_request_id).is_err() {
        return Err(AppError::bad_request("redeem_request_id must be a UUID"));
    }
    let credit_id = input.credit_id.map(|credit_id| credit_id.trim().to_owned());
    if credit_id.as_deref().is_some_and(str::is_empty) {
        return Err(AppError::bad_request("credit_id must not be empty"));
    }
    let row: (String, String, String) = sqlx::query_as(
        "SELECT access_token,account_id,originator FROM providers WHERE id=? AND is_deleted=0",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("provider not found"))?;
    let mut consume_url = url::Url::parse(&state.config.load().upstream_base)?;
    consume_url.set_path("/backend-api/wham/rate-limit-reset-credits/consume");
    consume_url.set_query(None);
    let mut body = serde_json::Map::new();
    body.insert("redeem_request_id".to_owned(), json!(redeem_request_id));
    if let Some(credit_id) = credit_id {
        body.insert("credit_id".to_owned(), json!(credit_id));
    }
    let user_agent = state
        .config
        .load()
        .upstream_user_agent_for(&row.2)
        .map(str::to_owned);
    let mut request = state
        .client
        .post(consume_url)
        .bearer_auth(&row.0)
        .header("chatgpt-account-id", &row.1)
        .header(identity::ORIGINATOR_HEADER, &row.2)
        .json(&body);
    if let Some(user_agent) = user_agent {
        request = request.header(header::USER_AGENT, user_agent);
    }
    let response = request
        .send()
        .await
        .map_err(|_| AppError::upstream(502, "provider rate-limit reset API request failed"))?;
    if !response.status().is_success() {
        return Err(AppError::upstream(
            502,
            format!(
                "provider rate-limit reset API returned {}",
                response.status()
            ),
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|_| AppError::upstream(502, "provider rate-limit reset API returned invalid JSON"))
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
            sqlx::query("UPDATE providers SET is_deleted=1 WHERE id=? AND is_deleted=0")
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

#[derive(Deserialize, Default)]
pub struct OAuthStartInput {
    #[serde(default)]
    originator: Option<String>,
}

pub async fn oauth_start(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Json(input): Json<OAuthStartInput>,
) -> Result<Json<Value>, AppError> {
    let result: Result<Json<Value>, AppError> = async {
        let originator = identity::provider_originator(input.originator.as_deref())?;
        Ok(Json(serde_json::to_value(
            oauth::start(&state, &user.id, &originator).await?,
        )?))
    }
    .await;
    let action = if result.is_ok() {
        "oauth.start"
    } else {
        "oauth.start.failed"
    };
    write_admin_audit(&state, &user.id, action, None, &peer.ip().to_string()).await?;
    result
}

#[derive(Deserialize)]
pub struct OAuthComplete {
    callback_url: String,
    #[serde(default)]
    allow_other_originator: bool,
    #[serde(default)]
    name: String,
    #[serde(default)]
    visibility: Option<String>,
}

pub async fn oauth_complete(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Json(input): Json<OAuthComplete>,
) -> Result<Json<Value>, AppError> {
    let result: Result<Json<Value>, AppError> = async {
        let (state_value, code) = oauth::callback_parameters(&state, &input.callback_url)?;
        let exchanged = oauth::exchange(&state, &state_value, &code, &user.id).await?;
        insert_provider(
            &state,
            &user.id,
            &input.name,
            ProviderCredentials {
                access: &exchanged.token.access_token,
                refresh: &exchanged.token.refresh_token,
                expires_at: Some(chrono::Utc::now().timestamp() + exchanged.token.expires_in),
            },
            None,
            ProviderRouting {
                originator: &exchanged.originator,
                allow_other_originator: input.allow_other_originator,
                visibility: &balancer::provider_visibility(input.visibility.as_deref())?,
            },
        )
        .await
    }
    .await;
    let action = if result.is_ok() {
        "oauth.complete"
    } else {
        "oauth.complete.failed"
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
    let sql = "SELECT c.user_id,k.id,k.name,k.prefix,COALESCE(NULLIF(c.model,''),'unknown'),date(c.created_at,'unixepoch'),COUNT(c.id),COALESCE(SUM(c.input_tokens),0),COALESCE(SUM(c.cached_tokens),0),COALESCE(SUM(c.output_tokens),0),COALESCE(SUM(c.request_transport_bytes+c.response_transport_bytes),0),COALESCE(SUM(c.official_cost_usd_nanos),0),COALESCE(SUM(c.actual_cost_usd_nanos),0) FROM api_calls c JOIN consumers k ON k.id=c.consumer_id WHERE c.created_at>=?";
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
        json!({"user_id":row.get::<String,_>(0),"consumer_id":row.get::<String,_>(1),"consumer_name":row.get::<String,_>(2),"consumer_prefix":row.get::<String,_>(3),"model":row.get::<String,_>(4),"date":row.get::<String,_>(5),"requests":row.get::<i64,_>(6),"input_tokens":row.get::<i64,_>(7),"cached_tokens":row.get::<i64,_>(8),"output_tokens":row.get::<i64,_>(9),"network_transport_bytes":row.get::<i64,_>(10),"official_cost_usd_nanos":row.get::<i64,_>(11),"actual_cost_usd_nanos":row.get::<i64,_>(12)})
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
        "SELECT COUNT(*) FROM api_calls c JOIN consumers k ON k.id=c.consumer_id LEFT JOIN providers ch ON ch.id=c.provider_id",
    );
    append_audit_filters(&mut total_query, &filters, &user);
    let total: i64 = total_query
        .build_query_scalar()
        .fetch_one(&state.db)
        .await?;

    let mut rows_query = QueryBuilder::<Sqlite>::new(
        "SELECT c.id,c.request_id,c.session_id,c.user_id,k.name,c.provider_id,ch.name,c.path,c.method,c.model,c.reasoning_effort,c.status,c.latency_ms,c.input_tokens,c.output_tokens,c.cached_tokens,c.error,c.client_ip,c.created_at,c.first_byte_latency_ms,c.request_bytes,c.response_bytes,c.request_transport_bytes,c.response_transport_bytes,c.official_cost_usd_nanos,c.actual_cost_usd_nanos,c.price_multiplier_nanos,c.official_consumed_usd_before_nanos,c.official_consumed_usd_after_nanos,c.actual_consumed_usd_before_nanos,c.actual_consumed_usd_after_nanos,c.official_provided_usd_before_nanos,c.official_provided_usd_after_nanos,c.actual_provided_usd_before_nanos,c.actual_provided_usd_after_nanos,c.fast_mode,c.error_code,c.downstream_user_agent,c.upstream_model,c.downstream_originator,c.upstream_originator,c.originator_fallback_reason,c.codex_turn_state_length FROM api_calls c JOIN consumers k ON k.id=c.consumer_id LEFT JOIN providers ch ON ch.id=c.provider_id",
    );
    append_audit_filters(&mut rows_query, &filters, &user);
    rows_query
        .push(" ORDER BY c.created_at DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let rows = rows_query.build().fetch_all(&state.db).await?;
    Ok(Json(json!({"rows": rows.into_iter().map(|row| {
        let mut item = json!({"id":row.get::<String,_>(0),"request_id":row.get::<String,_>(1),"session_id":row.get::<Option<String>,_>(2),"user_id":row.get::<String,_>(3),"consumer_name":row.get::<String,_>(4),"provider_id":row.get::<Option<String>,_>(5),"provider_name":row.get::<Option<String>,_>(6),"path":row.get::<String,_>(7),"method":row.get::<String,_>(8),"model":row.get::<Option<String>,_>(9),"reasoning_effort":row.get::<Option<String>,_>(10),"status":row.get::<i64,_>(11),"latency_ms":row.get::<i64,_>(12),"input_tokens":row.get::<i64,_>(13),"output_tokens":row.get::<i64,_>(14),"cached_tokens":row.get::<i64,_>(15),"error":row.get::<Option<String>,_>(16),"error_code":row.get::<Option<String>,_>("error_code"),"client_ip":row.get::<Option<String>,_>(17),"created_at":row.get::<i64,_>(18),"first_byte_latency_ms":row.get::<Option<i64>,_>(19),"request_bytes":row.get::<i64,_>(20),"response_bytes":row.get::<i64,_>(21),"request_transport_bytes":row.get::<i64,_>(22),"response_transport_bytes":row.get::<i64,_>(23),"official_cost_usd_nanos":row.get::<i64,_>(24),"actual_cost_usd_nanos":row.get::<i64,_>(25),"price_multiplier_nanos":row.get::<i64,_>(26),"official_consumed_usd_before_nanos":row.get::<i64,_>(27),"official_consumed_usd_after_nanos":row.get::<i64,_>(28),"actual_consumed_usd_before_nanos":row.get::<i64,_>(29),"actual_consumed_usd_after_nanos":row.get::<i64,_>(30),"official_provided_usd_before_nanos":row.get::<i64,_>(31),"official_provided_usd_after_nanos":row.get::<i64,_>(32),"actual_provided_usd_before_nanos":row.get::<i64,_>(33),"actual_provided_usd_after_nanos":row.get::<i64,_>(34),"fast_mode":row.get::<i64,_>(35) != 0});
        item["downstream_user_agent"] = json!(row.get::<Option<String>, _>("downstream_user_agent"));
        item["upstream_model"] = json!(row.get::<Option<String>, _>("upstream_model"));
        item["downstream_originator"] = json!(row.get::<Option<String>, _>("downstream_originator"));
        item["upstream_originator"] = json!(row.get::<Option<String>, _>("upstream_originator"));
        item["originator_fallback_reason"] = json!(row.get::<Option<String>, _>("originator_fallback_reason"));
        item["codex_turn_state_length"] = json!(row.get::<Option<i64>, _>("codex_turn_state_length"));
        item
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
    append_audit_text_filter(query, "ch.name", filters.provider.as_deref());
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
            | "x-openai-api-key"
            | "x-goog-api-key"
            | "x-auth"
            | "x-credential"
            | "access-key"
            | "x-amz-security-token"
            | "x-session-id"
            | "x-codex-session-id"
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
    let (sql, scope) = if admin {
        (
            "SELECT c.id,c.request_id,c.thread_id,c.user_id,k.name,c.provider_id,ch.name,c.method,c.path,c.model,c.reasoning_effort,c.status,c.latency_ms,c.input_tokens,c.output_tokens,c.cached_tokens,c.error,c.client_ip,c.affinity_hash,c.affinity_source,c.created_at,a.api_call_id,a.request_headers_json,a.request_body,a.request_body_truncated,a.response_headers_json,a.response_body,a.response_body_truncated,c.consumer_id,c.first_byte_latency_ms,c.request_bytes,c.response_bytes,c.request_transport_bytes,c.response_transport_bytes,c.downstream_accept_encoding,c.downstream_content_encoding,c.upstream_accept_encoding,c.upstream_content_encoding,a.upstream_request_headers_json,a.downstream_response_headers_json,c.upstream_http_version,c.official_cost_usd_nanos,c.actual_cost_usd_nanos,c.price_multiplier_nanos,c.official_consumed_usd_before_nanos,c.official_consumed_usd_after_nanos,c.actual_consumed_usd_before_nanos,c.actual_consumed_usd_after_nanos,c.official_provided_usd_before_nanos,c.official_provided_usd_after_nanos,c.actual_provided_usd_before_nanos,c.actual_provided_usd_after_nanos,COALESCE(a.bodies_deleted,0),c.fast_mode,c.error_code,c.downstream_user_agent,c.upstream_model,c.session_id,c.downstream_originator,c.upstream_originator,c.originator_fallback_reason,c.codex_turn_state_length FROM api_calls c JOIN consumers k ON k.id=c.consumer_id LEFT JOIN providers ch ON ch.id=c.provider_id LEFT JOIN request_archives a ON a.api_call_id=c.id WHERE c.id=?",
            None,
        )
    } else {
        (
            "SELECT c.id,c.request_id,c.thread_id,c.user_id,k.name,c.provider_id,ch.name,c.method,c.path,c.model,c.reasoning_effort,c.status,c.latency_ms,c.input_tokens,c.output_tokens,c.cached_tokens,c.error,c.client_ip,c.affinity_hash,c.affinity_source,c.created_at,a.api_call_id,a.request_headers_json,a.request_body,a.request_body_truncated,a.response_headers_json,a.response_body,a.response_body_truncated,c.consumer_id,c.first_byte_latency_ms,c.request_bytes,c.response_bytes,c.request_transport_bytes,c.response_transport_bytes,c.downstream_accept_encoding,c.downstream_content_encoding,c.upstream_accept_encoding,c.upstream_content_encoding,a.upstream_request_headers_json,a.downstream_response_headers_json,c.upstream_http_version,c.official_cost_usd_nanos,c.actual_cost_usd_nanos,c.price_multiplier_nanos,c.official_consumed_usd_before_nanos,c.official_consumed_usd_after_nanos,c.actual_consumed_usd_before_nanos,c.actual_consumed_usd_after_nanos,c.official_provided_usd_before_nanos,c.official_provided_usd_after_nanos,c.actual_provided_usd_before_nanos,c.actual_provided_usd_after_nanos,COALESCE(a.bodies_deleted,0),c.fast_mode,c.error_code,c.downstream_user_agent,c.upstream_model,c.session_id,c.downstream_originator,c.upstream_originator,c.originator_fallback_reason,c.codex_turn_state_length FROM api_calls c JOIN consumers k ON k.id=c.consumer_id LEFT JOIN providers ch ON ch.id=c.provider_id LEFT JOIN request_archives a ON a.api_call_id=c.id WHERE c.id=? AND c.user_id=?",
            Some(user.id.clone()),
        )
    };
    let mut query = sqlx::query(sql).bind(&id);
    if let Some(scope) = scope {
        query = query.bind(scope);
    }
    let row = query
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("audit event not found"))?;
    let thread_id = row.get::<Option<String>, _>(2);
    let session_id = row.get::<Option<String>, _>("session_id");
    let consumer_id = row.get::<String, _>(28);
    let navigation_rows = if let Some(session_id) = &session_id {
        let (sql, scope) = if admin {
            (
                "SELECT id,request_id,created_at FROM api_calls WHERE session_id=? AND consumer_id=? ORDER BY created_at,id",
                None,
            )
        } else {
            (
                "SELECT id,request_id,created_at FROM api_calls WHERE session_id=? AND consumer_id=? AND user_id=? ORDER BY created_at,id",
                Some(user.id.clone()),
            )
        };
        let mut query = sqlx::query(sql).bind(session_id).bind(consumer_id);
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
    let request_headers = visible_archive_headers(row.get::<Option<String>, _>(22), admin);
    let upstream_request_headers = visible_archive_headers(row.get::<Option<String>, _>(38), admin);
    let response_headers = visible_archive_headers(row.get::<Option<String>, _>(25), admin);
    let downstream_response_headers =
        visible_archive_headers(row.get::<Option<String>, _>(39), admin);
    let upstream_http_version = row.get::<Option<String>, _>(40);
    let official_cost_usd_nanos = row.get::<i64, _>(41);
    let actual_cost_usd_nanos = row.get::<i64, _>(42);
    let price_multiplier_nanos = row.get::<i64, _>(43);
    let official_consumed_usd_before_nanos = row.get::<i64, _>(44);
    let official_consumed_usd_after_nanos = row.get::<i64, _>(45);
    let actual_consumed_usd_before_nanos = row.get::<i64, _>(46);
    let actual_consumed_usd_after_nanos = row.get::<i64, _>(47);
    let official_provided_usd_before_nanos = row.get::<i64, _>(48);
    let official_provided_usd_after_nanos = row.get::<i64, _>(49);
    let actual_provided_usd_before_nanos = row.get::<i64, _>(50);
    let actual_provided_usd_after_nanos = row.get::<i64, _>(51);
    let archive_available = row.get::<Option<String>, _>(21).is_some();
    let bodies_available = archive_available && row.get::<i64, _>(52) == 0;
    let fast_mode = row.get::<i64, _>(53) != 0;
    let mut detail = json!({
        "id":row.get::<String,_>(0),"request_id":row.get::<String,_>(1),"thread_id":thread_id,"user_id":row.get::<String,_>(3),"consumer_name":row.get::<String,_>(4),
        "provider_id":row.get::<Option<String>,_>(5),"provider_name":row.get::<Option<String>,_>(6),"method":row.get::<String,_>(7),"path":row.get::<String,_>(8),
        "model":row.get::<Option<String>,_>(9),"reasoning_effort":row.get::<Option<String>,_>(10),"fast_mode":fast_mode,"status":row.get::<i64,_>(11),"latency_ms":row.get::<i64,_>(12),"input_tokens":row.get::<i64,_>(13),
        "output_tokens":row.get::<i64,_>(14),"cached_tokens":row.get::<i64,_>(15),"error":row.get::<Option<String>,_>(16),"client_ip":row.get::<Option<String>,_>(17),"first_byte_latency_ms":row.get::<Option<i64>,_>(29),"request_bytes":row.get::<i64,_>(30),"response_bytes":row.get::<i64,_>(31),"request_transport_bytes":row.get::<i64,_>(32),"response_transport_bytes":row.get::<i64,_>(33),"downstream_accept_encoding":row.get::<Option<String>,_>(34),"downstream_content_encoding":row.get::<Option<String>,_>(35),"upstream_accept_encoding":row.get::<Option<String>,_>(36),"upstream_content_encoding":row.get::<Option<String>,_>(37),
        "affinity_hash":row.get::<Option<String>,_>(18),"affinity_source":row.get::<Option<String>,_>(19),"created_at":row.get::<i64,_>(20),
        "request_headers":request_headers,"upstream_request_headers":upstream_request_headers,
        "request_body":row.get::<Option<Vec<u8>>,_>(23).map(|body| String::from_utf8_lossy(&body).into_owned()),"request_body_truncated":row.get::<Option<i64>,_>(24).unwrap_or_default() != 0,
        "response_headers":response_headers,"downstream_response_headers":downstream_response_headers,"response_body":row.get::<Option<Vec<u8>>,_>(26).map(|body| String::from_utf8_lossy(&body).into_owned()),
        "response_body_truncated":row.get::<Option<i64>,_>(27).unwrap_or_default() != 0,
        "previous":navigation(previous),"next":navigation(next)
    });
    detail["session_id"] = json!(session_id);
    detail["downstream_user_agent"] = json!(row.get::<Option<String>, _>("downstream_user_agent"));
    detail["upstream_model"] = json!(row.get::<Option<String>, _>("upstream_model"));
    detail["downstream_originator"] = json!(row.get::<Option<String>, _>("downstream_originator"));
    detail["upstream_originator"] = json!(row.get::<Option<String>, _>("upstream_originator"));
    detail["originator_fallback_reason"] =
        json!(row.get::<Option<String>, _>("originator_fallback_reason"));
    detail["codex_turn_state_length"] = row
        .get::<Option<i64>, _>("codex_turn_state_length")
        .map(Value::from)
        .unwrap_or(Value::Null);
    detail["error_code"] = row
        .get::<Option<String>, _>("error_code")
        .map(Value::String)
        .unwrap_or(Value::Null);
    detail["upstream_http_version"] = upstream_http_version
        .map(Value::String)
        .unwrap_or(Value::Null);
    detail["official_cost_usd_nanos"] = Value::from(official_cost_usd_nanos);
    detail["actual_cost_usd_nanos"] = Value::from(actual_cost_usd_nanos);
    detail["price_multiplier_nanos"] = Value::from(price_multiplier_nanos);
    detail["official_consumed_usd_before_nanos"] = Value::from(official_consumed_usd_before_nanos);
    detail["official_consumed_usd_after_nanos"] = Value::from(official_consumed_usd_after_nanos);
    detail["actual_consumed_usd_before_nanos"] = Value::from(actual_consumed_usd_before_nanos);
    detail["actual_consumed_usd_after_nanos"] = Value::from(actual_consumed_usd_after_nanos);
    detail["official_provided_usd_before_nanos"] = Value::from(official_provided_usd_before_nanos);
    detail["official_provided_usd_after_nanos"] = Value::from(official_provided_usd_after_nanos);
    detail["actual_provided_usd_before_nanos"] = Value::from(actual_provided_usd_before_nanos);
    detail["actual_provided_usd_after_nanos"] = Value::from(actual_provided_usd_after_nanos);
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
    let calls: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM api_calls WHERE user_id=? AND created_at>?")
            .bind(&user.id)
            .bind(chrono::Utc::now().timestamp() - 86400)
            .fetch_one(&state.db)
            .await?;
    let errors_query = "SELECT COUNT(*) FROM api_calls c WHERE c.user_id=? AND ".to_owned()
        + AUDIT_ERROR_CONDITION
        + " AND c.created_at>?";
    let errors: i64 = sqlx::query_scalar(&errors_query)
        .bind(&user.id)
        .bind(chrono::Utc::now().timestamp() - 86400)
        .fetch_one(&state.db)
        .await?;
    let (input_tokens_24h, output_tokens_24h, cached_tokens_24h): (i64, i64, i64) =
        sqlx::query_as(
            "SELECT COALESCE(SUM(input_tokens),0),COALESCE(SUM(output_tokens),0),COALESCE(SUM(cached_tokens),0) FROM api_calls WHERE user_id=? AND created_at>?",
        )
        .bind(&user.id)
        .bind(chrono::Utc::now().timestamp() - 86400)
        .fetch_one(&state.db)
        .await?;
    let (official_cost_usd_nanos_24h, actual_cost_usd_nanos_24h): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(official_cost_usd_nanos),0),COALESCE(SUM(actual_cost_usd_nanos),0) FROM api_calls WHERE user_id=? AND created_at>?",
    )
    .bind(&user.id)
    .bind(chrono::Utc::now().timestamp() - 86400)
    .fetch_one(&state.db)
    .await?;
    let (official_consumed_usd_nanos, consumed_usd_nanos): (i64, i64) = sqlx::query_as(
        "SELECT official_consumed_usd_nanos,consumed_usd_nanos FROM users WHERE id=?",
    )
    .bind(&user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(
        json!({"active_consumers":keys,"calls_24h":calls,"errors_24h":errors,"input_tokens_24h":input_tokens_24h,"output_tokens_24h":output_tokens_24h,"cached_tokens_24h":cached_tokens_24h,"official_cost_usd_nanos_24h":official_cost_usd_nanos_24h,"actual_cost_usd_nanos_24h":actual_cost_usd_nanos_24h,"official_consumed_usd_nanos":official_consumed_usd_nanos,"consumed_usd_nanos":consumed_usd_nanos}),
    ))
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
    let mut output = json!({
        "role":user.role,
        "auth_issuer":config.auth_issuer,
        "upstream_base":config.upstream_base,
        "upstream_openai_beta":config.upstream_openai_beta,
        "upstream_user_agent":config.upstream_user_agent,
        "upstream_user_agents":config.upstream_user_agents,
        "image_host_model":config.image_host_model,
        "available_model_ids":config.available_model_ids,
        "allow_all_users_debt":config.allow_all_users_debt,
        "oauth_authorize_url":config.oauth_authorize_url,
        "oauth_token_url":config.oauth_token_url,
        "oauth_redirect_uri":config.oauth_redirect_uri,
        "oauth_client_id":config.oauth_client_id,
        "response_body_limit":config.response_body_limit,
        "image_body_limit":config.image_body_limit,
        "audio_body_limit":config.audio_body_limit,
        "affinity_ttl_seconds":config.affinity_ttl_seconds,
        "provider_concurrency_limit":config.provider_concurrency_limit,
        "request_archive_retention_days":config.request_archive_retention_days,
        "model_price_multiplier":payments::format_usd_nanos(config.model_price_multiplier_nanos)
    });
    if is_admin(&user) {
        output["experimental_filter_codex_turn_state_312"] =
            json!(config.experimental_filter_codex_turn_state_312);
    }
    Ok(Json(output))
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
    let sql = "SELECT (c.created_at / 3600) * 3600 AS hour_start,c.provider_id,ch.name,COALESCE(NULLIF(c.model,''),'unknown'),SUM(CASE WHEN NULLIF(c.error_code,'') IS NULL THEN 1 ELSE 0 END),SUM(CASE WHEN NULLIF(c.error_code,'') IS NOT NULL THEN 1 ELSE 0 END),COALESCE(SUM(c.input_tokens),0) FROM api_calls c LEFT JOIN providers ch ON ch.id=c.provider_id WHERE c.created_at>=? AND c.created_at<=? GROUP BY hour_start,c.provider_id,ch.name,COALESCE(NULLIF(c.model,''),'unknown') ORDER BY hour_start DESC,ch.name,COALESCE(NULLIF(c.model,''),'unknown')";
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
            let input_tokens = row.get::<i64, _>(6);
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
                "input_tokens": input_tokens,
            })
        }).collect::<Vec<_>>()
    })))
}

pub async fn model_downgrade_audit(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<Value>, AppError> {
    require_admin(&user)?;
    let period = query.period.unwrap_or(UsagePeriod::Last7Days);
    let until = chrono::Utc::now().timestamp();
    let since = until - period.seconds();
    let sql = "SELECT (c.created_at / 3600) * 3600 AS hour_start,c.provider_id,ch.name,COALESCE(NULLIF(c.model,''),'unknown'),c.upstream_model,COUNT(*) FROM api_calls c LEFT JOIN providers ch ON ch.id=c.provider_id WHERE c.upstream_model IS NOT NULL AND c.created_at>=? AND c.created_at<=? GROUP BY hour_start,c.provider_id,ch.name,COALESCE(NULLIF(c.model,''),'unknown'),c.upstream_model ORDER BY hour_start DESC";
    let rows = sqlx::query(sql)
        .bind(since)
        .bind(until)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({
        "since": since,
        "until": until,
        "period": period.label(),
        "rows": rows.into_iter().map(|row| json!({
            "hour_start": row.get::<i64, _>(0),
            "provider_id": row.get::<Option<String>, _>(1),
            "provider_name": row.get::<Option<String>, _>(2),
            "downstream_model": row.get::<String, _>(3),
            "upstream_model": row.get::<String, _>(4),
            "requests": row.get::<i64, _>(5),
        })).collect::<Vec<_>>()
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
pub struct UpdateUpstreamUserAgent {
    #[serde(default)]
    originator: Option<String>,
    #[serde(default)]
    user_agent: Option<String>,
    /// Legacy payload field. It updates the CodeX override.
    #[serde(default)]
    upstream_user_agent: Option<String>,
}

pub async fn update_upstream_user_agent(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Json(input): Json<UpdateUpstreamUserAgent>,
) -> Result<Json<Value>, AppError> {
    require_admin(&user)?;
    let (originator, raw_user_agent) = if input.upstream_user_agent.is_some() {
        (
            identity::CODEX_ORIGINATOR.to_owned(),
            input.upstream_user_agent,
        )
    } else {
        (
            identity::provider_originator(input.originator.as_deref())?,
            input.user_agent,
        )
    };
    let user_agent = optional_header_value("upstream User-Agent", raw_user_agent.as_deref())?;
    {
        let _write = state.write_gate.lock().await;
        let key = format!("upstream_user_agent_{originator}");
        sqlx::query("UPDATE app_meta SET value=?,updated_at=? WHERE key=?")
            .bind(user_agent.as_deref().unwrap_or_default())
            .bind(chrono::Utc::now().timestamp())
            .bind(&key)
            .execute(&state.db)
            .await?;
        if originator == identity::CODEX_ORIGINATOR {
            sqlx::query("UPDATE app_meta SET value=?,updated_at=? WHERE key='upstream_user_agent'")
                .bind(user_agent.as_deref().unwrap_or_default())
                .bind(chrono::Utc::now().timestamp())
                .execute(&state.db)
                .await?;
        }
        let mut config = (**state.config.load()).clone();
        config
            .upstream_user_agents
            .set(&originator, user_agent.clone());
        if originator == identity::CODEX_ORIGINATOR {
            config.upstream_user_agent = user_agent.clone();
        }
        state.config.store(std::sync::Arc::new(config));
    }
    write_admin_audit(
        &state,
        &user.id,
        "settings.upstream_user_agent.update",
        None,
        &peer.ip().to_string(),
    )
    .await?;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
pub struct UpdateExperimentalTurnState312Filter {
    enabled: bool,
}

pub async fn update_experimental_turn_state_312_filter(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(user): Extension<UserIdentity>,
    Json(input): Json<UpdateExperimentalTurnState312Filter>,
) -> Result<Json<Value>, AppError> {
    require_admin(&user)?;
    let value = input.enabled.to_string();
    {
        let _write = state.write_gate.lock().await;
        sqlx::query(
            "UPDATE app_meta SET value=?,updated_at=? WHERE key='experimental_filter_codex_turn_state_312'",
        )
        .bind(&value)
        .bind(chrono::Utc::now().timestamp())
        .execute(&state.db)
        .await?;
        let mut config = (**state.config.load()).clone();
        config.experimental_filter_codex_turn_state_312 = input.enabled;
        state.config.store(std::sync::Arc::new(config));
    }
    write_admin_audit(
        &state,
        &user.id,
        "settings.experimental_turn_state_312_filter.update",
        None,
        &peer.ip().to_string(),
    )
    .await?;
    Ok(Json(json!({"ok":true,"enabled":input.enabled})))
}

#[derive(Deserialize)]
pub struct UpdateSettings {
    upstream_base: String,
    upstream_openai_beta: Option<String>,
    image_host_model: String,
    allow_all_users_debt: bool,
    oauth_authorize_url: String,
    oauth_token_url: String,
    oauth_redirect_uri: String,
    oauth_client_id: String,
    response_body_limit: usize,
    image_body_limit: usize,
    audio_body_limit: usize,
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
    let upstream_openai_beta = optional_header_value(
        "upstream OpenAI-Beta",
        input.upstream_openai_beta.as_deref(),
    )?;
    let oauth_authorize_url = validate_http_url("OAuth authorize URL", &input.oauth_authorize_url)?;
    let oauth_token_url = validate_http_url("OAuth token URL", &input.oauth_token_url)?;
    let oauth_redirect_uri = validate_http_url("OAuth redirect URI", &input.oauth_redirect_uri)?;
    let image_host_model = required_setting("image host model", &input.image_host_model)?;
    let oauth_client_id = required_setting("OAuth client ID", &input.oauth_client_id)?;
    let model_price_multiplier_nanos =
        payments::parse_model_price_multiplier_nanos(&input.model_price_multiplier)?;
    if !(1_024..=16 * 1_024 * 1_024).contains(&input.response_body_limit)
        || !(1_024..=16 * 1_024 * 1_024).contains(&input.image_body_limit)
        || !(1_024 * 1_024..=2_000_000_000).contains(&input.audio_body_limit)
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
            "upstream_openai_beta",
            upstream_openai_beta.clone().unwrap_or_default(),
        ),
        ("image_host_model", image_host_model.clone()),
        (
            "allow_all_users_debt",
            input.allow_all_users_debt.to_string(),
        ),
        ("oauth_authorize_url", oauth_authorize_url.clone()),
        ("oauth_token_url", oauth_token_url.clone()),
        ("oauth_redirect_uri", oauth_redirect_uri.clone()),
        ("oauth_client_id", oauth_client_id.clone()),
        ("response_body_limit", input.response_body_limit.to_string()),
        ("image_body_limit", input.image_body_limit.to_string()),
        ("audio_body_limit", input.audio_body_limit.to_string()),
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
    config.upstream_openai_beta = upstream_openai_beta;
    config.image_host_model = image_host_model;
    config.allow_all_users_debt = input.allow_all_users_debt;
    config.oauth_authorize_url = oauth_authorize_url;
    config.oauth_token_url = oauth_token_url;
    config.oauth_redirect_uri = oauth_redirect_uri;
    config.oauth_client_id = oauth_client_id;
    config.response_body_limit = input.response_body_limit;
    config.image_body_limit = input.image_body_limit;
    config.audio_body_limit = input.audio_body_limit;
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

fn optional_header_value(name: &str, value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 1_024 || HeaderValue::from_bytes(value.as_bytes()).is_err() {
        return Err(AppError::bad_request(format!(
            "{name} must be a valid HTTP header value up to 1024 characters"
        )));
    }
    Ok(Some(value.to_owned()))
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(root): Extension<UserIdentity>,
) -> Result<Json<Value>, AppError> {
    require_admin(&root)?;
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
                let topup_usd_nanos = topups
                    .get(&id)
                    .copied()
                    .unwrap_or(0);
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
    use axum::{
        Router,
        body::Body,
        http::{Method, Request, StatusCode},
        response::{IntoResponse, Response},
        routing::{get, post},
    };
    use ed25519_dalek::{Signer, SigningKey};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::{owner_downstream, test_downstream};

    const TEST_AUTH_AUDIENCE: &str = "openai-lb.test";

    fn setup_token(signing: &SigningKey, issuer: &str, user_id: &str) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA","kid":"setup","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({
                "sub":user_id,
                "sid":"session-1",
                "iss":issuer,
                "aud":TEST_AUTH_AUDIENCE,
                "amr":["webauthn"],
                "typ":"access",
                "iat":chrono::Utc::now().timestamp(),
                "exp":chrono::Utc::now().timestamp() + 300,
            }))
            .unwrap(),
        );
        let input = format!("{header}.{payload}");
        format!(
            "{input}.{}",
            URL_SAFE_NO_PAD.encode(signing.sign(input.as_bytes()).to_bytes())
        )
    }

    fn provider_oauth_access_token(account_id: &str) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({
                "https://api.openai.com/auth": {"chatgpt_account_id":account_id},
                "exp":chrono::Utc::now().timestamp() + 3600
            }))
            .unwrap(),
        );
        format!("{header}.{payload}.signature")
    }

    async fn provider_request(
        state: &AppState,
        method: Method,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> Response {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header("session-id", "test-session")
            .extension(ConnectInfo("127.0.0.1:9000".parse::<SocketAddr>().unwrap()));
        let body = match body {
            Some(value) => {
                builder = builder.header(header::CONTENT_TYPE, "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        crate::router(state.clone())
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap()
    }

    fn root_identity() -> UserIdentity {
        UserIdentity {
            id: "owner".to_owned(),
            email: None,
            name: None,
            role: "root".to_owned(),
        }
    }

    #[tokio::test]
    async fn provider_creation_persists_the_selected_originator() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','root',0)")
            .execute(&state.db)
            .await
            .unwrap();
        let access = provider_oauth_access_token("account-pi");
        let created = create_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(root_identity()),
            Json(CreateProvider {
                allow_other_originator: false,
                name: "pi provider".to_owned(),
                access_key: access.clone(),
                refresh_key: "refresh".to_owned(),
                originator: Some("pi".to_owned()),
                visibility: None,
                http_proxy_url: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(created.0["originator"], "pi");
        let id = created.0["id"].as_str().unwrap();
        let stored: String = sqlx::query_scalar("SELECT originator FROM providers WHERE id=?")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(stored, "pi");

        // Providers created without an explicit identity keep the CodeX CLI default.
        let default_created = create_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(root_identity()),
            Json(CreateProvider {
                name: String::new(),
                access_key: provider_oauth_access_token("account-default"),
                refresh_key: "refresh".to_owned(),
                originator: None,
                allow_other_originator: false,
                visibility: None,
                http_proxy_url: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(default_created.0["originator"], identity::CODEX_ORIGINATOR);
    }

    #[tokio::test]
    async fn provider_visibility_defaults_to_private_and_can_be_published() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','root',0)")
            .execute(&state.db)
            .await
            .unwrap();
        let created = create_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(root_identity()),
            Json(CreateProvider {
                name: "private provider".to_owned(),
                access_key: provider_oauth_access_token("account-visibility"),
                refresh_key: "refresh".to_owned(),
                originator: None,
                allow_other_originator: false,
                visibility: None,
                http_proxy_url: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(created.0["visibility"], "private");
        let id = created.0["id"].as_str().unwrap().to_owned();

        let rejected = create_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(root_identity()),
            Json(CreateProvider {
                name: String::new(),
                access_key: provider_oauth_access_token("account-invalid-visibility"),
                refresh_key: "refresh".to_owned(),
                originator: None,
                allow_other_originator: false,
                visibility: Some("shared".to_owned()),
                http_proxy_url: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

        let publish = |visibility: Option<&str>| {
            update_provider(
                State(state.clone()),
                ConnectInfo("127.0.0.1:8080".parse().unwrap()),
                Extension(root_identity()),
                Path(id.clone()),
                Json(ProviderUpdate {
                    name: None,
                    enabled: None,
                    refresh: None,
                    http_proxy_url: None,
                    originator: None,
                    allow_other_originator: None,
                    visibility: visibility.map(str::to_owned),
                }),
            )
        };
        let _ = publish(Some("public")).await.unwrap();
        let stored: String = sqlx::query_scalar("SELECT visibility FROM providers WHERE id=?")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(stored, "public");
        let renamed: String = sqlx::query_scalar("SELECT name FROM providers WHERE id=?")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(renamed, "private provider");

        assert_eq!(
            publish(Some("shared")).await.unwrap_err().status(),
            StatusCode::BAD_REQUEST
        );
        let _ = publish(None).await.unwrap();
        let stored: String = sqlx::query_scalar("SELECT visibility FROM providers WHERE id=?")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(stored, "public");
    }

    #[tokio::test]
    async fn provider_creation_rejects_unsupported_originators() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','root',0)")
            .execute(&state.db)
            .await
            .unwrap();
        let error = create_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(root_identity()),
            Json(CreateProvider {
                allow_other_originator: false,
                name: String::new(),
                access_key: provider_oauth_access_token("account-x"),
                refresh_key: "refresh".to_owned(),
                originator: Some("unsloth_studio".to_owned()),
                visibility: None,
                http_proxy_url: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        let providers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM providers")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(providers, 0);
    }

    #[tokio::test]
    async fn provider_originator_cannot_be_changed_after_creation() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','root',0)")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,owner_id,originator,created_at,updated_at) VALUES('provider','provider','account','access','refresh','owner','pi',0,0)")
            .execute(&state.db)
            .await
            .unwrap();
        let error = update_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(root_identity()),
            Path("provider".to_owned()),
            Json(ProviderUpdate {
                allow_other_originator: None,
                name: None,
                enabled: None,
                refresh: None,
                http_proxy_url: None,
                originator: Some(identity::CODEX_ORIGINATOR.to_owned()),
                visibility: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        let stored: String =
            sqlx::query_scalar("SELECT originator FROM providers WHERE id='provider'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(stored, "pi");

        let _ = update_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(root_identity()),
            Path("provider".to_owned()),
            Json(ProviderUpdate {
                allow_other_originator: None,
                name: Some("renamed".to_owned()),
                enabled: None,
                refresh: None,
                http_proxy_url: None,
                originator: None,
                visibility: None,
            }),
        )
        .await
        .unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT originator FROM providers WHERE id='provider'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(stored, "pi");
    }

    #[tokio::test]
    async fn deleting_provider_hides_it_without_changing_its_history_or_owner_value() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query(
            "INSERT INTO users(id,role,provided_usd_nanos,created_at) VALUES('owner','root',19,0)",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,owner_id,official_provided_usd_nanos,actual_provided_usd_nanos,created_at,updated_at) VALUES('provider','provider','account','access','refresh','owner',150,12,0,0)")
            .execute(&state.db)
            .await
            .unwrap();

        let response = delete_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(UserIdentity {
                id: "owner".to_owned(),
                email: None,
                name: None,
                role: "root".to_owned(),
            }),
            Path("provider".to_owned()),
        )
        .await
        .unwrap();
        assert_eq!(response.0["ok"], true);

        let provided_usd_nanos: i64 =
            sqlx::query_scalar("SELECT provided_usd_nanos FROM users WHERE id='owner'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(provided_usd_nanos, 19);
        let is_deleted: i64 =
            sqlx::query_scalar("SELECT is_deleted FROM providers WHERE id='provider'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(is_deleted, 1);
        let providers = list_providers(
            State(state.clone()),
            Extension(UserIdentity {
                id: "owner".to_owned(),
                email: None,
                name: None,
                role: "root".to_owned(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(providers.0, json!([]));
        assert!(
            state
                .balancer
                .select(&state, None, None, test_downstream())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn list_users_includes_financial_totals() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,consumed_usd_nanos,provided_usd_nanos,created_at) VALUES('admin','admin',0,0,0),('tenant','user',7_000_000_000,3_000_000_000,1)")
            .execute(&state.db)
            .await
            .unwrap();
        let users = list_users(
            State(state),
            Extension(UserIdentity {
                id: "admin".to_owned(),
                email: None,
                name: None,
                role: "admin".to_owned(),
            }),
        )
        .await
        .unwrap();
        let tenant = users
            .0
            .as_array()
            .unwrap()
            .iter()
            .find(|user| user["id"] == "tenant")
            .unwrap();

        assert_eq!(tenant["topup_usd_nanos"], 0);
        assert_eq!(tenant["consumed_usd_nanos"], 7_000_000_000_i64);
        assert_eq!(tenant["provided_usd_nanos"], 3_000_000_000_i64);
        assert_eq!(tenant["available_usd_nanos"], -4_000_000_000_i64);
    }

    #[tokio::test]
    async fn experimental_312_turn_state_filter_is_admin_only_and_persistent() {
        let state = crate::test_state("http://token.invalid").await;
        let user = UserIdentity {
            id: "tenant".to_owned(),
            email: None,
            name: None,
            role: "user".to_owned(),
        };
        let admin = UserIdentity {
            id: "admin".to_owned(),
            email: None,
            name: None,
            role: "admin".to_owned(),
        };
        sqlx::query(
            "INSERT INTO users(id,role,created_at) VALUES('tenant','user',0),('admin','admin',0)",
        )
        .execute(&state.db)
        .await
        .unwrap();

        let user_settings = settings(State(state.clone()), Extension(user))
            .await
            .unwrap();
        assert!(
            user_settings
                .0
                .get("experimental_filter_codex_turn_state_312")
                .is_none()
        );
        let admin_settings = settings(State(state.clone()), Extension(admin.clone()))
            .await
            .unwrap();
        assert_eq!(
            admin_settings.0["experimental_filter_codex_turn_state_312"],
            false
        );

        let forbidden = update_experimental_turn_state_312_filter(
            State(state.clone()),
            ConnectInfo("127.0.0.1:9000".parse().unwrap()),
            Extension(UserIdentity {
                id: "tenant".to_owned(),
                email: None,
                name: None,
                role: "user".to_owned(),
            }),
            Json(UpdateExperimentalTurnState312Filter { enabled: true }),
        )
        .await;
        assert!(forbidden.is_err());

        let _ = update_experimental_turn_state_312_filter(
            State(state.clone()),
            ConnectInfo("127.0.0.1:9000".parse().unwrap()),
            Extension(admin.clone()),
            Json(UpdateExperimentalTurnState312Filter { enabled: true }),
        )
        .await
        .unwrap();
        assert!(state.config.load().experimental_filter_codex_turn_state_312);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM app_meta WHERE key='experimental_filter_codex_turn_state_312'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "true"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM admin_audit WHERE action='settings.experimental_turn_state_312_filter.update'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn root_can_update_midas_settings() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('root','root',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        let root = UserIdentity {
            id: "root".to_owned(),
            email: None,
            name: None,
            role: "root".to_owned(),
        };

        let fund_user_id = "031a393d-8a0a-4ca0-8cb1-313e0d71dfa9";
        let _ = update_midas_settings(
            State(state.clone()),
            ConnectInfo("127.0.0.1:9000".parse::<SocketAddr>().unwrap()),
            Extension(root.clone()),
            Json(UpdateMidasSettings {
                midas_api_base: "https://midas.ntnl.io/api".to_owned(),
                midas_fund_user_id: fund_user_id.to_owned(),
                midas_fund_api_key: "midas_fund_test".to_owned(),
            }),
        )
        .await
        .unwrap();

        let stored: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='midas_fund_user_id'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(stored, fund_user_id);
        let settings = midas_settings(State(state), Extension(root)).await.unwrap();
        assert_eq!(settings.0["midas_fund_api_key_configured"], true);
    }

    #[tokio::test]
    async fn root_can_update_the_model_price_multiplier_and_clear_upstream_openai_beta() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('root','root',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        let root = UserIdentity {
            id: "root".to_owned(),
            email: None,
            name: None,
            role: "root".to_owned(),
        };

        let input: UpdateSettings = serde_json::from_value(json!({
            "upstream_base": "http://upstream.invalid",
            "upstream_openai_beta": null,
            "image_host_model": "gpt-5.4",
            "allow_all_users_debt": true,
            "oauth_authorize_url": "http://auth.invalid/oauth/authorize",
            "oauth_token_url": "http://token.invalid",
            "oauth_redirect_uri": "http://localhost:1455/auth/callback",
            "oauth_client_id": "test-client",
            "response_body_limit": 1024 * 1024,
            "image_body_limit": 16 * 1024 * 1024,
            "audio_body_limit": 1024 * 1024,
            "affinity_ttl_seconds": 3600,
            "request_archive_retention_days": 7,
            "model_price_multiplier": "0.25"
        }))
        .unwrap();
        assert_eq!(input.upstream_openai_beta, None);

        let _ = update_settings(
            State(state.clone()),
            ConnectInfo("127.0.0.1:9000".parse::<SocketAddr>().unwrap()),
            Extension(root.clone()),
            Json(input),
        )
        .await
        .unwrap();

        assert_eq!(
            state.config.load().model_price_multiplier_nanos,
            250_000_000
        );
        let stored: String = sqlx::query_scalar(
            "SELECT value FROM app_meta WHERE key='model_price_multiplier_nanos'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(stored, "250000000");
        let settings = settings(State(state.clone()), Extension(root))
            .await
            .unwrap();
        assert_eq!(settings.0["model_price_multiplier"], "0.25");
        assert_eq!(settings.0["allow_all_users_debt"], true);
        assert_eq!(settings.0["upstream_openai_beta"], Value::Null);
        let stored: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='allow_all_users_debt'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(stored, "true");
    }

    #[tokio::test]
    async fn upstream_user_agent_is_admin_only_validated_persistent_and_clearable() {
        let state = crate::test_state("http://token.invalid").await;
        let peer = "127.0.0.1:9000".parse::<SocketAddr>().unwrap();
        for role in ["user", "admin", "root"] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,?,0)")
                .bind(role)
                .bind(role)
                .execute(&state.db)
                .await
                .unwrap();
            let user = UserIdentity {
                id: role.into(),
                role: role.into(),
                email: None,
                name: None,
            };
            let result = update_upstream_user_agent(
                State(state.clone()),
                ConnectInfo(peer),
                Extension(user.clone()),
                Json(UpdateUpstreamUserAgent {
                    originator: None,
                    user_agent: None,
                    upstream_user_agent: Some(" Global UA/1.0 ".into()),
                }),
            )
            .await;
            if role == "user" {
                assert_eq!(result.unwrap_err().status(), StatusCode::FORBIDDEN);
                assert_eq!(state.config.load().upstream_user_agent, None);
                continue;
            }
            assert_eq!(result.unwrap().0["ok"], true);
            assert_eq!(
                state.config.load().upstream_user_agent.as_deref(),
                Some("Global UA/1.0")
            );
            let loaded = config::Config::load(
                config::BootstrapConfig {
                    listen: state.config.load().listen,
                    data_dir: std::env::temp_dir(),
                    database_path: ":memory:".into(),
                },
                &state.db,
            )
            .await
            .unwrap();
            assert_eq!(loaded.upstream_user_agent.as_deref(), Some("Global UA/1.0"));
            assert_eq!(
                settings(State(state.clone()), Extension(user.clone()))
                    .await
                    .unwrap()
                    .0["upstream_user_agent"],
                "Global UA/1.0"
            );
            for invalid in ["UA\r\nX-Injected: true".to_owned(), "x".repeat(1025)] {
                let error = update_upstream_user_agent(
                    State(state.clone()),
                    ConnectInfo(peer),
                    Extension(user.clone()),
                    Json(UpdateUpstreamUserAgent {
                        originator: None,
                        user_agent: None,
                        upstream_user_agent: Some(invalid),
                    }),
                )
                .await
                .unwrap_err();
                assert_eq!(error.status(), StatusCode::BAD_REQUEST);
                assert_eq!(
                    state.config.load().upstream_user_agent.as_deref(),
                    Some("Global UA/1.0")
                );
            }
            let cleared = update_upstream_user_agent(
                State(state.clone()),
                ConnectInfo(peer),
                Extension(user),
                Json(UpdateUpstreamUserAgent {
                    originator: None,
                    user_agent: None,
                    upstream_user_agent: Some(" ".into()),
                }),
            )
            .await
            .unwrap();
            assert_eq!(cleared.0["ok"], true);
            assert_eq!(state.config.load().upstream_user_agent, None);
            let stored: String =
                sqlx::query_scalar("SELECT value FROM app_meta WHERE key='upstream_user_agent'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(stored, "");
        }
        let audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_audit WHERE action='settings.upstream_user_agent.update'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(audits, 4);
    }

    #[tokio::test]
    async fn upstream_user_agent_overrides_are_independent_per_originator() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','root',0)")
            .execute(&state.db)
            .await
            .unwrap();
        let peer = "127.0.0.1:9000".parse::<SocketAddr>().unwrap();
        for (originator, user_agent) in [
            ("pi", "Pi upstream/1.0"),
            ("opencode", "OpenCode upstream/1.0"),
        ] {
            let _ = update_upstream_user_agent(
                State(state.clone()),
                ConnectInfo(peer),
                Extension(root_identity()),
                Json(UpdateUpstreamUserAgent {
                    originator: Some(originator.to_owned()),
                    user_agent: Some(user_agent.to_owned()),
                    upstream_user_agent: None,
                }),
            )
            .await
            .unwrap();
        }
        let config = state.config.load();
        assert_eq!(
            config.upstream_user_agents.pi.as_deref(),
            Some("Pi upstream/1.0")
        );
        assert_eq!(
            config.upstream_user_agents.opencode.as_deref(),
            Some("OpenCode upstream/1.0")
        );
        assert_eq!(config.upstream_user_agents.codex_cli_rs, None);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM app_meta WHERE key='upstream_user_agent_pi'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "Pi upstream/1.0"
        );
        let settings = settings(State(state), Extension(root_identity()))
            .await
            .unwrap();
        assert_eq!(settings.0["upstream_user_agents"]["pi"], "Pi upstream/1.0");
        assert_eq!(
            settings.0["upstream_user_agents"]["opencode"],
            "OpenCode upstream/1.0"
        );
    }

    #[tokio::test]
    async fn provider_concurrency_is_admin_only_persistent_and_visible_to_the_owner() {
        use futures_util::poll;
        let state = crate::test_state("http://token.invalid").await;
        for (id, role) in [("owner", "user"), ("admin", "admin"), ("root", "root")] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,?,0)")
                .bind(id)
                .bind(role)
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,owner_id,created_at,updated_at,visibility) VALUES('provider','provider','account','access','refresh','owner',0,0,'public')")
            .execute(&state.db).await.unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
        let owner = UserIdentity {
            id: "owner".into(),
            email: None,
            name: None,
            role: "user".into(),
        };
        let admin = UserIdentity {
            id: "admin".into(),
            role: "admin".into(),
            ..owner.clone()
        };
        let root = UserIdentity {
            id: "root".into(),
            role: "root".into(),
            ..owner.clone()
        };
        let peer = "127.0.0.1:9000".parse::<SocketAddr>().unwrap();
        assert_eq!(
            settings(State(state.clone()), Extension(admin.clone()))
                .await
                .unwrap()
                .0["provider_concurrency_limit"],
            3
        );
        let forbidden = update_provider_concurrency(
            State(state.clone()),
            ConnectInfo(peer),
            Extension(owner.clone()),
            Json(UpdateProviderConcurrency {
                provider_concurrency_limit: 2,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
        let invalid = update_provider_concurrency(
            State(state.clone()),
            ConnectInfo(peer),
            Extension(admin.clone()),
            Json(UpdateProviderConcurrency {
                provider_concurrency_limit: 0,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_eq!(state.config.load().provider_concurrency_limit, 3);
        for user in [root, admin.clone()] {
            let _ = update_provider_concurrency(
                State(state.clone()),
                ConnectInfo(peer),
                Extension(user),
                Json(UpdateProviderConcurrency {
                    provider_concurrency_limit: 2,
                }),
            )
            .await
            .unwrap();
        }
        let config = config::Config::load(
            crate::config::BootstrapConfig {
                listen: state.config.load().listen,
                data_dir: std::env::temp_dir(),
                database_path: ":memory:".into(),
            },
            &state.db,
        )
        .await
        .unwrap();
        assert_eq!(config.provider_concurrency_limit, 2);
        let first = state
            .balancer
            .select(&state, None, None, test_downstream())
            .await
            .unwrap();
        let second = state
            .balancer
            .select(&state, None, None, test_downstream())
            .await
            .unwrap();
        let mut queued = Box::pin(state.balancer.select(&state, None, None, test_downstream()));
        assert!(poll!(&mut queued).is_pending());
        for user in [owner.clone(), admin.clone()] {
            let providers = list_providers(State(state.clone()), Extension(user))
                .await
                .unwrap();
            assert_eq!(providers.0[0]["inflight"], 2);
            assert_eq!(providers.0[0]["queued"], 1);
            assert_eq!(providers.0[0]["concurrency_limit"], 2);
        }
        let _ = update_provider_concurrency(
            State(state.clone()),
            ConnectInfo(peer),
            Extension(admin),
            Json(UpdateProviderConcurrency {
                provider_concurrency_limit: 3,
            }),
        )
        .await
        .unwrap();
        let third = queued.await.unwrap();
        drop((first, second, third));
        let providers = list_providers(State(state.clone()), Extension(owner))
            .await
            .unwrap();
        assert_eq!(providers.0[0]["queued"], 0);
        assert_eq!(providers.0[0]["inflight"], 0);
        let audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_audit WHERE action='settings.provider_concurrency.update'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(audits, 3);
    }

    #[tokio::test]
    async fn admin_can_update_the_available_model_list() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('admin','admin',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        let admin = UserIdentity {
            id: "admin".to_owned(),
            email: None,
            name: None,
            role: "admin".to_owned(),
        };

        let _ = update_available_model_ids(
            State(state.clone()),
            ConnectInfo("127.0.0.1:9000".parse::<SocketAddr>().unwrap()),
            Extension(admin.clone()),
            Json(UpdateAvailableModelIds {
                available_model_ids: vec!["gpt-5.6-terra".to_owned(), "gpt-5.6-luna".to_owned()],
            }),
        )
        .await
        .unwrap();

        assert_eq!(
            state.config.load().available_model_ids,
            ["gpt-5.6-terra", "gpt-5.6-luna"]
        );
        let stored: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='available_model_ids'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(stored, r#"["gpt-5.6-terra","gpt-5.6-luna"]"#);
        let prices = model_prices(State(state.clone())).await;
        assert_eq!(prices.0["rows"].as_array().unwrap().len(), 2);
        let settings = settings(State(state), Extension(admin)).await.unwrap();
        assert_eq!(
            settings.0["available_model_ids"],
            json!(["gpt-5.6-terra", "gpt-5.6-luna"])
        );
    }

    #[tokio::test]
    async fn setup_atomically_binds_root_and_closes() {
        let signing = SigningKey::from_bytes(&[23_u8; 32]);
        let x = URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes());
        let jwks = Router::new().route("/jwks", get(move || {
            let x = x.clone();
            async move { Json(json!({"keys":[{"kid":"setup","kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","x":x}]})) }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, jwks).await.unwrap() });
        let issuer = format!("http://{address}");
        let token = setup_token(&signing, &issuer, "root-user");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        let state = crate::test_state("http://token.invalid").await;
        let wrong_audience = state
            .auth
            .verify_candidate(issuer.clone(), "other.example.com".to_owned(), &token)
            .await;
        assert!(matches!(
            wrong_audience,
            Err(error) if error.status() == StatusCode::UNAUTHORIZED
        ));
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at) VALUES('legacy-provider','legacy','legacy','access','refresh',?,?)")
            .bind(now)
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        let response = setup(
            State(state.clone()),
            headers.clone(),
            Json(SetupInput {
                auth_issuer: issuer.clone(),
                auth_audience: TEST_AUTH_AUDIENCE.to_owned(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(response.0["root_user_id"], "root-user");
        let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id='root-user'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(role, "root");
        let owner_id: String =
            sqlx::query_scalar("SELECT owner_id FROM providers WHERE id='legacy-provider'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(owner_id, "root-user");
        assert!(state.config.load().setup_complete);
        let second = setup(
            State(state),
            headers,
            Json(SetupInput {
                auth_issuer: issuer,
                auth_audience: TEST_AUTH_AUDIENCE.to_owned(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(second.status(), axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn system_resources_requires_administrator_access() {
        let state = crate::test_state("http://token.invalid").await;

        let error = system_resources(
            State(state),
            Extension(UserIdentity {
                id: "tenant".to_owned(),
                email: None,
                name: None,
                role: "user".to_owned(),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn provider_audit_requires_admin_and_groups_hourly_results() {
        let state = crate::test_state("http://token.invalid").await;
        // INVARIANT: The three recent calls must share an hour even when this
        // test runs in the first minute of a wall-clock hour.
        let now = chrono::Utc::now().timestamp().div_euclid(3600) * 3600 - 60;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('admin','admin',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('audit-consumer','admin','audit','sk-audit','audit-hash',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at,owner_id) VALUES('audit-provider','Provider A','account','access','refresh',?,?, 'admin')")
            .bind(now)
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,status,latency_ms,input_tokens,created_at) VALUES('audit-success','audit-success','audit-consumer','admin','audit-provider','POST','/v1/responses','gpt-5.6',200,1,11,?)")
            .bind(now - 60)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,status,latency_ms,input_tokens,error,error_code,created_at) VALUES('audit-failure','audit-failure','audit-consumer','admin','audit-provider','POST','/v1/responses','gpt-5.6',200,1,7,'upstream failed','server_error',?)")
            .bind(now - 30)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,status,latency_ms,input_tokens,error,created_at) VALUES('audit-http-error','audit-http-error','audit-consumer','admin','audit-provider','POST','/v1/responses','gpt-5.6',500,1,5,'client request rejected',?)")
            .bind(now - 20)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,status,latency_ms,input_tokens,created_at) VALUES('audit-old','audit-old','audit-consumer','admin','audit-provider','POST','/v1/responses','gpt-5.6',200,1,13,?)")
            .bind(now - 25 * 60 * 60)
            .execute(&state.db)
            .await
            .unwrap();

        let forbidden = provider_audit(
            State(state.clone()),
            Extension(UserIdentity {
                id: "tenant".to_owned(),
                email: None,
                name: None,
                role: "user".to_owned(),
            }),
            Query(UsageQuery { period: None }),
        )
        .await
        .unwrap_err();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

        let response = provider_audit(
            State(state.clone()),
            Extension(UserIdentity {
                id: "admin".to_owned(),
                email: None,
                name: None,
                role: "admin".to_owned(),
            }),
            Query(UsageQuery { period: None }),
        )
        .await
        .unwrap();
        let rows = response.0["rows"].as_array().unwrap();
        assert_eq!(response.0["period"], "7d");
        assert_eq!(rows.len(), 2);
        let current = rows.iter().find(|row| row["requests"] == 3).unwrap();
        assert_eq!(current["provider_name"], "Provider A");
        assert_eq!(current["model"], "gpt-5.6");
        assert_eq!(current["successful_requests"], 2);
        assert_eq!(current["failed_requests"], 1);
        assert_eq!(current["input_tokens"], 23);
        assert_eq!(current["success_rate"], 2.0 / 3.0);
        assert_eq!(current["failure_rate"], 1.0 / 3.0);

        let response = provider_audit(
            State(state),
            Extension(UserIdentity {
                id: "admin".to_owned(),
                email: None,
                name: None,
                role: "admin".to_owned(),
            }),
            Query(UsageQuery {
                period: Some(UsagePeriod::Last24Hours),
            }),
        )
        .await
        .unwrap();
        let rows = response.0["rows"].as_array().unwrap();
        assert_eq!(response.0["period"], "24h");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["requests"], 3);
        assert_eq!(rows[0]["input_tokens"], 23);
    }

    #[tokio::test]
    async fn model_downgrade_audit_requires_admin_and_groups_hourly_flows() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('admin','admin',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('audit-consumer','admin','audit','sk-audit','audit-hash',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at,owner_id) VALUES('audit-provider','Provider A','account','access','refresh',?,?, 'admin')")
            .bind(now)
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        let calls = [
            ("downgraded", "gpt-5.6-luna", 60),
            ("consistent", "gpt-6-astra", 30),
            ("earlier", "gpt-5.6-sol", 25 * 3600),
        ];
        for (id, upstream_model, age) in calls {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,upstream_model,status,latency_ms,created_at) VALUES(?,?,'audit-consumer','admin','audit-provider','POST','/v1/responses','gpt-6-astra',?,200,1,?)")
                .bind(id)
                .bind(id)
                .bind(upstream_model)
                .bind(now - age)
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,model,status,latency_ms,created_at) VALUES('unrecorded','unrecorded','audit-consumer','admin','audit-provider','POST','/v1/responses','gpt-6-astra',200,1,?)")
            .bind(now - 10)
            .execute(&state.db)
            .await
            .unwrap();

        let forbidden = model_downgrade_audit(
            State(state.clone()),
            Extension(UserIdentity {
                id: "tenant".to_owned(),
                email: None,
                name: None,
                role: "user".to_owned(),
            }),
            Query(UsageQuery { period: None }),
        )
        .await
        .unwrap_err();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

        let response = model_downgrade_audit(
            State(state.clone()),
            Extension(UserIdentity {
                id: "admin".to_owned(),
                email: None,
                name: None,
                role: "admin".to_owned(),
            }),
            Query(UsageQuery { period: None }),
        )
        .await
        .unwrap();
        let rows = response.0["rows"].as_array().unwrap();
        assert_eq!(response.0["period"], "7d");
        assert_eq!(rows.len(), 3);
        let downgraded = rows
            .iter()
            .find(|row| row["upstream_model"] == "gpt-5.6-luna")
            .unwrap();
        assert_eq!(downgraded["downstream_model"], "gpt-6-astra");
        assert_eq!(downgraded["provider_name"], "Provider A");
        assert_eq!(downgraded["requests"], 1);
        assert_eq!(downgraded["hour_start"], ((now - 60) / 3600) * 3600);

        let response = model_downgrade_audit(
            State(state),
            Extension(UserIdentity {
                id: "admin".to_owned(),
                email: None,
                name: None,
                role: "admin".to_owned(),
            }),
            Query(UsageQuery {
                period: Some(UsagePeriod::Last24Hours),
            }),
        )
        .await
        .unwrap();
        let rows = response.0["rows"].as_array().unwrap();
        assert_eq!(response.0["period"], "24h");
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn dashboard_returns_only_current_user_stats() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('tenant','user',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('consumer','tenant','consumer','sk-test','hash',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users(id,role,official_consumed_usd_nanos,consumed_usd_nanos,created_at) VALUES('other','user',900,800,?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('other-consumer','other','other','sk-other','other-hash',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        for (id, input_tokens, output_tokens, cached_tokens, created_at) in [
            ("first", 11_i64, 7_i64, 5_i64, now - 60),
            ("second", 3, 2, 1, now - 120),
            ("expired", 100, 80, 50, now - 86_401),
        ] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,input_tokens,output_tokens,cached_tokens,created_at) VALUES(?,?, 'consumer','tenant','POST','/v1/responses',200,1,?,?,?,?)")
                .bind(id)
                .bind(id)
                .bind(input_tokens)
                .bind(output_tokens)
                .bind(cached_tokens)
                .bind(created_at)
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,input_tokens,output_tokens,cached_tokens,official_cost_usd_nanos,actual_cost_usd_nanos,created_at) VALUES('other-call','other-call','other-consumer','other','POST','/v1/responses',500,1,100,80,50,100,80,?)")
            .bind(now - 60)
            .execute(&state.db)
            .await
            .unwrap();

        let response = dashboard(
            State(state),
            Extension(UserIdentity {
                id: "tenant".to_owned(),
                email: None,
                name: None,
                role: "user".to_owned(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(response.0["active_consumers"], 1);
        assert_eq!(response.0["calls_24h"], 2);
        assert_eq!(response.0["errors_24h"], 0);
        assert_eq!(response.0["input_tokens_24h"], 14);
        assert_eq!(response.0["output_tokens_24h"], 9);
        assert_eq!(response.0["cached_tokens_24h"], 6);
        assert_eq!(response.0["official_cost_usd_nanos_24h"], 0);
        assert_eq!(response.0["actual_cost_usd_nanos_24h"], 0);
        assert_eq!(response.0["official_consumed_usd_nanos"], 0);
        assert_eq!(response.0["consumed_usd_nanos"], 0);
        assert!(response.0.get("available_providers").is_none());
    }

    #[tokio::test]
    async fn audit_and_dashboard_classify_http_200_sse_failures() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        for owner in ["tenant", "other"] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,'user',?)")
                .bind(owner)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES(?,?,?,?,?,?)")
                .bind(owner).bind(owner).bind(owner).bind(owner).bind(owner).bind(now)
                .execute(&state.db).await.unwrap();
        }
        for (id, owner, status, code, message) in [
            ("success", "tenant", 200, None, None),
            (
                "failed",
                "tenant",
                200,
                Some("server_is_overloaded"),
                Some("busy"),
            ),
            (
                "code-only",
                "tenant",
                200,
                Some("unknown_future_code"),
                None,
            ),
            ("message-only", "tenant", 200, None, Some("response.failed")),
            ("http-error", "tenant", 502, None, None),
            (
                "client-cancelled",
                "tenant",
                499,
                None,
                Some("client_cancelled"),
            ),
            (
                "cancelled-with-code",
                "tenant",
                499,
                Some("upstream_failure"),
                Some("client_cancelled"),
            ),
            (
                "other",
                "other",
                200,
                Some("server_is_overloaded"),
                Some("other user"),
            ),
        ] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,error_code,error,created_at) VALUES(?,?,?,?, 'POST','/v1/responses',?,1,?,?,?)")
                .bind(id).bind(id).bind(owner).bind(owner).bind(status).bind(code).bind(message).bind(now)
                .execute(&state.db).await.unwrap();
        }
        let user = UserIdentity {
            id: "tenant".to_owned(),
            email: None,
            name: None,
            role: "user".to_owned(),
        };
        for (filter, expected) in [
            (json!({"status":"success"}), 2),
            (json!({"status":"error"}), 5),
            (
                json!({"status":"error","error_code":"server_is_overloaded"}),
                1,
            ),
            (json!({"error_code":"server"}), 0),
            (json!({"error_code":"unknown_future_code"}), 1),
        ] {
            let filters = serde_json::from_value(filter).unwrap();
            let response = audit(
                State(state.clone()),
                Extension(user.clone()),
                Query(filters),
            )
            .await
            .unwrap()
            .0;
            assert_eq!(response["total"], expected);
            assert_eq!(
                response["rows"].as_array().unwrap().len(),
                expected as usize
            );
        }
        let detail = audit_detail(
            State(state.clone()),
            Extension(user.clone()),
            Path("failed".to_owned()),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(detail["status"], 200);
        assert_eq!(detail["error_code"], "server_is_overloaded");
        assert_eq!(detail["error"], "busy");
        assert_eq!(detail["archive_available"], false);
        let response = dashboard(State(state), Extension(user)).await.unwrap().0;
        assert_eq!(response["calls_24h"], 7);
        assert_eq!(response["errors_24h"], 5);
    }

    #[tokio::test]
    async fn tenant_audit_scope_cannot_read_another_user() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        for user in ["tenant-a", "tenant-b"] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,'user',?)")
                .bind(user)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES(?,?,?,? ,?,?)")
                .bind(format!("key-{user}")).bind(user).bind(user).bind(user)
                .bind(format!("hash-{user}")).bind(now).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,created_at) VALUES(?,?,?,?, 'POST','/v1/responses',200,1,?)")
                .bind(format!("call-{user}")).bind("shared-client-request-id").bind(format!("key-{user}"))
                .bind(user).bind(now).execute(&state.db).await.unwrap();
        }
        let visible: Vec<String> = sqlx::query_scalar("SELECT id FROM api_calls WHERE user_id=?")
            .bind("tenant-a")
            .fetch_all(&state.db)
            .await
            .unwrap();
        assert_eq!(visible, vec!["call-tenant-a"]);
        let duplicated_request_ids: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM api_calls WHERE request_id='shared-client-request-id'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(duplicated_request_ids, 2);
    }

    #[tokio::test]
    async fn audit_detail_shows_sensitive_headers_only_to_admins() {
        let signing = SigningKey::from_bytes(&[37_u8; 32]);
        let x = URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes());
        let jwks = Router::new().route(
            "/jwks",
            get(move || {
                let x = x.clone();
                async move {
                    Json(json!({"keys":[{"kid":"setup","kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","x":x}]}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, jwks).await.unwrap() });
        let issuer = format!("http://{address}");
        let state = crate::test_state("http://token.invalid").await;
        state
            .auth
            .configure(issuer.clone(), TEST_AUTH_AUDIENCE.to_owned())
            .await
            .unwrap();
        let now = chrono::Utc::now().timestamp();
        for (id, role) in [("admin", "admin"), ("tenant", "user")] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,?,?)")
                .bind(id)
                .bind(role)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('tenant-key','tenant','tenant','sk-tenant','hash-tenant',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,created_at) VALUES('tenant-call','tenant-request','tenant-key','tenant','POST','/v1/responses',200,1,?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        let downstream = r#"[["authorization","Bearer consumer-secret"],["x-api-key","consumer-api-key"],["content-type","application/json"]]"#;
        let upstream = r#"[["authorization","Bearer upstream-secret"],["x-session-id","session-secret"],["content-type","application/json"]]"#;
        sqlx::query("INSERT INTO request_archives(api_call_id,request_headers_json,upstream_request_headers_json,request_body,request_body_truncated,response_headers_json,downstream_response_headers_json,response_body,response_body_truncated,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind("tenant-call")
            .bind(downstream)
            .bind(upstream)
            .bind(b"{}".as_slice())
            .bind(0)
            .bind(downstream)
            .bind(upstream)
            .bind(b"{}".as_slice())
            .bind(0)
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();

        let admin = provider_request(
            &state,
            Method::GET,
            "/api/audit/tenant-call",
            &setup_token(&signing, &issuer, "admin"),
            None,
        )
        .await;
        assert_eq!(admin.status(), StatusCode::OK);
        let admin: Value =
            serde_json::from_slice(&admin.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(admin["request_headers"], downstream);
        assert_eq!(admin["upstream_request_headers"], upstream);

        let tenant = provider_request(
            &state,
            Method::GET,
            "/api/audit/tenant-call",
            &setup_token(&signing, &issuer, "tenant"),
            None,
        )
        .await;
        assert_eq!(tenant.status(), StatusCode::OK);
        let tenant: Value =
            serde_json::from_slice(&tenant.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        for field in [
            "request_headers",
            "upstream_request_headers",
            "response_headers",
            "downstream_response_headers",
        ] {
            let headers = tenant[field].as_str().unwrap();
            assert!(headers.contains("content-type"));
            assert!(!headers.contains("secret"));
            assert!(!headers.contains("api-key"));
            assert!(!headers.contains("session-id"));
        }
    }

    #[tokio::test]
    async fn audit_bodies_can_be_deleted_by_the_owner_or_an_admin() {
        let signing = SigningKey::from_bytes(&[39_u8; 32]);
        let x = URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes());
        let jwks = Router::new().route(
            "/jwks",
            get(move || {
                let x = x.clone();
                async move {
                    Json(json!({"keys":[{"kid":"setup","kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","x":x}]}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, jwks).await.unwrap() });
        let issuer = format!("http://{address}");
        let state = crate::test_state("http://token.invalid").await;
        state
            .auth
            .configure(issuer.clone(), TEST_AUTH_AUDIENCE.to_owned())
            .await
            .unwrap();
        let now = chrono::Utc::now().timestamp();
        for (id, role) in [("tenant", "user"), ("other", "user"), ("admin", "admin")] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,?,?)")
                .bind(id)
                .bind(role)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }
        for user_id in ["tenant", "other"] {
            sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES(?,?,?,?,?,?)")
                .bind(format!("{user_id}-consumer"))
                .bind(user_id)
                .bind(user_id)
                .bind(format!("sk-{user_id}"))
                .bind(format!("hash-{user_id}"))
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,input_tokens,output_tokens,created_at) VALUES(?,?,?,?, 'POST','/v1/responses',200,1,3,5,?)")
                .bind(format!("{user_id}-call"))
                .bind(format!("{user_id}-request"))
                .bind(format!("{user_id}-consumer"))
                .bind(user_id)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO request_archives(api_call_id,request_headers_json,request_body,request_body_truncated,response_headers_json,response_body,response_body_truncated,created_at) VALUES(?,?,?,0,?,?,0,?)")
                .bind(format!("{user_id}-call"))
                .bind(r#"[["content-type","application/json"]]"#)
                .bind(json!({"input": user_id}).to_string())
                .bind(r#"[["content-type","application/json"]]"#)
                .bind(json!({"output": user_id}).to_string())
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }

        let tenant_token = setup_token(&signing, &issuer, "tenant");
        let tenant_delete = provider_request(
            &state,
            Method::DELETE,
            "/api/audit/tenant-call/bodies",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(tenant_delete.status(), StatusCode::OK);

        let tenant_detail = provider_request(
            &state,
            Method::GET,
            "/api/audit/tenant-call",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(tenant_detail.status(), StatusCode::OK);
        let tenant_detail: Value = serde_json::from_slice(
            &tenant_detail
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(tenant_detail["archive_available"], true);
        assert_eq!(tenant_detail["bodies_available"], false);
        assert!(tenant_detail["request_headers"].is_string());
        assert!(tenant_detail["response_headers"].is_string());

        let tenant_archive: (Vec<u8>, Option<Vec<u8>>, i64, i64, i64) = sqlx::query_as(
            "SELECT request_body,response_body,request_body_truncated,response_body_truncated,bodies_deleted FROM request_archives WHERE api_call_id='tenant-call'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(tenant_archive, (Vec::new(), None, 0, 0, 1));
        let tenant_call: (i64, i64, i64) = sqlx::query_as(
            "SELECT status,input_tokens,output_tokens FROM api_calls WHERE id='tenant-call'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(tenant_call, (200, 3, 5));

        let foreign = provider_request(
            &state,
            Method::DELETE,
            "/api/audit/other-call/bodies",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(foreign.status(), StatusCode::NOT_FOUND);

        let admin_delete = provider_request(
            &state,
            Method::DELETE,
            "/api/audit/other-call/bodies",
            &setup_token(&signing, &issuer, "admin"),
            None,
        )
        .await;
        assert_eq!(admin_delete.status(), StatusCode::OK);
        let admin_audit: (String, String, String) = sqlx::query_as(
            "SELECT action,target_id,client_ip FROM admin_audit WHERE action='audit.bodies.delete'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            admin_audit,
            (
                "audit.bodies.delete".to_owned(),
                "other-call".to_owned(),
                "127.0.0.1".to_owned(),
            )
        );
    }

    #[tokio::test]
    async fn usage_rows_groups_current_window_by_user_key_and_model() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        for (id, role) in [
            ("tenant-a", "user"),
            ("tenant-b", "user"),
            ("admin", "admin"),
        ] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,?,?)")
                .bind(id)
                .bind(role)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }
        for (id, user_id) in [("key-a", "tenant-a"), ("key-b", "tenant-b")] {
            sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES(?,?,?,? ,?,?)")
                .bind(id)
                .bind(user_id)
                .bind(id)
                .bind(format!("sk-{id}"))
                .bind(format!("hash-{id}"))
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }
        for (
            id,
            consumer_id,
            user_id,
            model,
            input,
            cached,
            output,
            official_cost,
            actual_cost,
            created_at,
        ) in [
            (
                "a-1",
                "key-a",
                "tenant-a",
                "gpt-5",
                10,
                3,
                4,
                15_000,
                1_500,
                now - 60,
            ),
            (
                "a-2",
                "key-a",
                "tenant-a",
                "gpt-5",
                6,
                2,
                8,
                25_000,
                2_500,
                now - 120,
            ),
            (
                "b-1",
                "key-b",
                "tenant-b",
                "gpt-4.1",
                5,
                1,
                2,
                10_000,
                1_000,
                now - 60,
            ),
            (
                "a-old",
                "key-a",
                "tenant-a",
                "gpt-5",
                99,
                0,
                0,
                999_999,
                99_999,
                now - 8 * 24 * 60 * 60,
            ),
        ] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,model,status,latency_ms,input_tokens,cached_tokens,output_tokens,official_cost_usd_nanos,actual_cost_usd_nanos,created_at) VALUES(?,?,?,?,'POST','/v1/responses',?,200,1,?,?,?,?,?,?)")
                .bind(id)
                .bind(id)
                .bind(consumer_id)
                .bind(user_id)
                .bind(model)
                .bind(input)
                .bind(cached)
                .bind(output)
                .bind(official_cost)
                .bind(actual_cost)
                .bind(created_at)
                .execute(&state.db)
                .await
                .unwrap();
        }

        let tenant = crate::auth::UserIdentity {
            id: "tenant-a".to_owned(),
            email: Some("a@example.com".to_owned()),
            name: Some("tenant-a".to_owned()),
            role: "user".to_owned(),
        };
        let tenant_rows = usage_rows(&state, &tenant, now - 24 * 60 * 60)
            .await
            .unwrap();
        assert_eq!(tenant_rows.len(), 1);
        assert_eq!(tenant_rows[0]["requests"], 2);
        assert_eq!(tenant_rows[0]["input_tokens"], 16);
        assert_eq!(tenant_rows[0]["cached_tokens"], 5);
        assert_eq!(tenant_rows[0]["output_tokens"], 12);
        assert_eq!(tenant_rows[0]["official_cost_usd_nanos"], 40_000);
        assert_eq!(tenant_rows[0]["actual_cost_usd_nanos"], 4_000);
        assert_eq!(tenant_rows[0]["model"], "gpt-5");

        let admin = crate::auth::UserIdentity {
            id: "admin".to_owned(),
            email: Some("admin@example.com".to_owned()),
            name: Some("admin".to_owned()),
            role: "admin".to_owned(),
        };
        let admin_rows = usage_rows(&state, &admin, now - 24 * 60 * 60)
            .await
            .unwrap();
        assert_eq!(admin_rows.len(), 2);
        assert_eq!(
            admin_rows
                .iter()
                .map(|row| row["requests"].as_i64().unwrap())
                .sum::<i64>(),
            3
        );
    }

    #[tokio::test]
    async fn usage_rows_split_dates_at_utc_midnight() {
        let state = crate::test_state("http://token.invalid").await;
        let midnight_utc = 1_704_067_200_i64;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('tenant','user',?)")
            .bind(midnight_utc)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('key','tenant','key','sk-utc','hash-utc',?)")
            .bind(midnight_utc)
            .execute(&state.db)
            .await
            .unwrap();
        for (id, created_at) in [("before", midnight_utc - 1), ("after", midnight_utc)] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,created_at) VALUES(?,?, 'key','tenant','POST','/v1/responses',200,1,?)")
                .bind(id)
                .bind(id)
                .bind(created_at)
                .execute(&state.db)
                .await
                .unwrap();
        }
        let user = crate::auth::UserIdentity {
            id: "tenant".to_owned(),
            email: None,
            name: None,
            role: "user".to_owned(),
        };
        let rows = usage_rows(&state, &user, midnight_utc - 60).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["date"], "2023-12-31");
        assert_eq!(rows[1]["date"], "2024-01-01");
    }

    #[tokio::test]
    async fn sensitive_admin_action_is_audited() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('admin','admin',?)")
            .bind(chrono::Utc::now().timestamp())
            .execute(&state.db)
            .await
            .unwrap();
        write_admin_audit(
            &state,
            "admin",
            "provider.create",
            Some("provider-1"),
            "127.0.0.1",
        )
        .await
        .unwrap();
        let row: (String, String) = sqlx::query_as("SELECT action,client_ip FROM admin_audit")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(row, ("provider.create".to_owned(), "127.0.0.1".to_owned()));
    }

    #[tokio::test]
    async fn provider_tokens_are_stored_and_replaced_as_plaintext() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','user',?)")
            .bind(chrono::Utc::now().timestamp())
            .execute(&state.db)
            .await
            .unwrap();
        let access = provider_oauth_access_token("account-old");
        let _ = insert_provider(
            &state,
            "owner",
            "provider",
            ProviderCredentials {
                access: &access,
                refresh: "refresh-old",
                expires_at: None,
            },
            None,
            ProviderRouting {
                originator: identity::CODEX_ORIGINATOR,
                allow_other_originator: false,
                visibility: balancer::PROVIDER_VISIBILITY_PRIVATE,
            },
        )
        .await
        .unwrap();
        let id: String = sqlx::query_scalar("SELECT id FROM providers")
            .fetch_one(&state.db)
            .await
            .unwrap();
        let stored: (String, String) =
            sqlx::query_as("SELECT access_token,refresh_token FROM providers WHERE id=?")
                .bind(&id)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(stored, (access, "refresh-old".to_owned()));

        let replacement = provider_oauth_access_token("account-new");
        let _ = replace_provider_token_values(
            &state,
            &id,
            Some("provider renamed"),
            &replacement,
            "refresh-new",
        )
        .await
        .unwrap();
        let updated: (String, String, String, String) = sqlx::query_as(
            "SELECT name,access_token,refresh_token,account_id FROM providers WHERE id=?",
        )
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            updated,
            (
                "provider renamed".to_owned(),
                replacement,
                "refresh-new".to_owned(),
                "account-new".to_owned()
            )
        );

        let replacement_without_name = provider_oauth_access_token("account-latest");
        let _ = replace_provider_token_values(
            &state,
            &id,
            None,
            &replacement_without_name,
            "refresh-latest",
        )
        .await
        .unwrap();
        let preserved_name: String = sqlx::query_scalar("SELECT name FROM providers WHERE id=?")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(preserved_name, "provider renamed");
    }

    #[tokio::test]
    async fn provider_without_a_name_uses_its_uuid() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','user',?)")
            .bind(chrono::Utc::now().timestamp())
            .execute(&state.db)
            .await
            .unwrap();
        let access = provider_oauth_access_token("account");
        let provider = insert_provider(
            &state,
            "owner",
            "",
            ProviderCredentials {
                access: &access,
                refresh: "refresh",
                expires_at: None,
            },
            None,
            ProviderRouting {
                originator: identity::CODEX_ORIGINATOR,
                allow_other_originator: false,
                visibility: balancer::PROVIDER_VISIBILITY_PRIVATE,
            },
        )
        .await
        .unwrap();
        let id = provider.0["id"].as_str().unwrap();
        assert_eq!(provider.0["name"], id);
        let stored_name: String = sqlx::query_scalar("SELECT name FROM providers WHERE id=?")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(stored_name, id);
    }

    #[tokio::test]
    async fn provider_test_calls_usage_with_server_side_credentials() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let usage = Router::new().route(
            "/backend-api/wham/usage",
            get(move |headers: HeaderMap| {
                let sender = sender.clone();
                async move {
                    sender.send(headers).await.unwrap();
                    Json(json!({
                        "plan_type":"team",
                        "rate_limit":{"primary_window":{"used_percent":25.0}}
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, usage).await.unwrap() });
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            &format!("http://{address}/backend-api/codex"),
        )
        .await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at) VALUES('provider','Provider','account','access','refresh',?,?)")
            .bind(now).bind(now).execute(&state.db).await.unwrap();

        let result = provider_usage(&state, "provider").await.unwrap();
        assert_eq!(result.0["usage"]["plan_type"], "team");
        assert_eq!(
            result.0["usage"]["rate_limit"]["primary_window"]["used_percent"],
            25.0
        );
        let headers = receiver.recv().await.unwrap();
        assert_eq!(headers[axum::http::header::AUTHORIZATION], "Bearer access");
        assert_eq!(headers["chatgpt-account-id"], "account");
    }

    #[tokio::test]
    async fn provider_capacity_is_public_summary_and_aggregates_plus_equivalents() {
        let usage = Router::new().route(
            "/backend-api/wham/usage",
            get(|headers: HeaderMap| async move {
                let usage = match headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                {
                    Some("Bearer pro-a") => {
                        json!({"user_id":"same-upstream-user","plan_type":"pro","rate_limit":{"primary_window":{"used_percent":68}}})
                    }
                    Some("Bearer pro-b") => {
                        json!({"user_id":"same-upstream-user","plan_type":"pro","rate_limit":{"primary_window":{"used_percent":11}}})
                    }
                    Some("Bearer prolite") => {
                        json!({"plan_type":"prolite","rate_limit":{"primary_window":{"used_percent":40}}})
                    }
                    _ => {
                        json!({"plan_type":"team","rate_limit":{"primary_window":{"used_percent":0}}})
                    }
                };
                Json(usage)
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, usage).await.unwrap() });
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            &format!("http://{address}/backend-api/codex"),
        )
        .await;
        let now = chrono::Utc::now().timestamp();
        for (id, access_token) in [
            ("pro-a", "pro-a"),
            ("pro-b", "pro-b"),
            ("prolite", "prolite"),
            ("team", "team"),
        ] {
            sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at) VALUES(?,?,?,?, 'refresh',?,?)")
                .bind(id)
                .bind(id)
                .bind(id)
                .bind(access_token)
                .bind(now)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }

        poll_provider_capacity(&state).await.unwrap();

        let capacity = provider_capacity(State(state.clone())).await.unwrap();
        assert_eq!(capacity.0["provider_count"], 4);
        assert_eq!(capacity.0["included_provider_count"], 2);
        assert_eq!(capacity.0["plus_equivalent_remaining_basis_points"], 94_000);
        assert!(capacity.0["last_sampled_at"].as_i64().is_some());
        assert!(capacity.0.get("providers").is_none());
        let history = capacity.0["history"].as_array().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0]["plus_equivalent_remaining_basis_points"], 94_000);
        let stored: (Option<String>, String, i64, Option<String>) = sqlx::query_as(
            "SELECT upstream_user_id,plan_type,remaining_basis_points,last_error FROM provider_capacity_snapshots WHERE provider_id='pro-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            stored,
            (
                Some("same-upstream-user".to_owned()),
                "pro".to_owned(),
                3_200,
                None
            )
        );
        let history: (i64, i64, i64) = sqlx::query_as(
            "SELECT plus_equivalent_remaining_basis_points,included_provider_count,provider_count FROM provider_capacity_history",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(history, (94_000, 2, 4));
        assert_eq!(plus_equivalent_multiplier(Some("plus")), Some(1));
        assert_eq!(plus_equivalent_multiplier(Some("prolite")), Some(5));
        assert_eq!(plus_equivalent_multiplier(Some("pro")), Some(20));
        assert_eq!(plus_equivalent_multiplier(Some("team")), None);
    }

    #[tokio::test]
    async fn provider_rate_limit_resets_use_server_side_credentials_and_redeem_request_id() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
        let read_sender = sender.clone();
        let credits = Router::new()
            .route(
                "/backend-api/wham/rate-limit-reset-credits",
                get(move |headers: HeaderMap| {
                    let sender = read_sender.clone();
                    async move {
                        sender.send((headers, None)).await.unwrap();
                        Json(json!({
                            "available_count": 1,
                            "credits": [{
                                "id": "credit-1",
                                "reset_type": "codex_rate_limits",
                                "status": "available",
                                "granted_at": 1,
                                "expires_at": null,
                                "title": "Rate-limit reset",
                                "description": "Reset Codex limits"
                            }]
                        }))
                    }
                }),
            )
            .route(
                "/backend-api/wham/rate-limit-reset-credits/consume",
                post(move |headers: HeaderMap, Json(body): Json<Value>| {
                    let sender = sender.clone();
                    async move {
                        sender.send((headers, Some(body))).await.unwrap();
                        Json(json!({"code":"reset"}))
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, credits).await.unwrap() });
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            &format!("http://{address}/backend-api/codex"),
        )
        .await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at) VALUES('provider','Provider','account','access','refresh',?,?)")
            .bind(now).bind(now).execute(&state.db).await.unwrap();

        let listed = provider_rate_limit_resets_value(&state, "provider")
            .await
            .unwrap();
        assert_eq!(listed["available_count"], 1);
        assert_eq!(listed["credits"][0]["id"], "credit-1");

        let redeem_request_id = Uuid::new_v4().to_string();
        let consumed = consume_provider_rate_limit_reset_value(
            &state,
            "provider",
            ConsumeProviderRateLimitReset {
                credit_id: Some("credit-1".to_owned()),
                redeem_request_id: redeem_request_id.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(consumed["code"], "reset");

        let (read_headers, read_body) = receiver.recv().await.unwrap();
        assert!(read_body.is_none());
        assert_eq!(read_headers[header::AUTHORIZATION], "Bearer access");
        assert_eq!(read_headers["chatgpt-account-id"], "account");
        let (consume_headers, consume_body) = receiver.recv().await.unwrap();
        assert_eq!(consume_headers[header::AUTHORIZATION], "Bearer access");
        assert_eq!(consume_headers["chatgpt-account-id"], "account");
        assert_eq!(
            consume_body,
            Some(json!({
                "credit_id": "credit-1",
                "redeem_request_id": redeem_request_id
            }))
        );
    }

    #[tokio::test]
    async fn provider_routes_enforce_roles_audit_actions_and_preserve_call_history() {
        let signing = SigningKey::from_bytes(&[31_u8; 32]);
        let x = URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes());
        let external = Router::new()
            .route(
                "/jwks",
                get(move || {
                    let x = x.clone();
                    async move { Json(json!({"keys":[{"kid":"setup","kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","x":x}]})) }
                }),
            )
            .route(
                "/backend-api/wham/usage",
                get(|headers: HeaderMap| async move {
                    if headers[header::AUTHORIZATION] == "Bearer denied" {
                        return (StatusCode::UNAUTHORIZED, "denied").into_response();
                    }
                    Json(json!({"email":"ops@example.com","plan_type":"team","rate_limit":{"primary_window":{"used_percent":10,"reset_after_seconds":1800}}})).into_response()
                }),
            )
            .route(
                "/backend-api/wham/rate-limit-reset-credits",
                get(|| async {
                    Json(json!({
                        "available_count": 1,
                        "credits": [{"id":"credit-1","title":"Rate-limit reset"}]
                    }))
                }),
            )
            .route(
                "/backend-api/wham/rate-limit-reset-credits/consume",
                post(|| async { Json(json!({"code":"reset"})) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, external).await.unwrap() });
        let issuer = format!("http://{address}");
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            &format!("{issuer}/backend-api/codex"),
        )
        .await;
        state
            .auth
            .configure(issuer.clone(), TEST_AUTH_AUDIENCE.to_owned())
            .await
            .unwrap();
        let now = chrono::Utc::now().timestamp();
        for (id, role) in [
            ("root-user", "root"),
            ("admin-user", "admin"),
            ("tenant-user", "user"),
            ("grantee-user", "user"),
        ] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,?,?)")
                .bind(id)
                .bind(role)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('history-key','root-user','history','sk-history','hash-history',?)")
            .bind(now).execute(&state.db).await.unwrap();

        let admin_browser_token = setup_token(&signing, &issuer, "admin-user");
        let users = provider_request(
            &state,
            Method::GET,
            "/api/users",
            &admin_browser_token,
            None,
        )
        .await;
        assert_eq!(users.status(), StatusCode::OK);
        let users: Value =
            serde_json::from_slice(&users.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(
            users
                .as_array()
                .unwrap()
                .iter()
                .find(|user| user["id"] == "tenant-user")
                .and_then(|user| user["allow_debt"].as_bool()),
            Some(false)
        );
        assert!(
            users
                .as_array()
                .unwrap()
                .iter()
                .all(|user| user.get("email").is_none())
        );
        let vacuum = provider_request(
            &state,
            Method::POST,
            "/api/system/resources",
            &admin_browser_token,
            None,
        )
        .await;
        assert_eq!(vacuum.status(), StatusCode::OK);
        let vacuum_audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_audit WHERE action='system.database.vacuum'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(vacuum_audits, 1);
        let allow_debt = provider_request(
            &state,
            Method::PATCH,
            "/api/users/tenant-user",
            &admin_browser_token,
            Some(json!({"allow_debt":true})),
        )
        .await;
        assert_eq!(allow_debt.status(), StatusCode::OK);
        let granted: i64 =
            sqlx::query_scalar("SELECT allow_debt FROM users WHERE id='tenant-user'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(granted, 1);

        for user_id in ["root-user", "admin-user"] {
            let provider_id = format!("provider-{user_id}");
            let call_id = format!("call-{user_id}");
            let thread_id = format!("thread-{user_id}");
            let session_id = format!("session-{user_id}");
            let access = provider_oauth_access_token(&format!("account-{user_id}"));
            sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
                .bind(&provider_id).bind(user_id).bind(format!("account-{user_id}"))
                .bind(access).bind("refresh").bind(now).bind(now).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO api_calls(id,request_id,thread_id,session_id,consumer_id,user_id,provider_id,method,path,status,upstream_http_version,latency_ms,created_at) VALUES(?,?,?,?,'history-key','root-user',?,'POST','/v1/responses',200,'HTTP/2',1,?)")
                .bind(&call_id).bind(&call_id).bind(&thread_id).bind(&session_id).bind(&provider_id).bind(now).execute(&state.db).await.unwrap();
            let browser_token = setup_token(&signing, &issuer, user_id);

            let read = provider_request(
                &state,
                Method::GET,
                &format!("/api/providers/{provider_id}"),
                &browser_token,
                None,
            )
            .await;
            assert_eq!(read.status(), StatusCode::OK);
            assert_eq!(read.headers()[header::CACHE_CONTROL], "no-store");
            let read_body = read.into_body().collect().await.unwrap().to_bytes();
            assert!(std::str::from_utf8(&read_body).unwrap().contains("refresh"));

            let replacement = provider_oauth_access_token(&format!("replacement-{user_id}"));
            let update = provider_request(
                &state,
                Method::PUT,
                &format!("/api/providers/{provider_id}"),
                &browser_token,
                Some(
                    json!({"name":"renamed","access_key":replacement,"refresh_key":"refresh-new"}),
                ),
            )
            .await;
            assert_eq!(update.status(), StatusCode::OK);
            sqlx::query("INSERT INTO request_archives(api_call_id,request_headers_json,upstream_request_headers_json,request_body,request_body_truncated,response_headers_json,downstream_response_headers_json,response_body,response_body_truncated,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
                .bind(&call_id)
                .bind(r#"[["content-type","application/json"]]"#)
                .bind(r#"[["content-type","application/json"],["originator","codex_cli_rs"]]"#)
                .bind(br#"{"input":"audit detail"}"#.as_slice())
                .bind(0)
                .bind(r#"[["content-type","application/json"]]"#)
                .bind(r#"[["content-type","application/json"],["content-encoding","gzip"]]"#)
                .bind(br#"{"output":"diagnostic"}"#.as_slice())
                .bind(0)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
            let affinity_hash =
                crate::balancer::affinity_hash(&format!("history-key:response-{user_id}"));
            sqlx::query("UPDATE api_calls SET affinity_hash=?,affinity_source='previous_response_id' WHERE id=?")
                .bind(&affinity_hash)
                .bind(&call_id)
                .execute(&state.db)
                .await
                .unwrap();
            let audit_model = format!("gpt-audit-{user_id}");
            sqlx::query("UPDATE api_calls SET model=?,upstream_model='upstream-test-model',downstream_user_agent='Test UA/1.0',reasoning_effort='high',fast_mode=1,first_byte_latency_ms=125,request_bytes=256,response_bytes=512,codex_turn_state_length=12,official_provided_usd_before_nanos=100,official_provided_usd_after_nanos=130,actual_provided_usd_before_nanos=10,actual_provided_usd_after_nanos=13 WHERE id=?")
                .bind(&audit_model)
                .bind(&call_id)
                .execute(&state.db)
                .await
                .unwrap();
            let previous_id = format!("previous-{user_id}");
            sqlx::query("INSERT INTO api_calls(id,request_id,thread_id,session_id,consumer_id,user_id,affinity_hash,affinity_source,method,path,status,latency_ms,created_at) VALUES(?,?,?,?,'history-key','root-user',?,'session-id','POST','/v1/responses',200,1,?)")
                .bind(&previous_id)
                .bind(format!("previous-request-{user_id}"))
                .bind(&thread_id)
                .bind(&session_id)
                .bind(&affinity_hash)
                .bind(now - 1)
                .execute(&state.db)
                .await
                .unwrap();
            let audit =
                provider_request(&state, Method::GET, "/api/audit", &browser_token, None).await;
            assert_eq!(audit.status(), StatusCode::OK);
            let audits: Value =
                serde_json::from_slice(&audit.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(
                audits["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|audit| audit["id"] == call_id)
                    .and_then(|audit| audit["provider_name"].as_str()),
                Some("renamed")
            );
            assert_eq!(
                audits["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|audit| audit["id"] == call_id)
                    .and_then(|audit| audit["consumer_name"].as_str()),
                Some("history")
            );
            assert_eq!(
                audits["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|audit| audit["id"] == call_id)
                    .and_then(|audit| audit["session_id"].as_str()),
                Some(session_id.as_str())
            );
            assert_eq!(
                audits["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|audit| audit["id"] == call_id)
                    .and_then(|audit| audit["first_byte_latency_ms"].as_i64()),
                Some(125)
            );
            let audited_call = audits["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|audit| audit["id"] == call_id)
                .unwrap();
            assert_eq!(audited_call["upstream_model"], "upstream-test-model");
            assert_eq!(audited_call["downstream_user_agent"], "Test UA/1.0");
            assert_eq!(audited_call["request_bytes"], 256);
            assert_eq!(audited_call["response_bytes"], 512);
            assert_eq!(audited_call["codex_turn_state_length"], 12);
            assert_eq!(audited_call["reasoning_effort"], "high");
            assert_eq!(audited_call["fast_mode"], true);
            assert_eq!(audited_call["official_provided_usd_before_nanos"], 100);
            assert_eq!(audited_call["official_provided_usd_after_nanos"], 130);
            assert_eq!(audited_call["actual_provided_usd_before_nanos"], 10);
            assert_eq!(audited_call["actual_provided_usd_after_nanos"], 13);
            let filtered = provider_request(
                &state,
                Method::GET,
                &format!("/api/audit?model={audit_model}&provider=renamed&status=success&limit=1"),
                &browser_token,
                None,
            )
            .await;
            assert_eq!(filtered.status(), StatusCode::OK);
            let filtered: Value =
                serde_json::from_slice(&filtered.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(filtered["total"], 1);
            assert_eq!(filtered["rows"].as_array().unwrap().len(), 1);
            assert_eq!(filtered["rows"][0]["id"], call_id);
            let detail = provider_request(
                &state,
                Method::GET,
                &format!("/api/audit/{call_id}"),
                &browser_token,
                None,
            )
            .await;
            assert_eq!(detail.status(), StatusCode::OK);
            let detail: Value =
                serde_json::from_slice(&detail.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(detail["upstream_model"], "upstream-test-model");
            assert_eq!(detail["downstream_user_agent"], "Test UA/1.0");
            assert_eq!(detail["archive_available"], true);
            assert_eq!(
                detail["upstream_request_headers"],
                json!(r#"[["content-type","application/json"],["originator","codex_cli_rs"]]"#)
            );
            assert_eq!(
                detail["downstream_response_headers"],
                json!(r#"[["content-type","application/json"],["content-encoding","gzip"]]"#)
            );
            assert_eq!(detail["request_body"], json!(r#"{"input":"audit detail"}"#));
            assert_eq!(detail["response_body"], json!(r#"{"output":"diagnostic"}"#));
            assert_eq!(detail["consumer_name"], "history");
            assert_eq!(detail["thread_id"], thread_id);
            assert_eq!(detail["session_id"], session_id);
            assert_eq!(detail["first_byte_latency_ms"], 125);
            assert_eq!(detail["request_bytes"], 256);
            assert_eq!(detail["response_bytes"], 512);
            assert_eq!(detail["codex_turn_state_length"], 12);
            assert_eq!(detail["reasoning_effort"], "high");
            assert_eq!(detail["fast_mode"], true);
            assert_eq!(detail["upstream_http_version"], "HTTP/2");
            assert_eq!(detail["official_provided_usd_before_nanos"], 100);
            assert_eq!(detail["official_provided_usd_after_nanos"], 130);
            assert_eq!(detail["actual_provided_usd_before_nanos"], 10);
            assert_eq!(detail["actual_provided_usd_after_nanos"], 13);
            assert_eq!(detail["previous"]["id"], previous_id);
            let circuit_id = format!("circuit-{user_id}");
            sqlx::query("INSERT INTO provider_circuit_events(id,provider_id,cause,rate_limit_json,opened_at,cooldown_until) VALUES(?,?, 'upstream HTTP 429','{}',?,?)")
                .bind(&circuit_id)
                .bind(&provider_id)
                .bind(now)
                .bind(now + 60)
                .execute(&state.db)
                .await
                .unwrap();
            let circuit_summary = provider_request(
                &state,
                Method::GET,
                "/api/providers/circuit-events",
                &browser_token,
                None,
            )
            .await;
            assert_eq!(circuit_summary.status(), StatusCode::OK);
            let circuit_summary: Value = serde_json::from_slice(
                &circuit_summary
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes(),
            )
            .unwrap();
            assert_eq!(circuit_summary["providers"][&provider_id]["id"], circuit_id);
            let circuit_history = provider_request(
                &state,
                Method::GET,
                &format!("/api/providers/{provider_id}/circuit-events"),
                &browser_token,
                None,
            )
            .await;
            assert_eq!(circuit_history.status(), StatusCode::OK);
            let circuit_history: Value = serde_json::from_slice(
                &circuit_history
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes(),
            )
            .unwrap();
            assert_eq!(circuit_history[0]["id"], circuit_id);
            let test = provider_request(
                &state,
                Method::POST,
                &format!("/api/providers/{provider_id}/test"),
                &browser_token,
                None,
            )
            .await;
            assert_eq!(test.status(), StatusCode::OK);
            let usage_list = provider_request(
                &state,
                Method::GET,
                "/api/providers/usage",
                &browser_token,
                None,
            )
            .await;
            assert_eq!(usage_list.status(), StatusCode::OK);
            let usage_body = usage_list.into_body().collect().await.unwrap().to_bytes();
            let usage_value: Value = serde_json::from_slice(&usage_body).unwrap();
            assert_eq!(
                usage_value.pointer(&format!("/providers/{provider_id}/usage/plan_type")),
                Some(&json!("team"))
            );
            assert_eq!(
                usage_value.pointer(&format!("/providers/{provider_id}/usage/email")),
                Some(&json!("ops@example.com"))
            );
            let resets = provider_request(
                &state,
                Method::GET,
                "/api/providers/rate-limit-resets",
                &browser_token,
                None,
            )
            .await;
            assert_eq!(resets.status(), StatusCode::OK);
            let resets: Value =
                serde_json::from_slice(&resets.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(
                resets.pointer(&format!("/providers/{provider_id}/resets/available_count")),
                Some(&json!(1))
            );
            let consumed = provider_request(
                &state,
                Method::POST,
                &format!("/api/providers/{provider_id}/rate-limit-resets/consume"),
                &browser_token,
                Some(json!({"credit_id":"credit-1","redeem_request_id":Uuid::new_v4()})),
            )
            .await;
            assert_eq!(consumed.status(), StatusCode::OK);
            let consumed: Value =
                serde_json::from_slice(&consumed.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(consumed["code"], "reset");
            let delete = provider_request(
                &state,
                Method::DELETE,
                &format!("/api/providers/{provider_id}"),
                &browser_token,
                None,
            )
            .await;
            assert_eq!(delete.status(), StatusCode::OK);
            let historical_provider: Option<String> =
                sqlx::query_scalar("SELECT provider_id FROM api_calls WHERE id=?")
                    .bind(call_id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(historical_provider, Some(provider_id.clone()));
            let is_deleted: i64 = sqlx::query_scalar("SELECT is_deleted FROM providers WHERE id=?")
                .bind(&provider_id)
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(is_deleted, 1);
        }

        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at,owner_id) VALUES('tenant-provider','tenant','tenant','access','refresh',?,?, 'tenant-user')")
            .bind(now).bind(now).execute(&state.db).await.unwrap();
        let tenant_token = setup_token(&signing, &issuer, "tenant-user");
        let tenant_vacuum = provider_request(
            &state,
            Method::POST,
            "/api/system/resources",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(tenant_vacuum.status(), StatusCode::FORBIDDEN);
        let tenant_providers =
            provider_request(&state, Method::GET, "/api/providers", &tenant_token, None).await;
        assert_eq!(tenant_providers.status(), StatusCode::OK);
        let tenant_providers: Value = serde_json::from_slice(
            &tenant_providers
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(tenant_providers[0]["owner_id"], "tenant-user");
        assert!(tenant_providers[0].get("owner_profile").is_none());
        let created = provider_request(
            &state,
            Method::POST,
            "/api/providers",
            &tenant_token,
            Some(json!({
                "name":"tenant-created",
                "access_key":provider_oauth_access_token("tenant-created"),
                "refresh_key":"refresh"
            })),
        )
        .await;
        assert_eq!(created.status(), StatusCode::OK);
        let created: Value =
            serde_json::from_slice(&created.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let created_id = created["id"].as_str().unwrap();
        let created_owner: String = sqlx::query_scalar("SELECT owner_id FROM providers WHERE id=?")
            .bind(created_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(created_owner, "tenant-user");
        let delete_created = provider_request(
            &state,
            Method::DELETE,
            &format!("/api/providers/{created_id}"),
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(delete_created.status(), StatusCode::OK);
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('tenant-audit-key','tenant-user','tenant audit','sk-tenant-audit','tenant-audit-hash',?)")
            .bind(now).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,created_at) VALUES('tenant-audit-call','tenant-audit-request','tenant-audit-key','tenant-user','POST','/v1/responses',200,1,?)")
            .bind(now).execute(&state.db).await.unwrap();
        let tenant_detail = provider_request(
            &state,
            Method::GET,
            "/api/audit/tenant-audit-call",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(tenant_detail.status(), StatusCode::OK);
        let tenant_audit =
            provider_request(&state, Method::GET, "/api/audit", &tenant_token, None).await;
        assert_eq!(tenant_audit.status(), StatusCode::OK);
        let tenant_audit: Value =
            serde_json::from_slice(&tenant_audit.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert!(tenant_audit["rows"][0].get("user_profile").is_none());
        let tenant_usage =
            provider_request(&state, Method::GET, "/api/usage", &tenant_token, None).await;
        assert_eq!(tenant_usage.status(), StatusCode::OK);
        let tenant_usage: Value =
            serde_json::from_slice(&tenant_usage.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert!(
            tenant_usage["rows"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row.get("user_profile").is_none())
        );
        let foreign_detail = provider_request(
            &state,
            Method::GET,
            "/api/audit/call-root-user",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(foreign_detail.status(), StatusCode::NOT_FOUND);
        for (method, path, body) in [
            (Method::GET, "/api/providers/usage", None),
            (Method::POST, "/api/providers/tenant-provider/test", None),
            (Method::GET, "/api/providers/rate-limit-resets", None),
        ] {
            let response = provider_request(&state, method, path, &tenant_token, body).await;
            assert_eq!(response.status(), StatusCode::OK);
        }
        let tenant_token_read = provider_request(
            &state,
            Method::GET,
            "/api/providers/tenant-provider",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(tenant_token_read.status(), StatusCode::FORBIDDEN);
        let tenant_token_update = provider_request(
            &state,
            Method::PUT,
            "/api/providers/tenant-provider",
            &tenant_token,
            Some(
                json!({"access_key":provider_oauth_access_token("tenant"),"refresh_key":"refresh"}),
            ),
        )
        .await;
        assert_eq!(tenant_token_update.status(), StatusCode::FORBIDDEN);
        let tenant_delete = provider_request(
            &state,
            Method::DELETE,
            "/api/providers/tenant-provider",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(tenant_delete.status(), StatusCode::OK);

        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at,owner_id) VALUES('foreign-provider','foreign','foreign','access','refresh',?,?, 'root-user')")
            .bind(now).bind(now).execute(&state.db).await.unwrap();
        let foreign_provider = provider_request(
            &state,
            Method::GET,
            "/api/providers/foreign-provider",
            &tenant_token,
            None,
        )
        .await;
        assert_eq!(foreign_provider.status(), StatusCode::NOT_FOUND);
        let tenant_providers =
            provider_request(&state, Method::GET, "/api/providers", &tenant_token, None).await;
        assert_eq!(tenant_providers.status(), StatusCode::OK);
        let tenant_providers: Value = serde_json::from_slice(
            &tenant_providers
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(tenant_providers, json!([]));

        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at) VALUES('denied-provider','denied','denied','denied','refresh',?,?)")
            .bind(now).bind(now).execute(&state.db).await.unwrap();
        let admin_token = setup_token(&signing, &issuer, "admin-user");
        let denied = provider_request(
            &state,
            Method::POST,
            "/api/providers/denied-provider/test",
            &admin_token,
            None,
        )
        .await;
        assert_eq!(denied.status(), StatusCode::BAD_GATEWAY);

        for (action, expected) in [
            ("provider.tokens.read", 2_i64),
            ("provider.tokens.update", 2),
            ("provider.test", 3),
            ("provider.delete", 4),
            ("provider.create", 1),
            ("provider.test.failed", 1),
            ("provider.rate_limit_reset.consume", 2),
        ] {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit WHERE action=?")
                .bind(action)
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, expected, "unexpected audit count for {action}");
        }
    }

    #[tokio::test]
    async fn consumer_credential_verification_is_exact_read_only_and_owner_scoped() {
        let signing = SigningKey::from_bytes(&[38_u8; 32]);
        let x = URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes());
        let jwks = Router::new().route("/jwks", get(move || {
            let x = x.clone();
            async move {
                Json(json!({"keys":[{"kid":"setup","kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","x":x}]}))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, jwks).await.unwrap() });
        let state = crate::test_state("http://token.invalid").await;
        state
            .auth
            .configure(issuer.clone(), TEST_AUTH_AUDIENCE.to_owned())
            .await
            .unwrap();
        for owner in ["verify-owner", "other-owner"] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,'user',0)")
                .bind(owner)
                .execute(&state.db)
                .await
                .unwrap();
        }
        for (id, owner, secret, disabled, deleted, system) in [
            ("owned", "verify-owner", "sk-sameprefix-original", 0, 0, 0),
            ("disabled", "verify-owner", "sk-disabled", 1, 0, 0),
            ("deleted", "verify-owner", "sk-deleted", 0, 1, 0),
            ("system", "verify-owner", "sk-system", 0, 0, 1),
            ("foreign", "other-owner", "sk-foreign", 0, 0, 0),
        ] {
            sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,is_disabled,is_deleted,is_system,request_archive,created_at) VALUES(?,?,'Test','sk-sameprefi',?,?,?,?,1,0)")
                .bind(id).bind(owner).bind(consumer_secret_hash(secret))
                .bind(disabled).bind(deleted).bind(system)
                .execute(&state.db).await.unwrap();
        }
        let token = setup_token(&signing, &issuer, "verify-owner");
        for (id, secret, matches, disabled) in [
            ("owned", "sk-sameprefix-original", true, false),
            ("owned", "sk-sameprefix-different", false, false),
            ("owned", "sk-foreign", false, false),
            ("owned", "", false, false),
            ("disabled", "sk-disabled", true, true),
        ] {
            let response = provider_request(
                &state,
                Method::POST,
                &format!("/api/consumers/{id}/verify"),
                &token,
                Some(json!({"secret":secret})),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let body: Value =
                serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(
                body,
                json!({"id":id,"credential_matches":matches,"is_disabled":disabled,"request_archive":true})
            );
        }
        let unchanged: (String, i64) =
            sqlx::query_as("SELECT secret_hash,is_disabled FROM consumers WHERE id='owned'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(
            unchanged,
            (consumer_secret_hash("sk-sameprefix-original"), 0)
        );
        assert_eq!(
            provider_request(&state, Method::GET, "/v1/models", "sk-disabled", None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        for id in ["foreign", "deleted", "system", "missing"] {
            let response = provider_request(
                &state,
                Method::POST,
                &format!("/api/consumers/{id}/verify"),
                &token,
                Some(json!({"secret":"sk-foreign"})),
            )
            .await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        assert_eq!(
            provider_request(
                &state,
                Method::POST,
                "/api/consumers/owned/verify",
                "sk-sameprefix-original",
                Some(json!({"secret":"sk-sameprefix-original"}))
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
        let rotated = provider_request(
            &state,
            Method::POST,
            "/api/consumers/owned/rotate",
            &token,
            None,
        )
        .await;
        let rotated: Value =
            serde_json::from_slice(&rotated.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        for (secret, matches) in [
            ("sk-sameprefix-original", false),
            (rotated["secret"].as_str().unwrap(), true),
        ] {
            let response = provider_request(
                &state,
                Method::POST,
                "/api/consumers/owned/verify",
                &token,
                Some(json!({"secret":secret})),
            )
            .await;
            let body: Value =
                serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(body["credential_matches"], matches);
        }
        assert_eq!(
            provider_request(&state, Method::DELETE, "/api/consumers/owned", &token, None)
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            provider_request(
                &state,
                Method::POST,
                "/api/consumers/owned/verify",
                &token,
                Some(json!({"secret":rotated["secret"]}))
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
        server.abort();
    }

    #[tokio::test]
    async fn consumer_name_update_is_scoped_and_validated() {
        let signing = SigningKey::from_bytes(&[37_u8; 32]);
        let x = URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes());
        let jwks = Router::new().route("/jwks", get(move || {
            let x = x.clone();
            async move {
                Json(json!({
                    "keys":[{"kid":"setup","kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","x":x}]
                }))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, jwks).await.unwrap() });
        let issuer = format!("http://{address}");
        let state = crate::test_state("http://token.invalid").await;
        state
            .auth
            .configure(issuer.clone(), TEST_AUTH_AUDIENCE.to_owned())
            .await
            .unwrap();
        let now = chrono::Utc::now().timestamp();
        for user_id in ["consumer-owner", "other-owner"] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,'user',?)")
                .bind(user_id)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('owned-consumer','consumer-owner','Original','sk-owned','hash-owned',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('foreign-consumer','other-owner','Foreign','sk-foreign','hash-foreign',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();

        let token = setup_token(&signing, &issuer, "consumer-owner");
        let updated = provider_request(
            &state,
            Method::PATCH,
            "/api/consumers/owned-consumer",
            &token,
            Some(json!({"name":"Renamed app"})),
        )
        .await;
        assert_eq!(updated.status(), StatusCode::OK);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT name FROM consumers WHERE id='owned-consumer'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            "Renamed app"
        );

        let archive = provider_request(
            &state,
            Method::PATCH,
            "/api/consumers/owned-consumer",
            &token,
            Some(json!({"request_archive":true})),
        )
        .await;
        assert_eq!(archive.status(), StatusCode::OK);
        let updated: (String, i64) =
            sqlx::query_as("SELECT name,request_archive FROM consumers WHERE id='owned-consumer'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(updated, ("Renamed app".to_owned(), 1));

        let intercept = provider_request(
            &state,
            Method::PATCH,
            "/api/consumers/owned-consumer",
            &token,
            Some(json!({"intercept_degradation":true})),
        )
        .await;
        assert_eq!(intercept.status(), StatusCode::OK);
        let updated: (String, i64, i64) = sqlx::query_as(
            "SELECT name,request_archive,intercept_degradation FROM consumers WHERE id='owned-consumer'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(updated, ("Renamed app".to_owned(), 1, 1));

        let invalid = provider_request(
            &state,
            Method::PATCH,
            "/api/consumers/owned-consumer",
            &token,
            Some(json!({"name":"   "})),
        )
        .await;
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

        let foreign = provider_request(
            &state,
            Method::PATCH,
            "/api/consumers/foreign-consumer",
            &token,
            Some(json!({"name":"Should not change"})),
        )
        .await;
        assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT name FROM consumers WHERE id='foreign-consumer'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "Foreign"
        );

        let secret_hash_before: String =
            sqlx::query_scalar("SELECT secret_hash FROM consumers WHERE id='owned-consumer'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let rotated = provider_request(
            &state,
            Method::POST,
            "/api/consumers/owned-consumer/rotate",
            &token,
            None,
        )
        .await;
        assert_eq!(rotated.status(), StatusCode::OK);
        let rotated: Value =
            serde_json::from_slice(&rotated.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let secret = rotated["secret"].as_str().unwrap();
        let rotated_consumer: (String, i64, String, String) = sqlx::query_as(
            "SELECT name,request_archive,prefix,secret_hash FROM consumers WHERE id='owned-consumer'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(rotated_consumer.0, "Renamed app");
        assert_eq!(rotated_consumer.1, 1);
        assert_eq!(
            rotated_consumer.2,
            secret.chars().take(11).collect::<String>()
        );
        assert_ne!(rotated_consumer.3, secret_hash_before);
        let new_key = provider_request(&state, Method::GET, "/v1/models", secret, None).await;
        assert_eq!(new_key.status(), StatusCode::OK);

        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,created_at) VALUES('owned-call','owned-request','owned-consumer','consumer-owner','POST','/v1/responses',200,1,?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO request_archives(api_call_id,request_headers_json,request_body,request_body_truncated,response_body_truncated,created_at) VALUES('owned-call','{}',X'7B7D',0,0,?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();

        let foreign_delete = provider_request(
            &state,
            Method::DELETE,
            "/api/consumers/foreign-consumer",
            &token,
            None,
        )
        .await;
        assert_eq!(foreign_delete.status(), StatusCode::NOT_FOUND);

        let deleted = provider_request(
            &state,
            Method::DELETE,
            "/api/consumers/owned-consumer",
            &token,
            None,
        )
        .await;
        assert_eq!(deleted.status(), StatusCode::OK);
        let is_deleted: i64 =
            sqlx::query_scalar("SELECT is_deleted FROM consumers WHERE id='owned-consumer'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(is_deleted, 1);
        for (table, predicate) in [
            ("consumers", "id='owned-consumer'"),
            ("api_calls", "id='owned-call'"),
            ("request_archives", "api_call_id='owned-call'"),
        ] {
            let count: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {predicate}"))
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(count, 1, "soft-deleted consumer lost data from {table}");
        }
        let listed = provider_request(&state, Method::GET, "/api/consumers", &token, None).await;
        assert_eq!(listed.status(), StatusCode::OK);
        let listed: Value =
            serde_json::from_slice(&listed.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(listed, json!([]));
        let deleted_key = provider_request(&state, Method::GET, "/v1/models", secret, None).await;
        assert_eq!(deleted_key.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn provider_usage_maps_invalid_json_to_bad_gateway() {
        let invalid = Router::new().route(
            "/backend-api/wham/usage",
            get(|| async { (StatusCode::OK, "not-json") }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, invalid).await.unwrap() });
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            &format!("http://{address}/backend-api/codex"),
        )
        .await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at) VALUES('invalid','invalid','account','access','refresh',?,?)")
            .bind(now).bind(now).execute(&state.db).await.unwrap();
        let error = provider_usage(&state, "invalid").await.unwrap_err();
        assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(error.message(), "provider Usage API returned invalid JSON");
    }

    #[tokio::test]
    async fn provider_usage_maps_network_failure_to_bad_gateway() {
        let unavailable = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = unavailable.local_addr().unwrap();
        drop(unavailable);
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            &format!("http://{address}/backend-api/codex"),
        )
        .await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at) VALUES('offline','offline','account','access','refresh',?,?)")
            .bind(now).bind(now).execute(&state.db).await.unwrap();
        let error = provider_usage(&state, "offline").await.unwrap_err();
        assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(error.message(), "provider Usage API request failed");
    }

    #[tokio::test]
    async fn provider_fallback_defaults_off_and_owner_can_toggle_without_changing_identity() {
        let state = crate::test_state("http://token.invalid").await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','user',0)")
            .execute(&state.db)
            .await
            .unwrap();
        let mut owner = root_identity();
        owner.role = "user".to_owned();
        let created = create_provider(
            State(state.clone()), ConnectInfo("127.0.0.1:8080".parse().unwrap()), Extension(owner.clone()),
            Json(serde_json::from_value(json!({
                "access_key":provider_oauth_access_token("pi-account"),"refresh_key":"refresh","originator":"pi"
            })).unwrap()),
        ).await.unwrap();
        assert_eq!(created.0["allow_other_originator"], false);
        let id = created.0["id"].as_str().unwrap();
        let mut stranger = owner.clone();
        stranger.id = "stranger".to_owned();
        let rejected = update_provider(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(stranger),
            Path(id.to_owned()),
            Json(serde_json::from_value(json!({"allow_other_originator":true})).unwrap()),
        )
        .await
        .unwrap_err();
        assert_eq!(rejected.status(), StatusCode::NOT_FOUND);
        for allowed in [true, false] {
            let _ = update_provider(
                State(state.clone()),
                ConnectInfo("127.0.0.1:8080".parse().unwrap()),
                Extension(owner.clone()),
                Path(id.to_owned()),
                Json(serde_json::from_value(json!({"allow_other_originator":allowed})).unwrap()),
            )
            .await
            .unwrap();
            let stored: (String, bool) = sqlx::query_as(
                "SELECT originator,allow_other_originator FROM providers WHERE id=?",
            )
            .bind(id)
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(stored, ("pi".to_owned(), allowed));
            assert_eq!(
                state
                    .balancer
                    .select(&state, None, None, owner_downstream())
                    .await
                    .is_ok(),
                allowed
            );
        }
        let updates: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_audit WHERE action='provider.update' AND target_id=?",
        )
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(updates, 2);
    }

    #[tokio::test]
    async fn oauth_completion_can_opt_into_fallback_without_changing_the_authorized_originator() {
        let access = provider_oauth_access_token("pi-account");
        let upstream = Router::new().route(
            "/token",
            post(move || {
                let access = access.clone();
                async move {
                    Json(json!({"access_token":access,"refresh_token":"refresh","expires_in":3600}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let state = crate::test_state(&format!("http://{address}/token")).await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('owner','root',0)")
            .execute(&state.db)
            .await
            .unwrap();
        let flow = oauth::start(&state, "owner", "pi").await.unwrap();
        assert!(flow.authorize_url.contains("originator=pi"));
        let callback = format!(
            "{}?code=test-code&state={}",
            state.config.load().oauth_redirect_uri,
            flow.state
        );
        let created = oauth_complete(
            State(state.clone()),
            ConnectInfo("127.0.0.1:8080".parse().unwrap()),
            Extension(root_identity()),
            Json(
                serde_json::from_value(
                    json!({"callback_url":callback,"allow_other_originator":true}),
                )
                .unwrap(),
            ),
        )
        .await
        .unwrap();
        assert_eq!(created.0["originator"], "pi");
        assert_eq!(created.0["allow_other_originator"], true);
        let lease = state
            .balancer
            .select(&state, None, None, owner_downstream())
            .await
            .unwrap();
        assert_eq!(lease.provider.originator, "pi");
    }
}
