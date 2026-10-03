// The audit detail response is a single large JSON literal.
#![recursion_limit = "256"]

pub mod api;
pub mod audit;
pub mod auth;
pub mod balancer;
mod concurrency;
pub mod config;
pub mod crypto;
pub mod db;
pub mod midas;
pub mod payments;
pub mod pricing;
pub mod proxy;
pub mod resources;

use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use axum::{
    Router,
    body::{Body, BodyDataStream},
    extract::{Request, State},
    http::StatusCode,
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use futures_util::StreamExt;
use rust_embed::RustEmbed;
use serde_json::json;
use sqlx::SqlitePool;
use tower_http::{compression::CompressionLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

use crate::audit::AuditWriter;
use crate::{auth::AuthManager, balancer::Balancer, config::Config, resources::ResourceMonitor};

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
    pub auth: AuthManager,
    pub audit: AuditWriter,
    pub balancer: Balancer,
    pub(crate) resources: Arc<tokio::sync::Mutex<ResourceMonitor>>,
}

impl AppState {
    /// Starts the periodic provider-maintenance loop. Production calls this once at startup;
    /// tests drive [`crate::balancer::Balancer::maintain`] directly instead.
    pub fn start_background_tasks(&self) {
        self.balancer
            .start_maintenance(self.db.clone(), self.write_gate.clone());
    }

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
        Ok(Self {
            config,
            db,
            write_gate,
            client,
            auth,
            audit,
            balancer,
            resources,
        })
    }
}

pub fn router(state: AppState) -> Router {
    let config = state.config.load();
    let response_limit = config.response_body_limit;
    drop(config);
    let browser_api = Router::new()
        .route("/api/me", get(api::me))
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
        .route(
            "/api/providers/{id}",
            patch(api::update_provider).delete(api::delete_provider),
        )
        .route(
            "/api/providers/{id}/key",
            get(api::read_provider_key).put(api::replace_provider_key),
        )
        .route("/api/providers/{id}/test", post(api::test_provider))
        .route("/api/providers/{id}/balance", get(api::provider_balance))
        .route("/api/usage", get(api::usage))
        .route("/api/model-prices", get(api::model_prices))
        .route("/api/audit", get(api::audit))
        .route("/api/provider-audit", get(api::provider_audit))
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
        .route("/api/users", get(api::list_users))
        .route("/api/users/{id}", patch(api::update_user))
        .route_layer(from_fn_with_state(state.clone(), auth::authenticate));

    let consumer_api = Router::new()
        .route(
            "/v1/chat/completions",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route(
            "/v1/responses",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route(
            "/chat/completions",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route(
            "/responses",
            post(proxy::handle_json).layer(RequestBodyLimitLayer::new(response_limit)),
        )
        .route("/v1/models", get(proxy::handle_models))
        .route("/models", get(proxy::handle_models))
        .route(
            "/v1/requests/query",
            post(proxy::handle_requests_query)
                .layer(RequestBodyLimitLayer::new(proxy::REQUEST_QUERY_BODY_LIMIT)),
        )
        .route(
            "/requests/query",
            post(proxy::handle_requests_query)
                .layer(RequestBodyLimitLayer::new(proxy::REQUEST_QUERY_BODY_LIMIT)),
        )
        .route("/v1/requests/{id}", get(proxy::handle_request_record))
        .route("/requests/{id}", get(proxy::handle_request_record));

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

async fn record_response_transport(
    State(audit): State<AuditWriter>,
    request: Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    let (parts, body) = response.into_parts();
    let Some(id) = parts.extensions.get::<proxy::AuditTransportId>().cloned() else {
        return Response::from_parts(parts, body);
    };
    let mut parts = parts;
    parts.headers.insert(
        "x-deepseek-lb-request-id",
        axum::http::HeaderValue::from_str(&id.id).expect("audit IDs are valid header values"),
    );
    let encoding = parts
        .headers
        .get(axum::http::header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| Some("identity".to_owned()));
    let downstream_response_headers_json = id
        .diagnostics_enabled
        .then(|| proxy::archive_headers(&parts.headers));
    let mut transport = ResponseTransportGuard {
        stream: Some(body.into_data_stream()),
        audit,
        id: id.id,
        bytes: 0,
        encoding,
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
    pub fn not_found_with_reason(message: impl Into<String>, reason: &'static str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            reason: Some(reason),
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
pub(crate) async fn test_state(upstream_base: &str) -> AppState {
    let pool = db::connect_memory().await.unwrap();
    let config = Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        data_dir: std::env::temp_dir(),
        database_path: std::path::PathBuf::from(":memory:"),
        setup_complete: false,
        auth_issuer: None,
        auth_audience: None,
        upstream_base: upstream_base.to_owned(),
        available_model_ids: config::default_available_model_ids(),
        allow_all_users_debt: false,
        response_body_limit: 1024 * 1024,
        affinity_ttl_seconds: 3600,
        provider_concurrency_limit: 3,
        request_archive_retention_days: 7,
        model_price_multiplier_nanos: 1_000_000_000,
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
        let state = crate::test_state("http://upstream.invalid").await;
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
    async fn consumer_endpoints_require_a_consumer_credential() {
        let state = crate::test_state("http://upstream.invalid").await;
        let mut request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"model":"deepseek-flash","messages":[]}"#))
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                40_000,
            ))));
        let response = crate::router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
