pub mod api;
mod audio_spool;
pub mod audit;
pub mod auth;
pub mod balancer;
mod concurrency;
pub mod config;
pub mod crypto;
pub mod db;
pub mod identity;
pub mod midas;
pub mod oauth;
pub mod payments;
pub mod proxy;
pub mod resources;

use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use axum::{
    Router,
    body::{Body, BodyDataStream},
    extract::{Request, State},
    http::StatusCode,
    middleware::{Next, from_fn, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use dashmap::DashMap;
use futures_util::StreamExt;
use rust_embed::RustEmbed;
use serde_json::json;
use sqlx::SqlitePool;
use tower_http::{compression::CompressionLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

use crate::audit::AuditWriter;
use crate::{
    auth::{ApiIdentity, AuthManager},
    balancer::Balancer,
    config::Config,
    identity::DownstreamClient,
    resources::ResourceMonitor,
};

#[derive(Clone, Default)]
pub(crate) struct SqliteWriteGate(Arc<tokio::sync::Mutex<()>>);

impl SqliteWriteGate {
    pub(crate) async fn lock(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.0.clone().lock_owned().await
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<Config>>,
    pub db: SqlitePool,
    pub(crate) write_gate: SqliteWriteGate,
    pub client: reqwest::Client,
    pub proxy_clients: Arc<DashMap<String, (String, reqwest::Client)>>,
    pub auth: AuthManager,
    pub audit: AuditWriter,
    pub balancer: Balancer,
    pub(crate) resources: Arc<tokio::sync::Mutex<ResourceMonitor>>,
    pub refresh_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub(crate) upstream_cookies: Arc<DashMap<String, proxy::UpstreamCookie>>,
    pub(crate) realtime_console_sessions: Arc<DashMap<String, RealtimeConsoleSession>>,
    pub(crate) oauth_flows: Arc<DashMap<String, OAuthFlow>>,
}

#[derive(Clone)]
pub(crate) struct RealtimeConsoleSession {
    pub identity: ApiIdentity,
    pub call_id: String,
    pub upstream_session_id: String,
    pub expires_at: i64,
}

pub(crate) struct OAuthFlow {
    pub verifier: String,
    pub created_by: String,
    /// Provider originator selected when this authorization attempt started.
    pub originator: String,
    pub expires_at: i64,
}

impl AppState {
    pub async fn new(config: Config, db: SqlitePool) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(600))
            .build()?;
        let auth =
            AuthManager::new(config.auth_issuer.clone(), config.auth_audience.clone()).await?;
        let config = Arc::new(ArcSwap::from_pointee(config));
        let write_gate = SqliteWriteGate::default();
        let audit = AuditWriter::new(db.clone(), config.clone(), write_gate.clone());
        let resources = Arc::new(tokio::sync::Mutex::new(ResourceMonitor::new(
            config.load().database_path.clone(),
            db.clone(),
            write_gate.clone(),
        )));
        let balancer = Balancer::default();
        balancer.set_concurrency_limit(config.load().provider_concurrency_limit);
        balancer.hydrate(&db).await?;
        balancer.start_maintenance(db.clone(), write_gate.clone());
        #[cfg(not(test))]
        audio_spool::start_cleanup(config.load().data_dir.clone());
        Ok(Self {
            config,
            db,
            write_gate,
            client,
            proxy_clients: Arc::new(DashMap::new()),
            auth,
            audit,
            balancer,
            resources,
            refresh_locks: Arc::new(DashMap::new()),
            upstream_cookies: Arc::new(DashMap::new()),
            realtime_console_sessions: Arc::new(DashMap::new()),
            oauth_flows: Arc::new(DashMap::new()),
        })
    }
}

impl AppState {
    pub fn provider_client(
        &self,
        provider: &balancer::Provider,
    ) -> Result<reqwest::Client, AppError> {
        let Some(proxy_url) = provider.http_proxy_url.as_deref() else {
            return Ok(self.client.clone());
        };
        if let Some(client) = self.proxy_clients.get(&provider.id)
            && client.value().0 == proxy_url
        {
            return Ok(client.value().1.clone());
        }
        let proxy = reqwest::Proxy::all(proxy_url)
            .map_err(|_| AppError::bad_request("invalid provider HTTP proxy URL"))?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(600))
            .proxy(proxy)
            .build()?;
        self.proxy_clients
            .insert(provider.id.clone(), (proxy_url.to_owned(), client.clone()));
        Ok(client)
    }
}

pub fn router(state: AppState) -> Router {
    let config = state.config.load();
    let response_limit = config.response_body_limit;
    let image_limit = config.image_body_limit;
    let audio_limit = config.audio_body_limit;
    drop(config);
    let browser_api = Router::new()
        .route("/api/me", get(api::me))
        .route(
            "/api/transcriptions",
            post(proxy::handle_console_transcription)
                .layer(RequestBodyLimitLayer::new(audio_limit)),
        )
        .route(
            "/api/images/generations",
            post(proxy::handle_console_image_generation)
                .layer(RequestBodyLimitLayer::new(image_limit)),
        )
        .route(
            "/api/realtime/calls",
            post(proxy::handle_console_realtime_call)
                .layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route(
            "/api/consumers",
            get(api::list_consumers).post(api::create_consumer),
        )
        .route(
            "/api/consumers/{id}",
            patch(api::update_consumer).delete(api::delete_consumer),
        )
        .route("/api/consumers/{id}/rotate", post(api::rotate_consumer))
        .route(
            "/api/consumers/{id}/verify",
            post(api::verify_consumer_credential),
        )
        .route(
            "/api/providers",
            get(api::list_providers).post(api::create_provider),
        )
        .route("/api/providers/usage", get(api::list_provider_usage))
        .route("/api/provider-capacity", get(api::provider_capacity))
        .route(
            "/api/providers/circuit-events",
            get(api::list_provider_circuit_summaries),
        )
        .route(
            "/api/providers/rate-limit-resets",
            get(api::list_provider_rate_limit_resets),
        )
        .route(
            "/api/providers/{id}",
            get(api::read_provider_tokens)
                .put(api::replace_provider_tokens)
                .patch(api::update_provider)
                .delete(api::delete_provider),
        )
        .route(
            "/api/providers/{id}/circuit-events",
            get(api::list_provider_circuit_events),
        )
        .route(
            "/api/providers/{id}/proxy-health",
            post(api::provider_proxy_health),
        )
        .route("/api/providers/{id}/test", post(api::test_provider))
        .route(
            "/api/providers/{id}/rate-limit-resets/consume",
            post(api::consume_provider_rate_limit_reset),
        )
        .route("/api/oauth/start", post(api::oauth_start))
        .route("/api/oauth/complete", post(api::oauth_complete))
        .route("/api/usage", get(api::usage))
        .route("/api/model-prices", get(api::model_prices))
        .route("/api/audit", get(api::audit))
        .route("/api/provider-audit", get(api::provider_audit))
        .route(
            "/api/model-downgrade-audit",
            get(api::model_downgrade_audit),
        )
        .route("/api/audit/{id}", get(api::audit_detail))
        .route(
            "/api/audit/{id}/bodies",
            axum::routing::delete(api::delete_audit_bodies),
        )
        .route("/api/admin-audit", get(api::list_admin_audit))
        .route("/api/dashboard", get(api::dashboard))
        .route(
            "/api/system/resources",
            get(api::system_resources).post(api::vacuum_system_database),
        )
        .route(
            "/api/settings",
            get(api::settings).patch(api::update_settings),
        )
        .route(
            "/api/settings/available-models",
            patch(api::update_available_model_ids),
        )
        .route(
            "/api/settings/provider-concurrency",
            patch(api::update_provider_concurrency),
        )
        .route(
            "/api/settings/upstream-user-agent",
            patch(api::update_upstream_user_agent),
        )
        .route(
            "/api/settings/experimental-turn-state-312-filter",
            patch(api::update_experimental_turn_state_312_filter),
        )
        .route("/api/users", get(api::list_users))
        .route("/api/users/{id}", patch(api::update_user))
        .route_layer(from_fn_with_state(state.clone(), auth::authenticate));

    let consumer_api = Router::new()
        .route(
            "/v1/responses",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route(
            "/v1/responses/compact",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route(
            "/backend-api/codex/responses",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route(
            "/backend-api/codex/responses/compact",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route(
            "/v1/audio/transcriptions",
            post(proxy::handle_audio).layer(RequestBodyLimitLayer::new(audio_limit)),
        )
        .route(
            "/v1/images/generations",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(image_limit)),
        )
        .route(
            "/v1/realtime/calls",
            post(proxy::handle_realtime_call).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route("/v1/realtime", get(proxy::handle_realtime_websocket))
        .route("/v1/models", get(proxy::handle_models))
        .route_layer(from_fn(require_session_id));

    Router::new()
        .route("/api/health", get(api::health))
        .route("/api/config", get(api::public_config))
        .route("/api/setup", get(api::setup_status).post(api::setup))
        .merge(
            Router::new()
                .route("/api/payments/summary", get(api::payment_summary))
                .route(
                    "/api/payments/settings",
                    get(api::midas_settings).patch(api::update_midas_settings),
                )
                .route_layer(from_fn_with_state(state.clone(), auth::authenticate)),
        )
        .merge(browser_api)
        .merge(consumer_api)
        .route(
            "/api/realtime",
            get(proxy::handle_console_realtime_websocket),
        )
        .fallback(static_asset)
        .layer(axum::extract::DefaultBodyLimit::disable())
        .layer(CompressionLayer::new())
        .layer(from_fn_with_state(
            state.audit.clone(),
            record_response_transport,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn require_session_id(request: Request, next: Next) -> Response {
    // INVARIANT: Pi Agent session ids are validated by the proxy itself, inside the audited region,
    // so a request Pi would not have produced (one without a UUIDv7) is answered with a 400 that also
    // reaches the call audit. Every other family is only required to present the header at all.
    let pi_client = identity::identify_client(request.headers())
        .is_ok_and(|client| client == DownstreamClient::Pi);
    let path = request.uri().path().to_owned();
    if pi_client && identity::is_pi_responses_path(&path) {
        return next.run(request).await;
    }
    // Image generation is a one-off request, so its caller is never required to present a session
    // id either; the proxy mints a UUIDv7 for the upstream request instead.
    if path == "/v1/images/generations" {
        return next.run(request).await;
    }
    let has_session_id = request
        .headers()
        .get("session-id")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| !value.trim().is_empty());
    if !has_session_id {
        return AppError::bad_request("session-id request header is required").into_response();
    }
    next.run(request).await
}

async fn record_response_transport(
    State(audit): State<AuditWriter>,
    request: Request,
    next: Next,
) -> Response {
    let request_codex_turn_state_length = request
        .headers()
        .get("x-codex-turn-state")
        .map(|value| value.as_bytes().len() as i64);
    let response = next.run(request).await;
    let (parts, body) = response.into_parts();
    let Some(id) = parts.extensions.get::<proxy::AuditTransportId>().cloned() else {
        return Response::from_parts(parts, body);
    };
    let mut parts = parts;
    parts.headers.insert(
        "x-openai-lb-request-id",
        axum::http::HeaderValue::from_str(&id.id).expect("audit IDs are valid header values"),
    );
    let encoding = parts
        .headers
        .get(axum::http::header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| Some("identity".to_owned()));
    let codex_turn_state_length = parts
        .headers
        .get("x-codex-turn-state")
        .map(|value| value.as_bytes().len() as i64)
        .or(request_codex_turn_state_length);
    let downstream_response_headers_json = id
        .diagnostics_enabled
        .then(|| proxy::archive_headers(&parts.headers));
    let mut transport = ResponseTransportGuard {
        stream: Some(body.into_data_stream()),
        audit,
        id: id.id,
        bytes: 0,
        encoding,
        codex_turn_state_length,
        headers: downstream_response_headers_json,
    };
    let output = async_stream::stream! {
        while let Some(item) = transport.stream.as_mut().expect("stream exists until drop").next().await {
            if let Ok(chunk) = &item {
                transport.bytes += chunk.len() as i64;
            }
            yield item;
        }
    };
    Response::from_parts(parts, Body::from_stream(output))
}

struct ResponseTransportGuard {
    stream: Option<BodyDataStream>,
    audit: AuditWriter,
    id: String,
    bytes: i64,
    encoding: Option<String>,
    codex_turn_state_length: Option<i64>,
    headers: Option<String>,
}

impl Drop for ResponseTransportGuard {
    fn drop(&mut self) {
        // INVARIANT: Dropping the inner stream first finalizes its base audit row.
        // Queue transport diagnostics afterward, even if the client cancelled or
        // never polled the body; an UPDATE before the base INSERT would be lost.
        drop(self.stream.take());
        self.audit.record_response_transport(
            std::mem::take(&mut self.id),
            self.bytes,
            self.encoding.take(),
            self.codex_turn_state_length,
            self.headers.take(),
        );
    }
}

include!(concat!(env!("OUT_DIR"), "/embedded_assets_fingerprint.rs"));

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct Assets;

async fn static_asset(uri: axum::http::Uri) -> Response {
    if ["/api", "/v1", "/backend-api"]
        .iter()
        .any(|prefix| uri.path() == *prefix || uri.path().starts_with(&format!("{prefix}/")))
    {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(
                json!({"error":{"message":"API route not found","type":"not_found","code":404}}),
            ),
        )
            .into_response();
    }
    let requested = uri.path().trim_start_matches('/');
    let path = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };
    let requested_asset = Assets::get(path);
    let is_fallback = requested_asset.is_none();
    match requested_asset.or_else(|| Assets::get("index.html")) {
        Some(asset) => {
            let mime = if is_fallback {
                "text/html; charset=utf-8"
            } else {
                match path.rsplit('.').next() {
                    Some("js") => "text/javascript",
                    Some("css") => "text/css",
                    Some("svg") => "image/svg+xml",
                    Some("woff2") => "font/woff2",
                    _ => "text/html; charset=utf-8",
                }
            };
            ([(axum::http::header::CONTENT_TYPE, mime)], asset.data).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    status: StatusCode,
    message: String,
    reason: Option<&'static str>,
    audit_transport: Option<proxy::AuditTransportId>,
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            reason: None,
            audit_transport: None,
        }
    }
    pub fn bad_request_with_reason(message: impl Into<String>, reason: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            reason: Some(reason),
            audit_transport: None,
        }
    }
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
            reason: None,
            audit_transport: None,
        }
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            reason: None,
            audit_transport: None,
        }
    }
    pub fn payment_required(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYMENT_REQUIRED,
            message: message.into(),
            reason: None,
            audit_transport: None,
        }
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            reason: None,
            audit_transport: None,
        }
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
            reason: None,
            audit_transport: None,
        }
    }
    pub fn unavailable_with_reason(message: impl Into<String>, reason: &'static str) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
            reason: Some(reason),
            audit_transport: None,
        }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            reason: None,
            audit_transport: None,
        }
    }
    pub fn upstream(status: u16, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            message: message.into(),
            reason: None,
            audit_transport: None,
        }
    }
    pub fn upstream_with_reason(
        status: u16,
        reason: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            message: message.into(),
            reason: Some(reason),
            audit_transport: None,
        }
    }
    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
    pub(crate) fn reason(&self) -> Option<&str> {
        self.reason
    }
    pub(crate) fn with_audit_transport(mut self, audit_transport: proxy::AuditTransportId) -> Self {
        self.audit_transport = Some(audit_transport);
        self
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            axum::Json(json!({"error":{
                "message":self.message,
                "type":"proxy_error",
                "code":self.status.as_u16(),
                "reason":self.reason,
            }})),
        )
            .into_response();
        if let Some(audit_transport) = self.audit_transport {
            response.extensions_mut().insert(audit_transport);
        }
        response
    }
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(%error, "request failed");
        Self::internal("internal server error")
    }
}

macro_rules! internal_from {
    ($($kind:ty),+ $(,)?) => {$(
        impl From<$kind> for AppError {
            fn from(error: $kind) -> Self {
                tracing::error!(%error, "request failed");
                Self::internal("internal server error")
            }
        }
    )+};
}

internal_from!(
    std::io::Error,
    sqlx::Error,
    reqwest::Error,
    serde_json::Error,
    url::ParseError
);

#[cfg(test)]
pub(crate) async fn test_state(oauth_token_url: &str) -> AppState {
    test_state_with_upstream(oauth_token_url, "http://upstream.invalid").await
}

/// Downstream identity that stands in for the first seeded test user.
#[cfg(test)]
pub(crate) fn test_downstream() -> balancer::Downstream<'static> {
    balancer::Downstream::new("user-1", identity::CODEX_ORIGINATOR)
}

/// Downstream identity owned by the default test provider owner (`owner`).
#[cfg(test)]
pub(crate) fn owner_downstream() -> balancer::Downstream<'static> {
    balancer::Downstream::new("owner", identity::CODEX_ORIGINATOR)
}

#[cfg(test)]
pub(crate) async fn test_state_with_upstream(
    oauth_token_url: &str,
    upstream_base: &str,
) -> AppState {
    let pool = db::connect_memory().await.unwrap();
    let config = Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        data_dir: std::env::temp_dir(),
        database_path: std::path::PathBuf::from(":memory:"),
        setup_complete: false,
        auth_issuer: None,
        auth_audience: None,
        upstream_base: upstream_base.to_owned(),
        upstream_openai_beta: None,
        upstream_user_agent: None,
        upstream_user_agents: config::UpstreamUserAgents::default(),
        experimental_filter_codex_turn_state_312: false,
        image_host_model: "gpt-5.4".to_owned(),
        available_model_ids: config::default_available_model_ids(),
        allow_all_users_debt: false,
        oauth_authorize_url: "http://auth.invalid/oauth/authorize".to_owned(),
        oauth_token_url: oauth_token_url.to_owned(),
        oauth_redirect_uri: "http://localhost:1455/auth/callback".to_owned(),
        oauth_client_id: "test-client".to_owned(),
        response_body_limit: 1024 * 1024,
        image_body_limit: 16 * 1024 * 1024,
        audio_body_limit: 1024 * 1024,
        affinity_ttl_seconds: 3600,
        provider_concurrency_limit: 3,
        request_archive_retention_days: 7,
        model_price_multiplier_nanos: 100_000_000,
        midas_api_base: "http://midas.invalid/api".to_owned(),
        midas_fund_user_id: None,
        midas_fund_api_key: None,
    };
    AppState::new(config, pool).await.unwrap()
}

#[cfg(test)]
mod routing_tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn unknown_api_namespace_returns_json_not_spa() {
        let state = crate::test_state("http://token.invalid").await;
        for path in ["/api/missing", "/v1/missing", "/backend-api/missing"] {
            let response = crate::router(state.clone())
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(
                response.headers().get("content-type").unwrap(),
                "application/json"
            );
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert!(
                std::str::from_utf8(&body)
                    .unwrap()
                    .contains("API route not found")
            );
        }
    }

    #[tokio::test]
    async fn consumer_api_requests_require_session_id_header() {
        let state = crate::test_state("http://token.invalid").await;
        let response = crate::router(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["error"]["message"],
            "session-id request header is required"
        );
    }
}
