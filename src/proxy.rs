use std::{borrow::Cow, convert::Infallible, net::SocketAddr, sync::Once, time::Instant};

use axum::{
    body::{Body, Bytes},
    extract::{
        ConnectInfo, Extension, Multipart, OriginalUri, State,
        ws::{Message as WebSocketMessage, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version, header},
    response::{IntoResponse, Response},
};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::OwnedSemaphorePermit,
};
use tokio_tungstenite::{
    client_async_tls_with_config, connect_async,
    tungstenite::{Message as TungsteniteMessage, client::IntoClientRequest},
};
use uuid::Uuid;

use crate::{
    AppError, AppState, RealtimeConsoleSession,
    audio_spool::AudioSpool,
    audit::{ARCHIVE_BODY_LIMIT, AuditEvent, AuditReservation, IMAGE_ARCHIVE_BODY_LIMIT},
    auth::{ApiIdentity, UserIdentity, api_identity},
    balancer::{Downstream, Lease, Provider, affinity_hash, track_response},
    identity::{self, CODEX_ORIGINATOR, ORIGINATOR_HEADER},
    midas, oauth, payments,
};

const HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
    "cookie",
    "proxy-connection",
    "set-cookie",
];

const PROXY_ONLY_HEADERS: &[&str] = &[
    "x-lb-affinity-key",
    "x-session-id",
    "session_id",
    "x-codex-session-id",
    "x-codex-conversation-id",
];

/// Byte length of the `x-codex-turn-state` header the console flags as a possible
/// model downgrade ("可能发生模型降级"): exactly 312. The experimental request-header
/// filter in `send_upstream` drops the same length before forwarding.
const DEGRADED_TURN_STATE_LENGTH: usize = 312;
/// Error code audited when a Consumer opted into degradation interception and the
/// upstream response carried the degradation signal.
const DEGRADATION_INTERCEPTED_CODE: &str = "degradation_intercepted";
/// Body message returned to a caller whose degraded response was intercepted.
const DEGRADATION_INTERCEPTED_MESSAGE: &str = "降级已拦截（x-codex-turn-state=312）";

// COMPATIBILITY: ChatGPT's desktop-only transcription endpoint requires a desktop User-Agent.
// The originator still follows the selected provider identity so every upstream request obeys the
// same provider header policy.
const TRANSCRIPTION_USER_AGENT: &str = "Codex Desktop/26.519.81530 (macos; aarch64)";

/// Request headers OpenAI-LB forwards to the upstream from a Pi Agent request.
///
/// Pi sends exactly this set on top of the identity headers the proxy owns; every other header a
/// caller supplies is dropped. `accept-encoding` drives upstream response compression, and
/// `content-encoding` is not forwarded because the proxy re-encodes the body it forwards itself.
const PI_PASSTHROUGH_HEADERS: &[&str] = &["accept-encoding"];

/// `openai-beta` value Pi Agent sends on the Codex responses endpoint; operators can override it
/// through the upstream `openai-beta` setting.
const PI_OPENAI_BETA: &str = "responses=experimental";

/// Request body encoding Pi Agent uses on the Codex responses endpoint, and the only one OpenAI-LB
/// accepts from a caller.
const PI_CONTENT_ENCODING: &str = "zstd";

/// Compression level Pi applies to its request bodies, reused for the bodies the proxy re-encodes.
const PI_BODY_COMPRESSION_LEVEL: i32 = 3;

/// Ceiling for a decompressed request body, so a small upload cannot expand into unbounded work.
const MAX_DECODED_REQUEST_BYTES: u64 = 64 * 1024 * 1024;

const MAX_REFERENCE_IMAGES: usize = 4;
const REALTIME_API_BASE_URL: &str = "https://api.openai.com/v1";
const REALTIME_MULTIPART_BOUNDARY: &str = "openai-lb-realtime-call-boundary";
static REALTIME_TLS_PROVIDER: Once = Once::new();

struct CallContext {
    model: Option<String>,
    stream: bool,
    cookie_key: Option<String>,
}

#[derive(Clone)]
pub(crate) struct UpstreamCookie {
    cookie: String,
    expires_at: i64,
}

struct RequestAuditContext {
    request_id: String,
    thread_id: Option<String>,
    session_id: Option<String>,
    method: Method,
    client_ip: String,
}

struct ResponsesRequest {
    body: Bytes,
    model: Option<String>,
    stream: bool,
    image: bool,
}

struct RealtimeCallRequest {
    sdp: String,
    session: Value,
}

struct RealtimeCallResponse {
    sdp: Bytes,
    call_id: String,
    audit_transport: Option<AuditTransportId>,
}

struct AuditStart<'a> {
    audit_id: &'a str,
    thread_id: Option<&'a str>,
    session_id: Option<&'a str>,
    method: &'a Method,
    path: &'a str,
    client_ip: &'a str,
    response_body_limit: usize,
}

struct BufferedUpstream {
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    body: Bytes,
}

enum UpstreamResponse {
    Live(reqwest::Response),
    Buffered(BufferedUpstream),
}

impl UpstreamResponse {
    fn status(&self) -> StatusCode {
        match self {
            Self::Live(response) => response.status(),
            Self::Buffered(response) => response.status,
        }
    }

    fn version(&self) -> Version {
        match self {
            Self::Live(response) => response.version(),
            Self::Buffered(response) => response.version,
        }
    }

    fn headers(&self) -> &HeaderMap {
        match self {
            Self::Live(response) => response.headers(),
            Self::Buffered(response) => &response.headers,
        }
    }

    async fn into_bytes(self) -> Result<Bytes, AppError> {
        match self {
            Self::Live(response) => Ok(response.bytes().await?),
            Self::Buffered(response) => Ok(response.body),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AuditTransportId {
    pub id: String,
    pub diagnostics_enabled: bool,
}

struct AuditTracker {
    permit: Option<AuditReservation>,
    archive_budget: Option<OwnedSemaphorePermit>,
    event: Option<AuditEvent>,
    response_body_limit: usize,
    started: Instant,
}

impl AuditTracker {
    fn begin(state: &AppState, identity: &ApiIdentity, start: AuditStart<'_>) -> Self {
        let permit = state.audit.try_reserve();
        let archive_budget = if permit.is_some() && identity.request_archive {
            state.audit.try_reserve_archive(start.response_body_limit)
        } else {
            None
        };
        let event = permit.as_ref().map(|_| AuditEvent {
            id: Uuid::new_v4().to_string(),
            request_id: start.audit_id.to_owned(),
            thread_id: start.thread_id.map(str::to_owned),
            session_id: start.session_id.map(str::to_owned),
            consumer_id: identity.consumer_id.clone(),
            user_id: identity.user_id.clone(),
            request_archive: archive_budget.is_some(),
            provider_id: None,
            affinity_hash: None,
            affinity_source: None,
            method: start.method.as_str().to_owned(),
            path: start.path.to_owned(),
            model: None,
            downstream_user_agent: None,
            downstream_originator: None,
            upstream_originator: None,
            originator_fallback_reason: None,
            upstream_model: None,
            reasoning_effort: None,
            fast_mode: false,
            status: 0,
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
            latency_ms: 0,
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
            official_cost_usd_nanos: 0,
            actual_cost_usd_nanos: 0,
            price_multiplier_nanos: state.config.load().model_price_multiplier_nanos,
            error: None,
            error_code: None,
            client_ip: start.client_ip.to_owned(),
            created_at: chrono::Utc::now().timestamp(),
            request_headers_json: "[]".to_owned(),
            request_body: Vec::new(),
            request_body_truncated: false,
            response_headers_json: None,
            upstream_request_headers_json: None,
            response_body: None,
            response_body_truncated: false,
        });
        Self {
            permit,
            archive_budget,
            event,
            response_body_limit: start.response_body_limit,
            started: Instant::now(),
        }
    }

    fn set_provider(&mut self, lease: &Lease) {
        if let Some(event) = &mut self.event {
            event.provider_id = Some(lease.provider.id.clone());
            event.upstream_originator = Some(lease.provider.originator.clone());
            event.originator_fallback_reason = lease.originator_fallback_reason.map(str::to_owned);
        }
    }

    fn set_affinity(&mut self, key: Option<&str>, source: Option<&str>) {
        if let Some(event) = &mut self.event {
            event.affinity_hash = key.map(affinity_hash);
            event.affinity_source = source.map(str::to_owned);
        }
    }

    fn set_reasoning_effort(&mut self, effort: Option<&str>) {
        if let Some(event) = &mut self.event {
            event.reasoning_effort = effort.map(str::to_owned);
        }
    }

    fn set_fast_mode(&mut self, enabled: bool) {
        if let Some(event) = &mut self.event {
            event.fast_mode = enabled;
        }
    }

    fn set_request(&mut self, headers: &HeaderMap, body: &[u8], truncated: bool) {
        if let Some(event) = &mut self.event {
            event.downstream_user_agent = header_value(headers, header::USER_AGENT);
            event.downstream_originator = header_value(headers, ORIGINATOR_HEADER);
            if !event.request_archive {
                return;
            }
            event.request_headers_json = archive_headers(headers);
            let (body, limit_truncated) = body_preview(body, ARCHIVE_BODY_LIMIT);
            event.request_body = body;
            event.request_body_truncated = truncated || limit_truncated;
        }
    }

    fn set_compression_headers(&mut self, downstream: &HeaderMap, upstream: Option<&HeaderMap>) {
        if let Some(event) = &mut self.event {
            if !downstream.is_empty() {
                event.downstream_accept_encoding =
                    header_value(downstream, header::ACCEPT_ENCODING);
            }
            if let Some(upstream) = upstream {
                event.upstream_content_encoding = header_value(upstream, header::CONTENT_ENCODING);
            }
        }
    }

    fn set_upstream_accept_encoding(&mut self, headers: &HeaderMap) {
        if let Some(event) = &mut self.event {
            event.upstream_accept_encoding = header_value(headers, header::ACCEPT_ENCODING)
                .or_else(|| Some("auto (gzip, br, zstd, deflate)".to_owned()));
        }
    }

    fn set_response_headers(&mut self, headers: &HeaderMap) {
        if let Some(event) = &mut self.event {
            if !event.request_archive {
                return;
            }
            event.response_headers_json = Some(archive_headers(headers));
        }
    }

    fn set_upstream_http_version(&mut self, version: Version) {
        if let Some(event) = &mut self.event {
            event.upstream_http_version = Some(
                match version {
                    Version::HTTP_09 => "HTTP/0.9",
                    Version::HTTP_10 => "HTTP/1.0",
                    Version::HTTP_11 => "HTTP/1.1",
                    Version::HTTP_2 => "HTTP/2",
                    Version::HTTP_3 => "HTTP/3",
                    _ => "unknown",
                }
                .to_owned(),
            );
        }
    }

    fn set_upstream_request_headers(&mut self, headers: &HeaderMap) {
        if let Some(event) = &mut self.event {
            if !event.request_archive {
                return;
            }
            event.upstream_request_headers_json = Some(archive_headers(headers));
        }
    }

    fn mark_first_byte(&mut self) {
        let elapsed = self.started.elapsed().as_millis().max(1) as i64;
        if let Some(event) = &mut self.event {
            event.first_byte_latency_ms.get_or_insert(elapsed);
        }
    }

    fn set_request_size(&mut self, bytes: i64) {
        if let Some(event) = &mut self.event {
            event.request_bytes = bytes;
            event.request_transport_bytes = bytes;
        }
    }

    fn set_response_size(&mut self, bytes: i64) {
        if let Some(event) = &mut self.event {
            event.response_bytes = bytes;
            event.response_transport_bytes = bytes;
        }
    }

    fn set_response_body(&mut self, body: &[u8], truncated: bool) {
        if let Some(event) = &mut self.event {
            if !event.request_archive {
                return;
            }
            let (body, limit_truncated) = body_preview(body, self.response_body_limit);
            event.response_body = Some(body);
            event.response_body_truncated = truncated || limit_truncated;
        }
    }

    fn inspect_sse_body(&mut self, bytes: &[u8]) -> Usage {
        let mut observation = SseObservation::default();
        observation.capture(bytes);
        if let Some(event) = &mut self.event {
            event.upstream_model = observation.upstream_model;
        }
        if let (Some(event), Some(failure)) = (&mut self.event, observation.failure) {
            event.error_code = failure.code;
            event.error = Some(failure.message);
        }
        observation.usage
    }

    fn inspect_json_body(&mut self, bytes: &[u8]) -> Usage {
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            return Usage::default();
        };
        if let Some(event) = &mut self.event {
            event.upstream_model = value
                .get("model")
                .and_then(Value::as_str)
                .filter(|model| !model.is_empty())
                .map(str::to_owned);
        }
        usage_from_value(&value)
    }

    fn set_error_code(&mut self, code: &str) {
        if let Some(event) = &mut self.event {
            event.error_code = Some(code.to_owned());
        }
    }

    fn set_error_response(&mut self, error: &AppError) {
        let body = serde_json::to_vec(&json!({"error":{
            "message":error.message(),
            "type":"proxy_error",
            "code":error.status().as_u16(),
            "reason":error.reason(),
        }}))
        .expect("proxy error response is serializable");
        self.set_response_size(body.len() as i64);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.set_response_headers(&headers);
        self.set_response_body(&body, false);
    }

    fn finish(
        &mut self,
        status: StatusCode,
        model: Option<&str>,
        usage: Usage,
        error: Option<&str>,
    ) -> Option<AuditTransportId> {
        let mut event = self.event.take()?;
        settle(
            &mut event,
            status.as_u16() as i64,
            model,
            usage,
            error,
            self.started,
        );
        let transport = AuditTransportId {
            id: event.id.clone(),
            diagnostics_enabled: event.request_archive,
        };
        self.permit
            .take()
            .expect("audit queue capacity is reserved once")
            .send(event, self.archive_budget.take());
        Some(transport)
    }

    fn take_stream(&mut self, status: StatusCode, model: Option<&str>) -> StreamCompletion {
        if let Some(event) = &mut self.event {
            event.status = status.as_u16() as i64;
            event.model = model.map(str::to_owned);
        }
        StreamCompletion::new(
            self.permit.take(),
            self.archive_budget.take(),
            self.event.take(),
            self.started,
        )
    }
}

impl Drop for AuditTracker {
    fn drop(&mut self) {
        if let Some(mut event) = self.event.take() {
            event.response_body_truncated = true;
            settle(
                &mut event,
                499,
                None,
                Usage::default(),
                Some("client_cancelled"),
                self.started,
            );
            self.permit
                .take()
                .expect("audit queue capacity is reserved once")
                .send(event, self.archive_budget.take());
        }
    }
}

fn settle(
    event: &mut AuditEvent,
    status: i64,
    model: Option<&str>,
    usage: Usage,
    error: Option<&str>,
    started: Instant,
) {
    event.status = status;
    event.model = model.map(str::to_owned);
    event.latency_ms = started.elapsed().as_millis().max(1) as i64;
    event.input_tokens = usage.input_tokens;
    event.output_tokens = usage.output_tokens;
    event.cached_tokens = usage.cached_tokens;
    event.official_cost_usd_nanos = model
        .and_then(|model| official_cost_usd_nanos(model, usage))
        .unwrap_or_default();
    event.price_multiplier_nanos =
        effective_price_multiplier_nanos(event.price_multiplier_nanos, event.fast_mode);
    event.actual_cost_usd_nanos =
        apply_price_multiplier(event.official_cost_usd_nanos, event.price_multiplier_nanos);
    if event.error.is_none() {
        event.error = error.map(str::to_owned);
    }
}

pub async fn handle_json(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let identity = api_identity(&state, &headers).await?;
    let audit = RequestAuditContext {
        request_id: Uuid::new_v4().to_string(),
        thread_id: thread_id(&headers),
        session_id: session_id(&headers),
        method,
        client_ip: peer.ip().to_string(),
    };
    // Compressed bodies are decoded before the proxy derives anything from them; the caller's
    // `content-encoding` header stays in place so audit records what actually arrived.
    let body = decode_request_body(&headers, body)?;
    dispatch(state, identity, audit, uri.path(), &headers, &body).await
}

pub async fn handle_audio(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, AppError> {
    let audit_id = Uuid::new_v4().to_string();
    let thread_id = thread_id(&headers);
    let session_id = session_id(&headers);
    let identity = api_identity(&state, &headers).await?;
    let mut audit = AuditTracker::begin(
        &state,
        &identity,
        AuditStart {
            audit_id: &audit_id,
            thread_id: thread_id.as_deref(),
            session_id: session_id.as_deref(),
            method: &method,
            path: uri.path(),
            client_ip: &peer.ip().to_string(),
            response_body_limit: ARCHIVE_BODY_LIMIT,
        },
    );
    audit.set_request(&headers, &[], true);
    audit.set_compression_headers(&headers, None);
    let result = async {
        ensure_request_quota(&state, &identity.user_id, identity.allow_debt).await?;
        dispatch_audio(state, identity, uri.path(), &headers, body, &mut audit).await
    }
    .await;
    finish_audit_error(&mut audit, result)
}

pub async fn handle_console_transcription(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    headers: HeaderMap,
    body: Body,
) -> Result<axum::Json<Value>, AppError> {
    let allow_debt = user_allows_debt(&state, &user.id).await?;
    ensure_request_quota(&state, &user.id, allow_debt).await?;
    let data_dir = state.config.load().data_dir.clone();
    let spool = AudioSpool::create(&data_dir, body, 0).await?;
    let mut first =
        select_ready_provider(&state, None, Downstream::new(&user.id, CODEX_ORIGINATOR)).await?;
    let first_id = first.provider.id.clone();
    let response =
        send_transcription_upstream(&state, &first, &headers, spool.body().await?).await?;
    let response = track_upstream(&state, &first_id, UpstreamResponse::Live(response)).await?;
    let (_lease, upstream) = if retryable(response.status()) {
        let response = finish_retry_attempt(&mut first, response).await?;
        match retry_provider(
            &state,
            None,
            &first_id,
            Downstream::new(&user.id, CODEX_ORIGINATOR),
        )
        .await
        {
            Some(second) => {
                drop(response);
                drop(first);
                let second_response =
                    send_transcription_upstream(&state, &second, &headers, spool.body().await?)
                        .await?;
                let second_response = track_upstream(
                    &state,
                    &second.provider.id,
                    UpstreamResponse::Live(second_response),
                )
                .await?;
                (second, second_response)
            }
            None => (first, response),
        }
    } else {
        (first, response)
    };
    drop(spool);
    let status = upstream.status();
    let body = upstream.into_bytes().await?;
    if !status.is_success() {
        return Err(AppError::upstream(
            status.as_u16(),
            error_from(status, &body).unwrap_or_else(|| "transcription failed".to_owned()),
        ));
    }
    let response = serde_json::from_slice::<Value>(&body)?;
    let text = response
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| {
            AppError::upstream(status.as_u16(), "transcription response is missing text")
        })?;
    Ok(axum::Json(json!({ "text": text })))
}

pub async fn handle_console_image_generation(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<axum::Json<Value>, AppError> {
    let identity = console_api_identity(&state, &user).await?;
    let audit = RequestAuditContext {
        request_id: Uuid::new_v4().to_string(),
        thread_id: thread_id(&headers),
        session_id: session_id(&headers),
        method: Method::POST,
        client_ip: peer.ip().to_string(),
    };
    dispatch_console_image_generation(state, identity, audit, headers, body).await
}

async fn console_api_identity(
    state: &AppState,
    user: &UserIdentity,
) -> Result<ApiIdentity, AppError> {
    let consumer_id = format!("console-{}", crate::crypto::consumer_secret_hash(&user.id));
    let allow_debt = user_allows_debt(state, &user.id).await?;
    let _write = state.write_gate.lock().await;
    sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,request_archive,is_system,created_at) VALUES(?,?, 'OpenAI-LB Console','console',?,1,1,?) ON CONFLICT(id) DO NOTHING")
        .bind(&consumer_id)
        .bind(&user.id)
        .bind(crate::crypto::consumer_secret_hash(&Uuid::new_v4().to_string()))
        .bind(chrono::Utc::now().timestamp())
        .execute(&state.db)
        .await?;
    Ok(ApiIdentity {
        consumer_id,
        user_id: user.id.clone(),
        request_archive: true,
        intercept_degradation: false,
        is_admin: matches!(user.role.as_str(), "root" | "admin"),
        allow_debt,
    })
}

async fn user_allows_debt(state: &AppState, user_id: &str) -> Result<bool, AppError> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT allow_debt FROM users WHERE id=?")
            .bind(user_id)
            .fetch_one(&state.db)
            .await?
            != 0,
    )
}

async fn ensure_request_quota(
    state: &AppState,
    user_id: &str,
    allow_debt: bool,
) -> Result<(), AppError> {
    if state.config.load().allow_all_users_debt || allow_debt {
        return Ok(());
    }
    let (provided_usd_nanos, consumed_usd_nanos): (i64, i64) =
        sqlx::query_as("SELECT provided_usd_nanos,consumed_usd_nanos FROM users WHERE id=?")
            .bind(user_id)
            .fetch_one(&state.db)
            .await?;
    let config = state.config.load_full();
    let topup_usd_nanos = if midas::is_configured(&config) {
        midas::inbound_transfer_totals(state, &[user_id.to_owned()])
            .await?
            .remove(user_id)
            .expect("Midas returns every requested user")
    } else {
        0
    };
    let available_usd_nanos = topup_usd_nanos
        .saturating_add(provided_usd_nanos)
        .saturating_sub(consumed_usd_nanos);
    (available_usd_nanos > 0).then_some(()).ok_or_else(|| {
        AppError::payment_required("prepaid credit is required before making a request")
    })
}

async fn dispatch_console_image_generation(
    state: AppState,
    identity: ApiIdentity,
    audit_context: RequestAuditContext,
    headers: HeaderMap,
    body: Bytes,
) -> Result<axum::Json<Value>, AppError> {
    let payload = serde_json::from_slice(&body)
        .map_err(|_| AppError::bad_request("JSON request body required"))?;
    let request = prepare_responses_request("/v1/images/generations", Some(payload), &state)?;
    let affinity_key = affinity_key(&headers, None);
    let affinity = affinity_key
        .as_ref()
        .map(|key| format!("{}:{}", identity.consumer_id, key.value));
    let mut audit = begin_responses_audit(&state, &identity, &audit_context, &headers, &request);
    audit.set_affinity(
        affinity.as_deref(),
        affinity_key.as_ref().map(|key| key.source),
    );
    let result = async {
        ensure_request_quota(&state, &identity.user_id, identity.allow_debt).await?;
        let (_lease, upstream) = dispatch_responses(
            &state,
            "/v1/images/generations",
            &headers,
            &request,
            affinity.as_deref(),
            &mut audit,
            Downstream::new(&identity.user_id, CODEX_ORIGINATOR),
        )
        .await?;
        let status = upstream.status();
        let body = upstream.into_bytes().await?;
        if !status.is_success() {
            return Err(AppError::upstream(
                status.as_u16(),
                error_from(status, &body).unwrap_or_else(|| "image generation failed".to_owned()),
            ));
        }
        audit.inspect_sse_body(&body);
        let (data, usage) = images_from_sse(&body)?;
        let response = json!({ "data": data });
        let response_body = serde_json::to_vec(&response)?;
        audit.mark_first_byte();
        audit.set_response_size(response_body.len() as i64);
        let mut response_headers = HeaderMap::new();
        response_headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        audit.set_response_headers(&response_headers);
        audit.set_response_body(&response_body, false);
        audit.finish(status, request.model.as_deref(), usage, None);
        Ok(axum::Json(response))
    }
    .await;
    finish_audit_error(&mut audit, result)
}

pub async fn handle_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    api_identity(&state, &headers).await?;
    let config = state.config.load();
    models_response(&config.available_model_ids)
}

pub async fn handle_realtime_call(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Response, AppError> {
    let identity = api_identity(&state, &headers).await?;
    let request = realtime_call_request(multipart).await?;
    let result = start_realtime_call(
        &state,
        &identity,
        RequestAuditContext {
            request_id: Uuid::new_v4().to_string(),
            thread_id: thread_id(&headers),
            session_id: session_id(&headers),
            method,
            client_ip: peer.ip().to_string(),
        },
        uri.path(),
        &headers,
        request,
        RealtimeCallSource::Consumer,
    )
    .await?;
    realtime_call_response(result)
}

pub async fn handle_console_realtime_call(
    State(state): State<AppState>,
    Extension(user): Extension<UserIdentity>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    mut headers: HeaderMap,
    multipart: Multipart,
) -> Result<Response, AppError> {
    let identity = console_api_identity(&state, &user).await?;
    let upstream_session_id = Uuid::new_v4().to_string();
    headers.insert(
        HeaderName::from_static("session-id"),
        HeaderValue::from_str(&upstream_session_id)
            .expect("a UUID is always a valid HTTP header value"),
    );
    let request = realtime_call_request(multipart).await?;
    let result = start_realtime_call(
        &state,
        &identity,
        RequestAuditContext {
            request_id: Uuid::new_v4().to_string(),
            thread_id: thread_id(&headers),
            session_id: session_id(&headers),
            method,
            client_ip: peer.ip().to_string(),
        },
        "/api/realtime/calls",
        &headers,
        request,
        RealtimeCallSource::Console,
    )
    .await?;
    let token = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    state
        .realtime_console_sessions
        .retain(|_, session| session.expires_at > now);
    state.realtime_console_sessions.insert(
        token.clone(),
        RealtimeConsoleSession {
            identity,
            call_id: result.call_id.clone(),
            upstream_session_id,
            expires_at: now + 600,
        },
    );
    let sdp = String::from_utf8(result.sdp.to_vec())
        .map_err(|_| AppError::upstream(502, "realtime upstream returned invalid SDP"))?;
    let mut response = axum::Json(json!({
        "sdp": sdp,
        "call_id": result.call_id,
        "sideband_token": token,
    }))
    .into_response();
    if let Some(audit_transport) = result.audit_transport {
        response.extensions_mut().insert(audit_transport);
    }
    Ok(response)
}

pub async fn handle_realtime_websocket(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let identity = api_identity(&state, &headers).await?;
    let client = identity::identify_client(&headers)?;
    let call_id = realtime_call_id_from_query(uri.query())?;
    let provider_id = state
        .balancer
        .affinity_provider_id(&realtime_affinity_key(&identity.consumer_id, &call_id))
        .ok_or_else(|| AppError::not_found("realtime call is unknown or has expired"))?;
    let downstream = Downstream::new(&identity.user_id, client.provider_originator());
    realtime_websocket_response(
        state,
        &identity,
        provider_id,
        call_id,
        headers,
        websocket,
        downstream,
    )
    .await
}

pub async fn handle_console_realtime_websocket(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    websocket: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let token = realtime_query_value(uri.query(), "token")
        .ok_or_else(|| AppError::unauthorized("missing realtime session token"))?;
    let (_, session) = state
        .realtime_console_sessions
        .remove(&token)
        .filter(|(_, session)| session.expires_at > chrono::Utc::now().timestamp())
        .ok_or_else(|| AppError::unauthorized("realtime session token is invalid or expired"))?;
    let provider_id = state
        .balancer
        .affinity_provider_id(&realtime_affinity_key(
            &session.identity.consumer_id,
            &session.call_id,
        ))
        .ok_or_else(|| AppError::not_found("realtime call is unknown or has expired"))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-session-id"),
        HeaderValue::from_str(&session.upstream_session_id)
            .expect("a UUID is always a valid HTTP header value"),
    );
    let downstream = Downstream::new(&session.identity.user_id, CODEX_ORIGINATOR);
    realtime_websocket_response(
        state,
        &session.identity,
        provider_id,
        session.call_id,
        headers,
        websocket,
        downstream,
    )
    .await
}

async fn realtime_call_request(mut multipart: Multipart) -> Result<RealtimeCallRequest, AppError> {
    let mut sdp = None;
    let mut session = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("invalid realtime multipart request"))?
    {
        match field.name() {
            Some("sdp") if sdp.is_none() => {
                sdp = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| AppError::bad_request("invalid realtime SDP"))?,
                );
            }
            Some("session") if session.is_none() => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| AppError::bad_request("invalid realtime session"))?;
                session = Some(
                    serde_json::from_slice(&bytes)
                        .map_err(|_| AppError::bad_request("realtime session must be JSON"))?,
                );
            }
            _ => {}
        }
    }
    let sdp = sdp
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("realtime SDP is required"))?;
    let session = session
        .filter(Value::is_object)
        .ok_or_else(|| AppError::bad_request("realtime session must be a JSON object"))?;
    Ok(RealtimeCallRequest { sdp, session })
}

/// Where a realtime call enters the proxy. The LB console is first-party traffic:
/// it carries a browser user agent and is authorized through an Auth Mini session.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RealtimeCallSource {
    Console,
    Consumer,
}

async fn start_realtime_call(
    state: &AppState,
    identity: &ApiIdentity,
    context: RequestAuditContext,
    path: &str,
    headers: &HeaderMap,
    request: RealtimeCallRequest,
    source: RealtimeCallSource,
) -> Result<RealtimeCallResponse, AppError> {
    let body = serde_json::to_vec(&json!({"sdp": &request.sdp, "session": &request.session}))?;
    let affinity_key = affinity_key(headers, Some(&request.session));
    let affinity = affinity_key
        .as_ref()
        .map(|key| format!("{}:{}", identity.consumer_id, key.value));
    let model = request
        .session
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut audit = AuditTracker::begin(
        state,
        identity,
        AuditStart {
            audit_id: &context.request_id,
            thread_id: context.thread_id.as_deref(),
            session_id: context.session_id.as_deref(),
            method: &context.method,
            path,
            client_ip: &context.client_ip,
            response_body_limit: ARCHIVE_BODY_LIMIT,
        },
    );
    audit.set_request(headers, &body, false);
    audit.set_request_size(body.len() as i64);
    audit.set_affinity(
        affinity.as_deref(),
        affinity_key.as_ref().map(|key| key.source),
    );
    let result = async {
        let originator = match source {
            RealtimeCallSource::Console => CODEX_ORIGINATOR,
            RealtimeCallSource::Consumer => {
                identity::identify_client(headers)?.provider_originator()
            }
        };
        ensure_request_quota(state, &identity.user_id, identity.allow_debt).await?;
        let downstream = Downstream::new(&identity.user_id, originator);
        let (lease, upstream) = dispatch_realtime_call(
            state,
            headers,
            &body,
            affinity.as_deref(),
            &mut audit,
            downstream,
        )
        .await?;
        let status = upstream.status();
        audit.mark_first_byte();
        audit.set_upstream_http_version(upstream.version());
        audit.set_response_headers(upstream.headers());
        let call_id = status
            .is_success()
            .then(|| realtime_call_id_from_headers(upstream.headers()))
            .transpose()?;
        let response = upstream.into_bytes().await?;
        audit.set_response_size(response.len() as i64);
        audit.set_response_body(&response, false);
        if !status.is_success() {
            return Err(AppError::upstream(
                status.as_u16(),
                error_from(status, &response).unwrap_or_else(|| "realtime call failed".to_owned()),
            ));
        }
        let call_id = call_id.expect("successful realtime calls have a validated call ID");
        state.balancer.pin_affinity(
            state,
            &realtime_affinity_key(&identity.consumer_id, &call_id),
            &lease.provider.id,
        );
        let audit_transport = audit.finish(status, model.as_deref(), Usage::default(), None);
        Ok(RealtimeCallResponse {
            sdp: response,
            call_id,
            audit_transport,
        })
    }
    .await;
    finish_audit_error(&mut audit, result)
}

fn realtime_call_response(result: RealtimeCallResponse) -> Result<Response, AppError> {
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/sdp")
        .header(
            header::LOCATION,
            format!("/v1/realtime/calls/{}", result.call_id),
        )
        .body(Body::from(result.sdp))
        .expect("realtime call response is valid");
    if let Some(audit_transport) = result.audit_transport {
        response.extensions_mut().insert(audit_transport);
    }
    Ok(response)
}

async fn dispatch_realtime_call(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
    affinity: Option<&str>,
    audit: &mut AuditTracker,
    downstream: Downstream<'_>,
) -> Result<(Lease, UpstreamResponse), AppError> {
    let mut first = select_ready_provider(state, affinity, downstream).await?;
    audit.set_provider(&first);
    let first_id = first.provider.id.clone();
    let response = send_realtime_call_upstream(state, &first, headers, body).await?;
    let response = track_upstream(state, &first_id, UpstreamResponse::Live(response)).await?;
    if !retryable(response.status()) {
        return Ok((first, response));
    }
    let response = finish_retry_attempt(&mut first, response).await?;
    let Some(second) = retry_provider(state, affinity, &first_id, downstream).await else {
        return Ok((first, response));
    };
    drop(response);
    drop(first);
    audit.set_provider(&second);
    let response = send_realtime_call_upstream(state, &second, headers, body).await?;
    let response =
        track_upstream(state, &second.provider.id, UpstreamResponse::Live(response)).await?;
    Ok((second, response))
}

async fn send_realtime_call_upstream(
    state: &AppState,
    lease: &Lease,
    inbound: &HeaderMap,
    body: &[u8],
) -> Result<reqwest::Response, AppError> {
    let payload: Value = serde_json::from_slice(body)
        .map_err(|_| AppError::bad_request("invalid realtime request payload"))?;
    let raw_session = payload
        .get("session")
        .filter(|value| value.is_object())
        .ok_or_else(|| AppError::bad_request("realtime session must be a JSON object"))?;
    let sdp = payload
        .get("sdp")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("realtime SDP is required"))?;
    let session = normalize_realtime_session(raw_session)?;
    let (endpoint, multipart) = realtime_call_request_parts(state, sdp, &session)?;
    let client = state.provider_client(&lease.provider)?;
    let mut request = client.post(endpoint);
    for (name, value) in inbound {
        if should_forward_realtime_header(name) {
            request = request.header(name, value);
        }
    }
    let mut request = request
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={REALTIME_MULTIPART_BOUNDARY}"),
        )
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", lease.access_token),
        )
        .header("chatgpt-account-id", &lease.provider.account_id)
        .body(multipart)
        .build()?;
    let user_agent = lease
        .upstream_user_agent(&state.config.load())
        .map(str::to_owned);
    apply_upstream_identity_headers(
        request.headers_mut(),
        &lease.provider,
        user_agent.as_deref(),
        &lease.provider.originator,
    )?;
    Ok(client.execute(request).await?)
}

async fn realtime_websocket_response(
    state: AppState,
    identity: &ApiIdentity,
    provider_id: String,
    call_id: String,
    inbound: HeaderMap,
    websocket: WebSocketUpgrade,
    downstream: Downstream<'_>,
) -> Result<Response, AppError> {
    ensure_request_quota(&state, &identity.user_id, identity.allow_debt).await?;
    let mut lease = state
        .balancer
        .select_provider(&provider_id, downstream)
        .await?;
    refresh_if_needed(&state, &mut lease).await?;
    ensure_realtime_tls_provider();
    let request = realtime_upstream_websocket_request(&state, &lease, &inbound, &call_id)?;
    let (upstream, _) = connect_realtime_upstream(&state, &lease, request)
        .await
        .map_err(|error| {
            let message = match &error {
                tokio_tungstenite::tungstenite::Error::Http(response) => format!(
                    "realtime upstream websocket handshake failed with HTTP {}",
                    response.status()
                ),
                _ => format!("realtime upstream websocket handshake failed: {error}"),
            };
            AppError::upstream(502, message)
        })?;
    Ok(websocket
        .on_upgrade(move |socket| realtime_websocket_bridge(socket, upstream, lease))
        .into_response())
}

fn ensure_realtime_tls_provider() {
    REALTIME_TLS_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

async fn connect_realtime_upstream(
    _state: &AppState,
    lease: &Lease,
    request: axum::http::Request<()>,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
        axum::http::Response<Option<Vec<u8>>>,
    ),
    tokio_tungstenite::tungstenite::Error,
> {
    let Some(proxy_url) = lease.provider.http_proxy_url.as_deref() else {
        return connect_async(request).await;
    };
    let proxy = url::Url::parse(proxy_url).map_err(|_| {
        tokio_tungstenite::tungstenite::Error::Url(
            tokio_tungstenite::tungstenite::error::UrlError::NoHostName,
        )
    })?;
    let host = request
        .uri()
        .host()
        .ok_or(tokio_tungstenite::tungstenite::Error::Url(
            tokio_tungstenite::tungstenite::error::UrlError::NoHostName,
        ))?;
    let port = request.uri().port_u16().unwrap_or(443);
    let mut stream = TcpStream::connect(format!(
        "{}:{}",
        proxy.host_str().unwrap_or_default(),
        proxy.port_or_known_default().unwrap_or(80)
    ))
    .await?;
    let credentials = match proxy.password() {
        Some(password) => format!(
            "Proxy-Authorization: Basic {}\r\n",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{password}", proxy.username()))
        ),
        None => String::new(),
    };
    stream
        .write_all(
            format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n{credentials}\r\n")
                .as_bytes(),
        )
        .await?;
    let mut response = Vec::new();
    loop {
        let mut byte = [0; 1];
        stream.read_exact(&mut byte).await?;
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") || response.len() > 8192 {
            break;
        }
    }
    if !response.starts_with(b"HTTP/1.1 200") && !response.starts_with(b"HTTP/1.0 200") {
        return Err(tokio_tungstenite::tungstenite::Error::Http(Box::new(
            axum::http::Response::builder()
                .status(502)
                .body(Some(response))
                .unwrap(),
        )));
    }
    client_async_tls_with_config(request, stream, None, None).await
}

fn realtime_upstream_websocket_request(
    state: &AppState,
    lease: &Lease,
    inbound: &HeaderMap,
    call_id: &str,
) -> Result<axum::http::Request<()>, AppError> {
    let mut endpoint = realtime_api_base_url(state)?;
    endpoint.set_path(&format!(
        "{}/realtime",
        endpoint.path().trim_end_matches('/')
    ));
    endpoint.set_query(None);
    endpoint.query_pairs_mut().append_pair("call_id", call_id);
    match endpoint.scheme() {
        "https" => endpoint.set_scheme("wss").expect("wss is a valid scheme"),
        "http" => endpoint.set_scheme("ws").expect("ws is a valid scheme"),
        _ => return Err(AppError::internal("unsupported realtime upstream scheme")),
    }
    let mut request = endpoint
        .as_str()
        .into_client_request()
        .map_err(|_| AppError::internal("failed to create realtime websocket request"))?;
    for (name, value) in inbound {
        if should_forward_realtime_header(name) {
            request.headers_mut().insert(name, value.clone());
        }
    }
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", lease.access_token))
            .map_err(|_| AppError::internal("invalid realtime upstream credential"))?,
    );
    request.headers_mut().insert(
        HeaderName::from_static("chatgpt-account-id"),
        HeaderValue::from_str(&lease.provider.account_id)
            .map_err(|_| AppError::internal("invalid realtime account ID"))?,
    );
    let user_agent = lease
        .upstream_user_agent(&state.config.load())
        .map(str::to_owned);
    apply_upstream_identity_headers(
        request.headers_mut(),
        &lease.provider,
        user_agent.as_deref(),
        &lease.provider.originator,
    )?;
    Ok(request)
}

async fn realtime_websocket_bridge(
    socket: WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    _lease: Lease,
) {
    let (mut client_send, mut client_receive) = socket.split();
    let (mut upstream_send, mut upstream_receive) = upstream.split();
    let client_to_upstream = async {
        while let Some(Ok(message)) = client_receive.next().await {
            if upstream_send
                .send(realtime_upstream_message(message))
                .await
                .is_err()
            {
                break;
            }
        }
    };
    let upstream_to_client = async {
        while let Some(Ok(message)) = upstream_receive.next().await {
            if client_send
                .send(realtime_client_message(message))
                .await
                .is_err()
            {
                break;
            }
        }
    };
    tokio::select! {
        _ = client_to_upstream => {}
        _ = upstream_to_client => {}
    }
}

fn realtime_upstream_message(message: WebSocketMessage) -> TungsteniteMessage {
    match message {
        WebSocketMessage::Text(value) => TungsteniteMessage::Text(value.to_string().into()),
        WebSocketMessage::Binary(value) => TungsteniteMessage::Binary(value.to_vec().into()),
        WebSocketMessage::Ping(value) => TungsteniteMessage::Ping(value.to_vec().into()),
        WebSocketMessage::Pong(value) => TungsteniteMessage::Pong(value.to_vec().into()),
        WebSocketMessage::Close(_) => TungsteniteMessage::Close(None),
    }
}

fn realtime_client_message(message: TungsteniteMessage) -> WebSocketMessage {
    match message {
        TungsteniteMessage::Text(value) => WebSocketMessage::Text(value.to_string().into()),
        TungsteniteMessage::Binary(value) => WebSocketMessage::Binary(value.to_vec().into()),
        TungsteniteMessage::Ping(value) => WebSocketMessage::Ping(value.to_vec().into()),
        TungsteniteMessage::Pong(value) => WebSocketMessage::Pong(value.to_vec().into()),
        TungsteniteMessage::Close(_) | TungsteniteMessage::Frame(_) => {
            WebSocketMessage::Close(None)
        }
    }
}

fn realtime_affinity_key(consumer_id: &str, call_id: &str) -> String {
    format!("{consumer_id}:realtime:{call_id}")
}

fn realtime_call_id_from_headers(headers: &HeaderMap) -> Result<String, AppError> {
    let location = headers
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::upstream(502, "realtime upstream response is missing Location"))?;
    location
        .split('?')
        .next()
        .unwrap_or(location)
        .rsplit('/')
        .find(|segment| segment.starts_with("rtc_") && segment.len() > 4)
        .map(str::to_owned)
        .ok_or_else(|| {
            AppError::upstream(502, "realtime upstream response has an invalid Location")
        })
}

fn realtime_call_id_from_query(query: Option<&str>) -> Result<String, AppError> {
    realtime_query_value(query, "call_id")
        .filter(|call_id| call_id.starts_with("rtc_") && call_id.len() > 4)
        .ok_or_else(|| AppError::bad_request("realtime call_id is required"))
}

fn realtime_api_base_url(state: &AppState) -> Result<url::Url, AppError> {
    let configured = state.config.load().upstream_base.clone();
    let mut base = url::Url::parse(&configured)?;
    let host = base.host_str().unwrap_or_default();
    if matches!(host, "chatgpt.com" | "chat.openai.com") {
        base = url::Url::parse(REALTIME_API_BASE_URL)?;
    }
    if base
        .path()
        .trim_end_matches('/')
        .ends_with("/backend-api/codex")
    {
        base.set_path("/v1");
    }
    base.set_query(None);
    base.set_fragment(None);
    Ok(base)
}

fn normalize_realtime_session(session: &Value) -> Result<Value, AppError> {
    let mut session = session.clone();
    let object = session
        .as_object_mut()
        .ok_or_else(|| AppError::bad_request("realtime session must be a JSON object"))?;
    if object.contains_key("delegation") {
        return Err(AppError::bad_request(
            "GPT-Live delegation is not supported by the Realtime API endpoint",
        ));
    }
    // COMPATIBILITY: the first LB voice page shipped the ChatGPT AVAS session type.
    // Keep accepting it while callers migrate to the public Realtime session shape; remove
    // after audit archives contain no `type: quicksilver` requests for a full retention window.
    if matches!(
        object.get("type").and_then(Value::as_str),
        Some("quicksilver") | None
    ) {
        object.insert("type".to_owned(), Value::String("realtime".to_owned()));
    } else if object.get("type").and_then(Value::as_str) != Some("realtime") {
        return Err(AppError::bad_request(
            "realtime session.type must be realtime",
        ));
    }
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .unwrap_or("gpt-realtime-1.5")
        .to_owned();
    object.insert("model".to_owned(), Value::String(model));
    Ok(session)
}

fn realtime_call_request_parts(
    state: &AppState,
    sdp: &str,
    session: &Value,
) -> Result<(String, Vec<u8>), AppError> {
    let mut endpoint = realtime_api_base_url(state)?;
    endpoint.set_path(&format!(
        "{}/realtime/calls",
        endpoint.path().trim_end_matches('/')
    ));
    endpoint.set_query(None);
    let session = serde_json::to_vec(session)?;
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{REALTIME_MULTIPART_BOUNDARY}\r\nContent-Disposition: form-data; name=\"sdp\"\r\nContent-Type: application/sdp\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(sdp.as_bytes());
    body.extend_from_slice(
        format!(
            "\r\n--{REALTIME_MULTIPART_BOUNDARY}\r\nContent-Disposition: form-data; name=\"session\"\r\nContent-Type: application/json\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&session);
    body.extend_from_slice(format!("\r\n--{REALTIME_MULTIPART_BOUNDARY}--\r\n").as_bytes());
    Ok((endpoint.to_string(), body))
}

fn should_forward_realtime_header(name: &HeaderName) -> bool {
    !name.as_str().eq_ignore_ascii_case("openai-alpha")
        && should_forward_request_header("/v1/realtime", name)
}

fn realtime_query_value(query: Option<&str>, key: &str) -> Option<String> {
    query.and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes()).find_map(|(name, value)| {
            (name == key && !value.is_empty()).then(|| value.into_owned())
        })
    })
}

fn response_archive_body_limit(image: bool) -> usize {
    if image {
        IMAGE_ARCHIVE_BODY_LIMIT
    } else {
        ARCHIVE_BODY_LIMIT
    }
}

fn body_preview(body: &[u8], limit: usize) -> (Vec<u8>, bool) {
    let end = body.len().min(limit);
    (body[..end].to_vec(), body.len() > end)
}

pub(crate) fn archive_headers(headers: &HeaderMap) -> String {
    let values = headers
        .iter()
        .filter(|(name, _)| *name != header::AUTHORIZATION)
        .map(|(name, value)| {
            (
                name.as_str(),
                value.to_str().unwrap_or("<non-UTF-8 header value>"),
            )
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&values).expect("HTTP headers are serializable")
}

#[derive(Default)]
struct StreamingPreview {
    body: Vec<u8>,
    truncated: bool,
    bytes: i64,
}

impl StreamingPreview {
    fn capture(&mut self, bytes: &[u8]) {
        self.bytes += bytes.len() as i64;
        let remaining = ARCHIVE_BODY_LIMIT.saturating_sub(self.body.len());
        let copied = remaining.min(bytes.len());
        self.body.extend_from_slice(&bytes[..copied]);
        self.truncated |= copied < bytes.len();
    }
}

fn thread_id(headers: &HeaderMap) -> Option<String> {
    ["x-codex-conversation-id", "thread-id"]
        .iter()
        .find_map(|name| {
            headers
                .get(*name)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
}

fn session_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

async fn dispatch(
    state: AppState,
    identity: ApiIdentity,
    audit_context: RequestAuditContext,
    path: &str,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, AppError> {
    let parsed = serde_json::from_slice::<Value>(body).ok();
    let affinity_key = affinity_key(headers, parsed.as_ref());
    let affinity = affinity_key
        .as_ref()
        .map(|key| format!("{}:{}", identity.consumer_id, key.value));
    let request = prepare_responses_request(path, parsed, &state)?;
    let mut audit = begin_responses_audit(&state, &identity, &audit_context, headers, &request);
    audit.set_affinity(
        affinity.as_deref(),
        affinity_key.as_ref().map(|key| key.source),
    );
    let result = async {
        let client = identity::identify_client(headers)?;
        // INVARIANT: a Pi-identified request must carry Pi's own UUIDv7. Validating here rather than
        // in the middleware keeps the rejection inside the audited region, so an operator can see
        // which Consumer presented an id Pi would not have produced.
        identity::require_pi_session_id(headers, path)?;
        ensure_request_quota(&state, &identity.user_id, identity.allow_debt).await?;
        let downstream = Downstream::new(&identity.user_id, client.provider_originator());
        let (lease, upstream) = dispatch_responses(
            &state,
            path,
            headers,
            &request,
            affinity.as_deref(),
            &mut audit,
            downstream,
        )
        .await?;
        intercept_degraded_response(&identity, &upstream, &mut audit)?;
        let context = CallContext {
            model: request.model.clone(),
            stream: request.stream,
            cookie_key: upstream_cookie_key(&lease, affinity.as_deref()),
        };
        if request.image {
            image_response(&state, context, lease, upstream, &mut audit).await
        } else {
            relay_response(&state, context, lease, upstream, &mut audit).await
        }
    }
    .await;
    finish_audit_error(&mut audit, result)
}

fn prepare_responses_request(
    path: &str,
    parsed: Option<Value>,
    state: &AppState,
) -> Result<ResponsesRequest, AppError> {
    let image = path == "/v1/images/generations";
    let model = if image {
        None
    } else {
        let model = parsed.as_ref().and_then(|value| value.get("model"));
        model
            .map(|model| {
                model
                    .as_str()
                    .filter(|model| !model.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| AppError::bad_request("model must be a non-empty string"))
            })
            .transpose()?
    };
    if let Some(model) = model.as_deref() {
        ensure_model_is_available(state, model)?;
    }
    let stream = parsed
        .as_ref()
        .and_then(|value| value.get("stream"))
        .and_then(Value::as_bool)
        .unwrap_or_default();
    let body = transform_request(path, parsed, state)?;
    let model = if image {
        serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    } else {
        model
    };
    let stream = if image { true } else { stream };
    Ok(ResponsesRequest {
        body,
        model,
        stream,
        image,
    })
}

fn begin_responses_audit(
    state: &AppState,
    identity: &ApiIdentity,
    context: &RequestAuditContext,
    headers: &HeaderMap,
    request: &ResponsesRequest,
) -> AuditTracker {
    let mut audit = AuditTracker::begin(
        state,
        identity,
        AuditStart {
            audit_id: &context.request_id,
            thread_id: context.thread_id.as_deref(),
            session_id: context.session_id.as_deref(),
            method: &context.method,
            path: "/v1/responses",
            client_ip: &context.client_ip,
            response_body_limit: response_archive_body_limit(request.image),
        },
    );
    audit.set_request(headers, &request.body, false);
    audit.set_compression_headers(headers, None);
    audit.set_request_size(request.body.len() as i64);
    let request_json = serde_json::from_slice::<Value>(&request.body).ok();
    audit.set_reasoning_effort(request_json.as_ref().and_then(reasoning_effort).as_deref());
    audit.set_fast_mode(request_json.as_ref().is_some_and(fast_mode));
    audit
}

fn finish_audit_error<T>(
    audit: &mut AuditTracker,
    result: Result<T, AppError>,
) -> Result<T, AppError> {
    result.map_err(|error| {
        if audit.event.is_none() {
            return error;
        }
        audit.set_error_response(&error);
        let audit_transport = audit
            .finish(
                error.status(),
                None,
                Usage::default(),
                Some(error.message()),
            )
            .expect("an active audit event always has a transport ID");
        error.with_audit_transport(audit_transport)
    })
}

/// Headers for the upstream attempts of one dispatched request.
///
/// Image generation is a one-off request, so its caller is never required to present a session id
/// (the router only enforces the header elsewhere). The upstream still receives one: when the
/// caller supplies none, the proxy mints a fresh UUIDv7 whose embedded timestamp is the request
/// time, which is the value a one-shot client would present. A caller-supplied session id goes
/// upstream unchanged.
fn upstream_request_headers<'a>(path: &str, inbound: &'a HeaderMap) -> Cow<'a, HeaderMap> {
    if path != "/v1/images/generations" || session_id(inbound).is_some() {
        return Cow::Borrowed(inbound);
    }
    let mut headers = inbound.clone();
    headers.insert(
        HeaderName::from_static("session-id"),
        HeaderValue::from_str(&Uuid::now_v7().to_string())
            .expect("a UUID is always a valid header value"),
    );
    Cow::Owned(headers)
}

async fn dispatch_responses(
    state: &AppState,
    source_path: &str,
    headers: &HeaderMap,
    request: &ResponsesRequest,
    affinity: Option<&str>,
    audit: &mut AuditTracker,
    downstream: Downstream<'_>,
) -> Result<(Lease, UpstreamResponse), AppError> {
    let headers = upstream_request_headers(source_path, headers);
    let headers = headers.as_ref();
    let mut first = select_ready_provider(state, affinity, downstream).await?;
    audit.set_provider(&first);
    let first_id = first.provider.id.clone();
    let response = send_upstream(
        state,
        &first,
        source_path,
        headers,
        upstream_request_body(&first, source_path, &request.body)?.into(),
        affinity,
        audit,
    )
    .await?;
    let response = track_upstream(state, &first_id, UpstreamResponse::Live(response)).await?;
    if !retryable(response.status()) {
        return Ok((first, response));
    }
    let response = finish_retry_attempt(&mut first, response).await?;
    let Some(second) = retry_provider(state, affinity, &first_id, downstream).await else {
        return Ok((first, response));
    };
    drop(response);
    drop(first);
    audit.set_provider(&second);
    let second_response = send_upstream(
        state,
        &second,
        source_path,
        headers,
        upstream_request_body(&second, source_path, &request.body)?.into(),
        affinity,
        audit,
    )
    .await?;
    let second_response = track_upstream(
        state,
        &second.provider.id,
        UpstreamResponse::Live(second_response),
    )
    .await?;
    Ok((second, second_response))
}

fn reasoning_effort(request: &Value) -> Option<String> {
    request
        .pointer("/reasoning/effort")
        .and_then(Value::as_str)
        .or_else(|| request.get("reasoning_effort").and_then(Value::as_str))
        .map(str::to_owned)
}

fn fast_mode(request: &Value) -> bool {
    matches!(
        request.get("service_tier").and_then(Value::as_str),
        Some("fast" | "priority")
    )
}

const FAST_MODE_MULTIPLIER: i64 = 2;

fn effective_price_multiplier_nanos(price_multiplier_nanos: i64, fast_mode: bool) -> i64 {
    let multiplier = i128::from(price_multiplier_nanos.max(0))
        * i128::from(if fast_mode { FAST_MODE_MULTIPLIER } else { 1 });
    i64::try_from(multiplier).expect("configured price multiplier keeps costs in range")
}

async fn dispatch_audio(
    state: AppState,
    identity: ApiIdentity,
    path: &str,
    headers: &HeaderMap,
    body: Body,
    audit: &mut AuditTracker,
) -> Result<Response, AppError> {
    let client = identity::identify_client(headers)?;
    // COMPATIBILITY: the transcription upstream is a desktop-only endpoint, so only the CodeX
    // harness family (and the LB console) may spend a provider on it.
    if client.provider_originator() != CODEX_ORIGINATOR {
        return Err(identity::capability_requires_codex_client());
    }
    let affinity_key = affinity_key(headers, None);
    let affinity = affinity_key
        .as_ref()
        .map(|key| format!("{}:{}", identity.consumer_id, key.value));
    audit.set_affinity(
        affinity.as_deref(),
        affinity_key.as_ref().map(|key| key.source),
    );
    let data_dir = state.config.load().data_dir.clone();
    let spool = AudioSpool::create(&data_dir, body, ARCHIVE_BODY_LIMIT).await?;
    audit.set_request(headers, &spool.preview, spool.preview_truncated);
    audit.set_request_size(spool.bytes);
    let mut first = select_ready_provider(
        &state,
        affinity.as_deref(),
        Downstream::new(&identity.user_id, client.provider_originator()),
    )
    .await?;
    audit.set_provider(&first);
    let first_id = first.provider.id.clone();
    let response = send_upstream(
        &state,
        &first,
        path,
        headers,
        spool.body().await?,
        None,
        audit,
    )
    .await?;
    let response = track_upstream(&state, &first_id, UpstreamResponse::Live(response)).await?;
    let (lease, upstream) = if retryable(response.status()) {
        let response = finish_retry_attempt(&mut first, response).await?;
        match retry_provider(
            &state,
            affinity.as_deref(),
            &first_id,
            Downstream::new(&identity.user_id, client.provider_originator()),
        )
        .await
        {
            Some(second) => {
                drop(response);
                drop(first);
                audit.set_provider(&second);
                let second_response = send_upstream(
                    &state,
                    &second,
                    path,
                    headers,
                    spool.body().await?,
                    None,
                    audit,
                )
                .await?;
                let second_response = track_upstream(
                    &state,
                    &second.provider.id,
                    UpstreamResponse::Live(second_response),
                )
                .await?;
                (second, second_response)
            }
            None => (first, response),
        }
    } else {
        (first, response)
    };
    drop(spool);
    intercept_degraded_response(&identity, &upstream, audit)?;
    relay_response(
        &state,
        CallContext {
            model: None,
            stream: false,
            cookie_key: None,
        },
        lease,
        upstream,
        audit,
    )
    .await
}

async fn finish_retry_attempt(
    lease: &mut Lease,
    upstream: UpstreamResponse,
) -> Result<UpstreamResponse, AppError> {
    let buffered = match upstream {
        UpstreamResponse::Live(response) => BufferedUpstream {
            status: response.status(),
            version: response.version(),
            headers: response.headers().clone(),
            body: response.bytes().await?,
        },
        UpstreamResponse::Buffered(response) => response,
    };
    // INVARIANT: Never hold one provider's slot while waiting in another's queue.
    lease.release();
    Ok(UpstreamResponse::Buffered(buffered))
}

async fn retry_provider(
    state: &AppState,
    affinity: Option<&str>,
    first_id: &str,
    downstream: Downstream<'_>,
) -> Option<Lease> {
    let mut second = state
        .balancer
        .select(state, affinity, Some(first_id), downstream)
        .await
        .ok()?;
    refresh_if_needed(state, &mut second).await.ok()?;
    Some(second)
}

async fn select_ready_provider(
    state: &AppState,
    affinity: Option<&str>,
    downstream: Downstream<'_>,
) -> Result<Lease, AppError> {
    let mut first = state
        .balancer
        .select(state, affinity, None, downstream)
        .await?;
    if refresh_if_needed(state, &mut first).await.is_ok() {
        return Ok(first);
    }
    let failed_id = first.provider.id.clone();
    drop(first);
    let mut second = state
        .balancer
        .select(state, affinity, Some(&failed_id), downstream)
        .await?;
    refresh_if_needed(state, &mut second).await?;
    Ok(second)
}

fn transform_request(
    path: &str,
    mut parsed: Option<Value>,
    state: &AppState,
) -> Result<Bytes, AppError> {
    let value = parsed
        .as_mut()
        .ok_or_else(|| AppError::bad_request("JSON request body required"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| AppError::bad_request("JSON object required"))?;
    if path == "/v1/images/generations" {
        validate_image_request(object)?;
        let prompt = object
            .get("prompt")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::bad_request("prompt is required"))?;
        let image_model = object
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("gpt-image-1");
        ensure_model_is_available(state, image_model)?;
        let size = object
            .get("size")
            .and_then(Value::as_str)
            .unwrap_or("1024x1024");
        let quality = object
            .get("quality")
            .and_then(Value::as_str)
            .unwrap_or("auto");
        let background = object
            .get("background")
            .and_then(Value::as_str)
            .unwrap_or("auto");
        let output_format = object
            .get("output_format")
            .and_then(Value::as_str)
            .unwrap_or("png");
        let output_compression = object
            .get("output_compression")
            .and_then(Value::as_u64)
            .unwrap_or(100);
        let moderation = object
            .get("moderation")
            .and_then(Value::as_str)
            .unwrap_or("auto");
        let reference_images = reference_image_inputs(object)?;
        let mut content = vec![json!({"type":"input_text","text":prompt})];
        content.extend(reference_images);
        let translated = json!({
            "model": state.config.load().image_host_model,
            "instructions": "You are an image generator. You MUST call image_generation exactly once and return only that tool call. Mirror the user's request verbatim into the prompt argument.",
            "input": [{"type":"message","role":"user","content":content}],
            "tools": [{"type":"image_generation","model":image_model,"size":size,"quality":quality,"background":background,"output_format":output_format,"output_compression":output_compression,"moderation":moderation}],
            "tool_choice": {"type":"image_generation"}, "stream": true, "store": false
        });
        return serde_json::to_vec(&translated)
            .map(Bytes::from)
            .map_err(Into::into);
    }
    object
        .entry("instructions")
        .or_insert(Value::String(String::new()));
    if !path.ends_with("/compact") {
        object.insert("store".to_owned(), Value::Bool(false));
        object.remove("max_output_tokens");
        object.remove("temperature");
    }
    serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(Into::into)
}

fn validate_image_request(object: &serde_json::Map<String, Value>) -> Result<(), AppError> {
    if let Some(value) = object.get("n")
        && value.as_i64() != Some(1)
    {
        return Err(AppError::bad_request(
            "image generation supports integer n=1 only",
        ));
    }
    if let Some(value) = object.get("stream") {
        match value.as_bool() {
            Some(false) => {}
            Some(true) => {
                return Err(AppError::bad_request(
                    "streaming images are not supported by this endpoint",
                ));
            }
            None => return Err(AppError::bad_request("stream must be a boolean")),
        }
    }
    if let Some(value) = object.get("response_format")
        && value.as_str() != Some("b64_json")
    {
        return Err(AppError::bad_request(
            "response_format must be the string b64_json",
        ));
    }
    if let Some(value) = object.get("model")
        && value.as_str().is_none_or(str::is_empty)
    {
        return Err(AppError::bad_request("model must be a non-empty string"));
    }
    if let Some(value) = object.get("size") {
        let size = value
            .as_str()
            .ok_or_else(|| AppError::bad_request("size must be a string"))?;
        validate_image_size(size, object.get("model").and_then(Value::as_str))?;
    }
    validate_enum(object, "quality", &["auto", "low", "medium", "high"])?;
    validate_enum(object, "background", &["auto", "opaque", "transparent"])?;
    validate_enum(object, "output_format", &["png", "jpeg", "webp"])?;
    validate_enum(object, "moderation", &["auto", "low"])?;
    reference_image_inputs(object)?;
    if let Some(value) = object.get("output_compression") {
        let compression = value.as_i64().ok_or_else(|| {
            AppError::bad_request("output_compression must be an integer from 0 to 100")
        })?;
        if !(0..=100).contains(&compression) {
            return Err(AppError::bad_request("output_compression must be 0-100"));
        }
    }
    Ok(())
}

fn reference_image_inputs(object: &serde_json::Map<String, Value>) -> Result<Vec<Value>, AppError> {
    let Some(value) = object.get("reference_images") else {
        return Ok(Vec::new());
    };
    let images = value
        .as_array()
        .ok_or_else(|| AppError::bad_request("reference_images must be an array"))?;
    if images.len() > MAX_REFERENCE_IMAGES {
        return Err(AppError::bad_request(format!(
            "reference_images supports at most {MAX_REFERENCE_IMAGES} images"
        )));
    }
    images
        .iter()
        .map(|image| {
            let image_url = image
                .as_str()
                .ok_or_else(|| AppError::bad_request("reference_images must contain strings"))?;
            validate_reference_image_url(image_url)?;
            Ok(json!({
                "type": "input_image",
                "image_url": image_url,
                "detail": "auto"
            }))
        })
        .collect()
}

fn validate_reference_image_url(image_url: &str) -> Result<(), AppError> {
    let (metadata, encoded) = image_url
        .split_once(";base64,")
        .ok_or_else(|| AppError::bad_request("reference images must be base64 data URLs"))?;
    let mime = metadata.strip_prefix("data:").filter(|mime| {
        matches!(
            *mime,
            "image/png" | "image/jpeg" | "image/jpg" | "image/webp" | "image/gif"
        )
    });
    if mime.is_none() || encoded.is_empty() {
        return Err(AppError::bad_request(
            "reference images must be PNG, JPEG, WEBP, or GIF data URLs",
        ));
    }
    Ok(())
}

fn validate_image_size(size: &str, model: Option<&str>) -> Result<(), AppError> {
    if matches!(size, "auto" | "1024x1024" | "1536x1024" | "1024x1536") {
        return Ok(());
    }
    if model != Some("gpt-image-2") {
        return Err(AppError::bad_request(
            "custom image sizes require model gpt-image-2",
        ));
    }
    let (width, height) = size
        .split_once('x')
        .ok_or_else(|| AppError::bad_request("size must use WIDTHxHEIGHT"))?;
    let width = width
        .parse::<u64>()
        .map_err(|_| AppError::bad_request("size must use WIDTHxHEIGHT"))?;
    let height = height
        .parse::<u64>()
        .map_err(|_| AppError::bad_request("size must use WIDTHxHEIGHT"))?;
    if width > 3_840 || height > 3_840 || width % 16 != 0 || height % 16 != 0 {
        return Err(AppError::bad_request(
            "size dimensions must be multiples of 16 and no larger than 3840",
        ));
    }
    let pixels = width * height;
    if !(655_360..=8_294_400).contains(&pixels) || width.max(height) > 3 * width.min(height) {
        return Err(AppError::bad_request(
            "size must contain 655360-8294400 pixels with an aspect ratio no wider than 3:1",
        ));
    }
    Ok(())
}

fn validate_enum(
    object: &serde_json::Map<String, Value>,
    field: &str,
    allowed: &[&str],
) -> Result<(), AppError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    let value = value
        .as_str()
        .ok_or_else(|| AppError::bad_request(format!("{field} must be a string")))?;
    if !allowed.contains(&value) {
        return Err(AppError::bad_request(format!("unsupported {field}")));
    }
    Ok(())
}

struct AffinityKey {
    source: &'static str,
    value: String,
}

fn affinity_key(headers: &HeaderMap, _: Option<&Value>) -> Option<AffinityKey> {
    session_id(headers).map(|value| AffinityKey {
        source: "session-id",
        value,
    })
}

async fn refresh_if_needed(state: &AppState, lease: &mut Lease) -> Result<(), AppError> {
    if lease
        .provider
        .expires_at
        .is_none_or(|expires| expires > chrono::Utc::now().timestamp() + 60)
    {
        return Ok(());
    }
    let lock = state
        .refresh_locks
        .entry(lease.provider.id.clone())
        .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let _guard = lock.lock().await;
    let current: (String, String, Option<i64>, String) = sqlx::query_as(
        "SELECT access_token,refresh_token,expires_at,account_id FROM providers WHERE id=? AND is_deleted=0",
    )
    .bind(&lease.provider.id)
    .fetch_one(&state.db)
    .await?;
    let now = chrono::Utc::now().timestamp();
    if current.2.is_some_and(|expires| expires > now + 60) {
        lease.access_token = current.0;
        lease.refresh_token = current.1;
        lease.provider.expires_at = current.2;
        lease.provider.account_id = current.3;
        return Ok(());
    }
    let refresh_token = current.1.clone();
    let token = match oauth::refresh_with_client(state, Some(&lease.provider), &refresh_token).await
    {
        Ok(token) => token,
        Err(error) => {
            {
                let _write = state.write_gate.lock().await;
                sqlx::query("UPDATE providers SET status='auth_error',last_error='credential refresh failed',updated_at=? WHERE id=? AND refresh_token=? AND is_deleted=0")
                    .bind(now).bind(&lease.provider.id).bind(&current.1).execute(&state.db).await?;
            }
            state.balancer.reload_providers(&state.db).await?;
            return Err(error);
        }
    };
    let account_id = oauth::account_id_from_jwt(&token.access_token)
        .unwrap_or_else(|_| lease.provider.account_id.clone());
    let expires_at = now + token.expires_in;
    let updated = {
        let _write = state.write_gate.lock().await;
        sqlx::query("UPDATE providers SET access_token=?,refresh_token=?,account_id=?,expires_at=?,status='active',last_error=NULL,updated_at=? WHERE id=? AND refresh_token=? AND is_deleted=0")
            .bind(&token.access_token)
            .bind(&token.refresh_token)
            .bind(&account_id).bind(expires_at).bind(now).bind(&lease.provider.id).bind(&current.1).execute(&state.db).await?
    };
    if updated.rows_affected() == 0 {
        return Err(AppError::unavailable(
            "provider credential changed during refresh",
        ));
    }
    lease.access_token = token.access_token;
    lease.refresh_token = token.refresh_token;
    lease.provider.account_id = account_id;
    lease.provider.expires_at = Some(expires_at);
    state.balancer.reload_providers(&state.db).await?;
    Ok(())
}

fn upstream_cookie_key(lease: &Lease, affinity: Option<&str>) -> Option<String> {
    affinity.map(|affinity| format!("{}:{}", lease.provider.id, affinity_hash(affinity)))
}

fn upstream_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            value
                .split(';')
                .next()
                .filter(|cookie| cookie.starts_with("__oailb="))
                .map(str::to_owned)
        })
}

fn store_upstream_cookie(state: &AppState, key: Option<&str>, headers: &HeaderMap) {
    let Some(key) = key else {
        return;
    };
    let Some(cookie) = upstream_cookie(headers) else {
        return;
    };
    state.upstream_cookies.insert(
        key.to_owned(),
        UpstreamCookie {
            cookie,
            expires_at: chrono::Utc::now().timestamp() + 3600,
        },
    );
}

async fn send_upstream(
    state: &AppState,
    lease: &Lease,
    path: &str,
    inbound: &HeaderMap,
    body: reqwest::Body,
    affinity: Option<&str>,
    audit: &mut AuditTracker,
) -> Result<reqwest::Response, AppError> {
    if path == "/v1/audio/transcriptions" {
        let mut request = transcription_request(state, lease, inbound)?
            .body(body)
            .build()?;
        let user_agent = state
            .config
            .load()
            .upstream_user_agent_for(&lease.provider.originator)
            .map(str::to_owned)
            .unwrap_or_else(|| TRANSCRIPTION_USER_AGENT.to_owned());
        apply_upstream_identity_headers(
            request.headers_mut(),
            &lease.provider,
            Some(&user_agent),
            &lease.provider.originator,
        )?;
        audit.set_upstream_request_headers(request.headers());
        return Ok(state
            .provider_client(&lease.provider)?
            .execute(request)
            .await?);
    }
    let suffix = match path {
        "/v1/responses/compact" | "/backend-api/codex/responses/compact" => "/responses/compact",
        _ => "/responses",
    };
    let client = state.provider_client(&lease.provider)?;
    let mut request = client.post(format!("{}{}", state.config.load().upstream_base, suffix));
    // INVARIANT: A provider authorized for the Pi identity presents exactly the request headers Pi
    // Agent sends. Every other header the caller supplied is dropped rather than forwarded, so the
    // upstream can never observe a Pi-identified request that Pi itself could not have produced.
    if lease.provider.originator == identity::PI_ORIGINATOR {
        request = pi_upstream_request(request, path, inbound)?;
    } else {
        let filter_codex_turn_state_312 =
            state.config.load().experimental_filter_codex_turn_state_312;
        // INVARIANT: x-codex-turn-state is owned by the client for one turn, not by
        // session affinity. Forward it only when supplied, without caching or adding
        // it: absence can mark a new turn, where replaying an old token breaks routing.
        for (name, value) in inbound {
            if filter_codex_turn_state_312
                && name == "x-codex-turn-state"
                && value.as_bytes().len() == DEGRADED_TURN_STATE_LENGTH
            {
                continue;
            }
            if should_forward_request_header(path, name) {
                request = request.header(name, value);
            }
        }
    }
    audit.set_upstream_accept_encoding(inbound);
    if let Some(key) = upstream_cookie_key(lease, affinity)
        && let Some(entry) = state.upstream_cookies.get(&key)
    {
        let now = chrono::Utc::now().timestamp();
        if entry.expires_at > now {
            request = request.header(header::COOKIE, &entry.cookie);
        } else {
            drop(entry);
            state.upstream_cookies.remove(&key);
        }
    }
    request = request
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", lease.access_token),
        )
        .header("chatgpt-account-id", &lease.provider.account_id);
    request = request.header(header::CONTENT_TYPE, "application/json");
    if let Some(openai_beta) = state.config.load().upstream_openai_beta.clone() {
        request = request.header("OpenAI-Beta", openai_beta);
    }
    let mut request = request.body(body).build()?;
    // INVARIANT: Pi always sends a User-Agent, so a Pi-identified upstream request must carry one
    // even when no operator override is configured. `upstream_user_agent` covers the override and
    // the cross-family fallback default; the remaining case is a Pi client served by a Pi provider,
    // whose own user agent the identity check already confirmed is Pi-shaped.
    let user_agent = lease
        .upstream_user_agent(&state.config.load())
        .map(str::to_owned)
        .or_else(|| {
            (lease.provider.originator == identity::PI_ORIGINATOR)
                .then(|| header_value(inbound, header::USER_AGENT))
                .flatten()
        });
    apply_upstream_identity_headers(
        request.headers_mut(),
        &lease.provider,
        user_agent.as_deref(),
        &lease.provider.originator,
    )?;
    audit.set_upstream_request_headers(request.headers());
    Ok(client.execute(request).await?)
}

async fn send_transcription_upstream(
    state: &AppState,
    lease: &Lease,
    inbound: &HeaderMap,
    body: reqwest::Body,
) -> Result<reqwest::Response, AppError> {
    let mut request = transcription_request(state, lease, inbound)?
        .body(body)
        .build()?;
    let user_agent = state
        .config
        .load()
        .upstream_user_agent_for(&lease.provider.originator)
        .map(str::to_owned)
        .unwrap_or_else(|| TRANSCRIPTION_USER_AGENT.to_owned());
    apply_upstream_identity_headers(
        request.headers_mut(),
        &lease.provider,
        Some(&user_agent),
        &lease.provider.originator,
    )?;
    Ok(state
        .provider_client(&lease.provider)?
        .execute(request)
        .await?)
}

fn transcription_request(
    state: &AppState,
    lease: &Lease,
    inbound: &HeaderMap,
) -> Result<reqwest::RequestBuilder, AppError> {
    let upstream = state.config.load();
    let endpoint = format!(
        "{}/transcribe",
        upstream.upstream_base.trim_end_matches("/codex")
    );
    drop(upstream);
    let mut request = state.provider_client(&lease.provider)?.post(endpoint);
    for (name, value) in inbound {
        // INVARIANT: this endpoint authenticates the desktop client itself, so the desktop User-Agent
        // replaces the caller's instead of being appended next to it.
        if name == header::USER_AGENT {
            continue;
        }
        if should_forward_request_header("/v1/audio/transcriptions", name) {
            request = request.header(name, value);
        }
    }
    Ok(request
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", lease.access_token),
        )
        .header("chatgpt-account-id", &lease.provider.account_id)
        .header(header::USER_AGENT, TRANSCRIPTION_USER_AGENT))
}

const CODEX_TURN_METADATA: &str = "x-codex-turn-metadata";
const CODEX_INSTALLATION_ID: &str = "installation_id";

fn apply_upstream_identity_headers(
    headers: &mut HeaderMap,
    provider: &Provider,
    user_agent: Option<&str>,
    originator: &str,
) -> Result<(), AppError> {
    if let Some(user_agent) = user_agent {
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_str(user_agent)
                .map_err(|_| AppError::internal("invalid configured upstream User-Agent"))?,
        );
    }
    // INVARIANT: The originator belongs to the provider, never to the downstream client. Inserting
    // instead of appending keeps exactly one value, so the upstream identity always equals the
    // client family the provider's credentials were authorized for.
    headers.insert(
        ORIGINATOR_HEADER,
        HeaderValue::from_str(originator)
            .map_err(|_| AppError::internal("invalid provider originator"))?,
    );
    let Some(metadata) = headers.get(CODEX_TURN_METADATA) else {
        return Ok(());
    };
    let Some(rewritten) = rewrite_installation_id(metadata.as_bytes(), &provider.id) else {
        return Ok(());
    };
    headers.insert(
        HeaderName::from_static(CODEX_TURN_METADATA),
        HeaderValue::from_bytes(&rewritten)
            .map_err(|_| AppError::internal("Codex turn metadata is not a header value"))?,
    );
    Ok(())
}

/// Replaces every occurrence of the `installation_id` value with the selected
/// provider ID, so the client's key order, spacing, and unknown fields survive.
///
/// Returns `None` when the metadata names no non-empty `installation_id` (not JSON,
/// key absent, or a non-string value); the caller then forwards the client's header
/// as is instead of rejecting an otherwise valid request.
fn rewrite_installation_id(metadata: &[u8], provider_id: &str) -> Option<Vec<u8>> {
    // INVARIANT: providers.id is a server-generated UUID (see `create_provider`), so a
    // raw replacement can never introduce JSON escaping.
    let document = std::str::from_utf8(metadata).ok()?;
    let parsed = serde_json::from_str::<Value>(document).ok()?;
    let installation_id = parsed.get(CODEX_INSTALLATION_ID)?.as_str()?;
    if installation_id.is_empty() {
        return None;
    }
    Some(document.replace(installation_id, provider_id).into_bytes())
}

fn header_value(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn should_forward_request_header(path: &str, name: &HeaderName) -> bool {
    let lower = name.as_str().to_ascii_lowercase();
    if path.starts_with("/v1/realtime") && lower == "x-session-id" {
        return true;
    }
    if lower == "authorization"
        || HOP_HEADERS.contains(&lower.as_str())
        || PROXY_ONLY_HEADERS.contains(&lower.as_str())
    {
        return false;
    }
    let common = matches!(
        lower.as_str(),
        "accept"
            | "accept-encoding"
            | "user-agent"
            | "session-id"
            | "thread-id"
            | "x-client-request-id"
    ) || lower.starts_with("x-openai-")
        || lower.starts_with("x-codex-");
    if path == "/v1/audio/transcriptions" {
        return common || lower == "content-type";
    }
    common || lower.starts_with("openai-")
}

/// Decodes a compressed request body so the proxy can validate and transform it.
///
/// Pi Agent compresses every Codex request body with zstd, so a body left untouched here could
/// never be parsed for the model, quota and audit fields the proxy derives from it.
fn decode_request_body(headers: &HeaderMap, body: Bytes) -> Result<Bytes, AppError> {
    let encoding = headers
        .get(header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("identity"));
    let Some(encoding) = encoding else {
        return Ok(body);
    };
    if !encoding.eq_ignore_ascii_case(PI_CONTENT_ENCODING) {
        return Err(AppError::bad_request_with_reason(
            format!("unsupported request content-encoding: {encoding}"),
            "downstream_content_encoding_invalid",
        ));
    }
    let mut decoder = std::io::Read::take(
        zstd::stream::read::Decoder::new(body.as_ref()).map_err(|_| undecodable_body())?,
        MAX_DECODED_REQUEST_BYTES + 1,
    );
    let mut decoded = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut decoded).map_err(|_| undecodable_body())?;
    if decoded.len() as u64 > MAX_DECODED_REQUEST_BYTES {
        return Err(AppError::bad_request_with_reason(
            "request body exceeds the decompressed size limit",
            "downstream_body_too_large",
        ));
    }
    Ok(Bytes::from(decoded))
}

fn undecodable_body() -> AppError {
    AppError::bad_request_with_reason(
        "request body is not valid zstd",
        "downstream_content_encoding_invalid",
    )
}

/// Body the selected provider receives.
///
/// A Pi-identified provider must observe the request Pi sends, and Pi always compresses its Codex
/// request bodies with zstd, so the proxy re-encodes the transformed body instead of forwarding an
/// uncompressed one next to a `content-encoding` header.
fn upstream_request_body(lease: &Lease, path: &str, body: &Bytes) -> Result<Bytes, AppError> {
    if lease.provider.originator != identity::PI_ORIGINATOR || !identity::is_pi_responses_path(path)
    {
        return Ok(body.clone());
    }
    zstd::stream::encode_all(body.as_ref(), PI_BODY_COMPRESSION_LEVEL)
        .map(Bytes::from)
        .map_err(|error| {
            tracing::error!(%error, "failed to compress a Pi request body");
            AppError::internal("failed to encode the request body")
        })
}

/// Builds the upstream request headers for a provider authorized as Pi Agent.
///
/// Pi Agent sends a closed set of headers on a Codex request: the identity headers OpenAI-LB owns
/// (`authorization`, `chatgpt-account-id`, `originator`, `user-agent`), the protocol headers below,
/// and its session id. Nothing else is forwarded, so headers Pi never sends — `thread-id`,
/// `x-codex-*`, `x-openai-*` and anything unrecognized — cannot reach the upstream.
fn pi_upstream_request(
    mut request: reqwest::RequestBuilder,
    path: &str,
    inbound: &HeaderMap,
) -> Result<reqwest::RequestBuilder, AppError> {
    for &name in PI_PASSTHROUGH_HEADERS {
        if let Some(value) = inbound.get(name) {
            request = request.header(name, value);
        }
    }
    // INVARIANT: Pi sends one session id as both `session-id` and `x-client-request-id`. The proxy
    // forwards the caller's own id for the former and derives the latter from it, so the two never
    // disagree upstream the way a substituted value would. A Pi-identified request was validated to
    // carry a UUIDv7 before it got here, and a cross-originator fallback request keeps its own id:
    // OpenAI-LB fabricates no Pi session id. The only proxy-minted id is the one-off image
    // generation id from `upstream_request_headers`, which arrives without a caller value.
    let session_id = session_id(inbound).ok_or_else(|| {
        AppError::bad_request_with_reason(
            "session-id request header is required",
            "downstream_session_id_missing",
        )
    })?;
    request = request
        .header("session-id", &session_id)
        .header("x-client-request-id", &session_id);
    if identity::is_pi_responses_path(path) {
        request = request
            .header(header::ACCEPT, "text/event-stream")
            .header("accept-language", "*")
            .header("sec-fetch-mode", "cors")
            .header("content-encoding", PI_CONTENT_ENCODING)
            .header("OpenAI-Beta", PI_OPENAI_BETA);
    }
    Ok(request)
}

fn retryable(status: StatusCode) -> bool {
    matches!(status.as_u16(), 401 | 403 | 429) || status.is_server_error()
}

/// True when an upstream response carries the degradation signal: an
/// `x-codex-turn-state` header of exactly [`DEGRADED_TURN_STATE_LENGTH`] bytes.
fn degradation_signal(headers: &HeaderMap) -> bool {
    headers
        .get("x-codex-turn-state")
        .is_some_and(|value| value.as_bytes().len() == DEGRADED_TURN_STATE_LENGTH)
}

/// Applies the Consumer-level degradation interception.
///
/// INVARIANT: only successful upstream responses are intercepted; an upstream error keeps
/// its own status so the caller still sees the real failure. Returning `Err` drops the
/// upstream response before its body is read, cancelling the upstream request.
fn intercept_degraded_response(
    identity: &ApiIdentity,
    upstream: &UpstreamResponse,
    audit: &mut AuditTracker,
) -> Result<(), AppError> {
    if !identity.intercept_degradation
        || !upstream.status().is_success()
        || !degradation_signal(upstream.headers())
    {
        return Ok(());
    }
    audit.set_error_code(DEGRADATION_INTERCEPTED_CODE);
    Err(AppError::unavailable_with_reason(
        DEGRADATION_INTERCEPTED_MESSAGE,
        DEGRADATION_INTERCEPTED_CODE,
    ))
}

async fn track_upstream(
    state: &AppState,
    provider_id: &str,
    upstream: UpstreamResponse,
) -> Result<UpstreamResponse, AppError> {
    if upstream.status() != StatusCode::TOO_MANY_REQUESTS {
        track_response(state, provider_id, upstream.status(), &[]).await?;
        return Ok(upstream);
    }
    let buffered = match upstream {
        UpstreamResponse::Live(response) => BufferedUpstream {
            status: response.status(),
            version: response.version(),
            headers: response.headers().clone(),
            body: response.bytes().await?,
        },
        UpstreamResponse::Buffered(response) => response,
    };
    track_response(state, provider_id, buffered.status, &buffered.body).await?;
    Ok(UpstreamResponse::Buffered(buffered))
}

async fn relay_response(
    state: &AppState,
    context: CallContext,
    lease: Lease,
    upstream: UpstreamResponse,
    audit: &mut AuditTracker,
) -> Result<Response, AppError> {
    audit.mark_first_byte();
    let status = upstream.status();
    audit.set_upstream_http_version(upstream.version());
    audit.set_compression_headers(&HeaderMap::new(), Some(upstream.headers()));
    audit.set_response_headers(upstream.headers());
    store_upstream_cookie(state, context.cookie_key.as_deref(), upstream.headers());
    let mut headers = filtered_response_headers(upstream.headers());
    let is_sse = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"))
        });
    let is_stream =
        context.stream && status.is_success() && matches!(&upstream, UpstreamResponse::Live(_));
    if !is_stream {
        let bytes = upstream.into_bytes().await?;
        audit.set_response_size(bytes.len() as i64);
        audit.set_response_body(&bytes, false);
        let usage = if is_sse {
            audit.inspect_sse_body(&bytes)
        } else {
            audit.inspect_json_body(&bytes)
        };
        let audit_id = audit.finish(
            status,
            context.model.as_deref(),
            usage,
            error_from(status, &bytes).as_deref(),
        );
        let mut response = build_response(status, headers, Body::from(bytes))?;
        if let Some(id) = audit_id {
            response.extensions_mut().insert(id);
        }
        return Ok(response);
    }
    if !headers.iter().any(|(name, _)| name == header::CONTENT_TYPE) {
        headers.push((
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        ));
    }
    let audit_id = audit.event.as_ref().map(|event| AuditTransportId {
        id: event.id.clone(),
        diagnostics_enabled: event.request_archive,
    });
    let completion = audit.take_stream(status, context.model.as_deref());
    let UpstreamResponse::Live(upstream) = upstream else {
        unreachable!("only successful live upstream responses stream")
    };
    let mut stream = upstream.bytes_stream();
    let output = async_stream::stream! {
        let mut completion = completion;
        let mut stream_failed = false;
        while let Some(item) = stream.next().await {
            match item {
                Ok(bytes) => {
                    completion.capture_sse(&bytes);
                    yield Ok::<Bytes, Infallible>(bytes);
                }
                Err(error) => {
                    tracing::warn!(%error, "upstream stream ended with error");
                    stream_failed = true;
                    break;
                }
            }
        }
        completion.finish(stream_failed);
        drop(lease);
    };
    let mut response = build_response(status, headers, Body::from_stream(output))?;
    if let Some(id) = audit_id {
        response.extensions_mut().insert(id);
    }
    Ok(response)
}

async fn image_response(
    state: &AppState,
    context: CallContext,
    lease: Lease,
    upstream: UpstreamResponse,
    audit: &mut AuditTracker,
) -> Result<Response, AppError> {
    audit.mark_first_byte();
    let status = upstream.status();
    audit.set_upstream_http_version(upstream.version());
    if !status.is_success() {
        return relay_response(state, context, lease, upstream, audit).await;
    }
    audit.set_response_headers(upstream.headers());
    let bytes = upstream.into_bytes().await?;
    audit.inspect_sse_body(&bytes);
    let (images, usage) = images_from_sse(&bytes)?;
    let envelope = serde_json::to_vec(
        &json!({"created": chrono::Utc::now().timestamp(), "data": images, "usage": usage}),
    )?;
    audit.set_response_size(envelope.len() as i64);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    audit.set_response_headers(&headers);
    audit.set_response_body(&envelope, false);
    let audit_id = audit.finish(status, context.model.as_deref(), usage, None);
    let mut response = build_response(
        StatusCode::OK,
        vec![(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        Body::from(envelope),
    )?;
    if let Some(id) = audit_id {
        response.extensions_mut().insert(id);
    }
    Ok(response)
}

fn images_from_sse(bytes: &[u8]) -> Result<(Vec<Value>, Usage), AppError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AppError::upstream(502, "invalid image event stream"))?;
    let mut items = Vec::new();
    let mut usage = Usage::default();
    let mut event_count = 0;
    let mut last_event = None;
    let mut upstream_detail = None;
    let mut upstream_text = None;
    for line in text.lines().filter_map(|line| line.strip_prefix("data: ")) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        event_count += 1;
        last_event = event
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or(last_event);
        if upstream_detail.is_none() {
            upstream_detail = image_event_detail(&event);
        }
        if upstream_text.is_none() {
            upstream_text = image_event_text(&event);
        }
        usage.merge(usage_from_value(&event));
        if event.get("type").and_then(Value::as_str) == Some("response.output_item.done")
            && let Some(item) = event.get("item").filter(|item| {
                item.get("type").and_then(Value::as_str) == Some("image_generation_call")
            })
        {
            items.push(item.clone());
        }
        if event.get("type").and_then(Value::as_str) == Some("response.completed")
            && items.is_empty()
            && let Some(output) = event.pointer("/response/output").and_then(Value::as_array)
        {
            items.extend(
                output
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(Value::as_str) == Some("image_generation_call")
                    })
                    .cloned(),
            );
        }
    }
    let data = items
        .into_iter()
        .filter_map(|item| {
            item.get("result").and_then(Value::as_str).map(
                |result| json!({"b64_json":result,"revised_prompt":item.get("revised_prompt")}),
            )
        })
        .collect::<Vec<_>>();
    if data.is_empty() {
        let detail = upstream_detail
            .map(|detail| format!(" Upstream reason: {detail}."))
            .unwrap_or_default();
        let response_text = upstream_text
            .map(|text| format!(" Upstream response: {text}."))
            .unwrap_or_default();
        let event_summary = last_event
            .map(|event| {
                format!(" The last upstream event was {event}; the response contained {event_count} readable event(s).")
            })
            .unwrap_or_else(|| " The upstream response contained no readable image events.".to_owned());
        return Err(AppError::upstream_with_reason(
            502,
            "image_data_missing",
            format!(
                "Image generation failed: the upstream provider did not return image data.{detail}{response_text}{event_summary}"
            ),
        ));
    }
    Ok((data, usage))
}

fn image_event_detail(event: &Value) -> Option<String> {
    [
        "/error/message",
        "/response/error/message",
        "/response/incomplete_details/reason",
        "/item/error/message",
        "/item/failure/message",
        "/message",
    ]
    .iter()
    .find_map(|pointer| event.pointer(pointer).and_then(Value::as_str))
    .or_else(|| event.pointer("/item/refusal").and_then(Value::as_str))
    .or_else(|| {
        event
            .pointer("/response/output")
            .and_then(Value::as_array)
            .and_then(|output| {
                output.iter().find_map(|item| {
                    item.get("refusal")
                        .and_then(Value::as_str)
                        .or_else(|| item.pointer("/error/message").and_then(Value::as_str))
                })
            })
    })
    .map(str::to_owned)
    .or_else(|| event.get("item").and_then(image_call_detail))
    .map(|detail| concise_image_detail(&detail))
}

fn image_call_detail(item: &Value) -> Option<String> {
    if item.get("type").and_then(Value::as_str) != Some("image_generation_call") {
        return None;
    }
    let status = item.get("status").and_then(Value::as_str);
    let result = match item.get("result") {
        None => "missing",
        Some(Value::Null) => "null",
        Some(Value::String(value)) if value.is_empty() => "empty",
        Some(Value::String(_)) => "present",
        Some(_) => "not a string",
    };
    let text = response_text(item);
    if let Some(text) = text {
        return Some(format!("image_generation_call response: {text}"));
    }
    let fields = item
        .as_object()
        .map(|object| {
            object
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    Some(
        status
            .map(|status| format!("image_generation_call status={status}; result={result}"))
            .unwrap_or_else(|| {
                format!(
                    "image_generation_call did not return usable image data; result={result}; fields={fields}"
                )
            }),
    )
}

fn image_event_text(event: &Value) -> Option<String> {
    event
        .pointer("/response/output")
        .or_else(|| event.get("output"))
        .or_else(|| event.get("text"))
        .and_then(response_text)
        .or_else(|| {
            event
                .get("item")
                .filter(|item| {
                    item.get("type").and_then(Value::as_str) != Some("image_generation_call")
                })
                .and_then(response_text)
        })
}

fn response_text(value: &Value) -> Option<String> {
    let mut texts = Vec::new();
    collect_response_text(value, &mut texts);
    (!texts.is_empty()).then(|| concise_image_detail(&texts.join(" ")))
}

fn collect_response_text(value: &Value, texts: &mut Vec<String>) {
    match value {
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_response_text(value, texts)),
        Value::Object(object) => object.iter().for_each(|(key, value)| {
            if matches!(
                key.as_str(),
                "text" | "refusal" | "message" | "reason" | "detail"
            ) && let Some(text) = value.as_str().filter(|text| !text.trim().is_empty())
            {
                texts.push(text.to_owned());
            } else if !matches!(key.as_str(), "result" | "b64_json" | "data")
                && matches!(value, Value::Object(_) | Value::Array(_))
            {
                collect_response_text(value, texts);
            }
        }),
        Value::String(text) if !text.trim().is_empty() => texts.push(text.clone()),
        _ => {}
    }
}

fn concise_image_detail(detail: &str) -> String {
    detail
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect()
}

fn filtered_response_headers(headers: &HeaderMap) -> Vec<(HeaderName, HeaderValue)> {
    // INVARIANT: Preserve x-codex-turn-state for the client, which alone owns its
    // turn lifetime and decides whether to send it on the next request.
    headers
        .iter()
        .filter(|(name, _)| {
            let lower = name.as_str().to_ascii_lowercase();
            !HOP_HEADERS.contains(&lower.as_str()) && lower != "x-request-id"
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

struct StreamCompletion {
    permit: Option<AuditReservation>,
    archive_budget: Option<OwnedSemaphorePermit>,
    event: Option<AuditEvent>,
    started: Instant,
    response_preview: StreamingPreview,
    response_bytes: i64,
    sse: SseObservation,
}

impl StreamCompletion {
    fn new(
        permit: Option<AuditReservation>,
        archive_budget: Option<OwnedSemaphorePermit>,
        event: Option<AuditEvent>,
        started: Instant,
    ) -> Self {
        Self {
            permit,
            archive_budget,
            event,
            started,
            response_preview: StreamingPreview::default(),
            response_bytes: 0,
            sse: SseObservation::default(),
        }
    }

    fn capture_sse(&mut self, bytes: &[u8]) {
        let Some(request_archive) = self.event.as_ref().map(|event| event.request_archive) else {
            return;
        };
        self.response_bytes += bytes.len() as i64;
        self.sse.capture(bytes);
        if let Some(event) = &mut self.event {
            event.upstream_model.clone_from(&self.sse.upstream_model);
        }
        if let (Some(event), Some(failure)) = (&mut self.event, &self.sse.failure) {
            event.error_code = failure.code.clone();
            event.error = Some(failure.message.clone());
        }
        if request_archive {
            self.response_preview.capture(bytes);
        }
    }

    fn finish(&mut self, failed: bool) {
        let Some(mut event) = self.event.take() else {
            return;
        };
        let status = if failed { 502 } else { event.status };
        let error = failed.then_some("upstream_stream_error");
        let model = event.model.clone();
        event.response_body = Some(std::mem::take(&mut self.response_preview.body));
        event.response_body_truncated = self.response_preview.truncated || failed;
        event.response_bytes = self.response_bytes;
        settle(
            &mut event,
            status,
            model.as_deref(),
            self.sse.usage,
            error,
            self.started,
        );
        self.permit
            .take()
            .expect("audit queue capacity is reserved once")
            .send(event, self.archive_budget.take());
    }
}

impl Drop for StreamCompletion {
    fn drop(&mut self) {
        if let Some(mut event) = self.event.take() {
            let model = event.model.clone();
            event.response_body = Some(std::mem::take(&mut self.response_preview.body));
            event.response_body_truncated = true;
            event.response_bytes = self.response_bytes;
            settle(
                &mut event,
                499,
                model.as_deref(),
                self.sse.usage,
                Some("client_cancelled"),
                self.started,
            );
            self.permit
                .take()
                .expect("audit queue capacity is reserved once")
                .send(event, self.archive_budget.take());
        }
    }
}

fn build_response(
    status: StatusCode,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: Body,
) -> Result<Response, AppError> {
    let mut response = Response::builder().status(status);
    for (name, value) in headers {
        response = response.header(name, value);
    }
    response
        .body(body)
        .map_err(|_| AppError::internal("failed to build response"))
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
struct Usage {
    input_tokens: i64,
    output_tokens: i64,
    cached_tokens: i64,
    cache_write_tokens: i64,
}

impl Usage {
    fn merge(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.max(other.input_tokens);
        self.output_tokens = self.output_tokens.max(other.output_tokens);
        self.cached_tokens = self.cached_tokens.max(other.cached_tokens);
        self.cache_write_tokens = self.cache_write_tokens.max(other.cache_write_tokens);
    }
}

fn usage_from_value(value: &Value) -> Usage {
    let usage = value
        .get("usage")
        .or_else(|| value.pointer("/response/usage"));
    Usage {
        input_tokens: usage
            .and_then(|v| v.get("input_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        output_tokens: usage
            .and_then(|v| v.get("output_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        cached_tokens: usage
            .and_then(|v| v.pointer("/input_tokens_details/cached_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        cache_write_tokens: usage
            .and_then(|v| v.pointer("/input_tokens_details/cache_write_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or_default(),
    }
}

const LONG_CONTEXT_THRESHOLD_TOKENS: i64 = 272_000;

#[derive(Clone, Copy)]
struct TokenRates {
    input_usd_nanos: i64,
    cached_input_usd_nanos: i64,
    cache_write_usd_nanos: i64,
    output_usd_nanos: i64,
}

#[derive(Clone, Copy)]
struct TokenPricing {
    model: &'static str,
    short: TokenRates,
    long: Option<TokenRates>,
}

#[derive(serde::Serialize)]
pub struct ModelPriceRates {
    pub input_usd_nanos: i64,
    pub cached_input_usd_nanos: Option<i64>,
    pub cache_write_usd_nanos: Option<i64>,
    pub output_usd_nanos: i64,
}

#[derive(serde::Serialize)]
pub struct ModelPrice {
    pub model: &'static str,
    pub short: ModelPriceRates,
    pub long: Option<ModelPriceRates>,
}

impl TokenPricing {
    const fn rates(self, input_tokens: i64) -> TokenRates {
        if input_tokens >= LONG_CONTEXT_THRESHOLD_TOKENS {
            match self.long {
                Some(rates) => rates,
                None => self.short,
            }
        } else {
            self.short
        }
    }
}

// INVARIANT: These standard token prices are release-owned. Their computed value
// is stored with the audit event, so later price-table updates cannot rewrite history.
const TOKEN_PRICING: &[TokenPricing] = &[
    TokenPricing {
        model: "gpt-6-astra",
        short: TokenRates {
            input_usd_nanos: 10_000,
            cached_input_usd_nanos: 1_000,
            cache_write_usd_nanos: 12_500,
            output_usd_nanos: 50_000,
        },
        long: Some(TokenRates {
            input_usd_nanos: 20_000,
            cached_input_usd_nanos: 2_000,
            cache_write_usd_nanos: 25_000,
            output_usd_nanos: 75_000,
        }),
    },
    TokenPricing {
        model: "gpt-6-sol",
        short: TokenRates {
            input_usd_nanos: 2_000,
            cached_input_usd_nanos: 200,
            cache_write_usd_nanos: 2_500,
            output_usd_nanos: 10_000,
        },
        long: Some(TokenRates {
            input_usd_nanos: 4_000,
            cached_input_usd_nanos: 400,
            cache_write_usd_nanos: 5_000,
            output_usd_nanos: 15_000,
        }),
    },
    TokenPricing {
        model: "gpt-6-luna",
        short: TokenRates {
            input_usd_nanos: 100,
            cached_input_usd_nanos: 10,
            cache_write_usd_nanos: 125,
            output_usd_nanos: 500,
        },
        long: Some(TokenRates {
            input_usd_nanos: 200,
            cached_input_usd_nanos: 20,
            cache_write_usd_nanos: 250,
            output_usd_nanos: 750,
        }),
    },
    TokenPricing {
        model: "gpt-5.6-sol",
        short: TokenRates {
            input_usd_nanos: 4_000,
            cached_input_usd_nanos: 400,
            cache_write_usd_nanos: 5_000,
            output_usd_nanos: 20_000,
        },
        long: Some(TokenRates {
            input_usd_nanos: 8_000,
            cached_input_usd_nanos: 800,
            cache_write_usd_nanos: 10_000,
            output_usd_nanos: 30_000,
        }),
    },
    TokenPricing {
        model: "gpt-5.6-terra",
        short: TokenRates {
            input_usd_nanos: 2_000,
            cached_input_usd_nanos: 200,
            cache_write_usd_nanos: 2_500,
            output_usd_nanos: 12_000,
        },
        long: Some(TokenRates {
            input_usd_nanos: 4_000,
            cached_input_usd_nanos: 400,
            cache_write_usd_nanos: 5_000,
            output_usd_nanos: 18_000,
        }),
    },
    TokenPricing {
        model: "gpt-5.6-luna",
        short: TokenRates {
            input_usd_nanos: 200,
            cached_input_usd_nanos: 20,
            cache_write_usd_nanos: 250,
            output_usd_nanos: 1_200,
        },
        long: Some(TokenRates {
            input_usd_nanos: 400,
            cached_input_usd_nanos: 40,
            cache_write_usd_nanos: 500,
            output_usd_nanos: 1_800,
        }),
    },
    TokenPricing {
        model: "gpt-5.6-cyber",
        short: TokenRates {
            input_usd_nanos: 12_500,
            cached_input_usd_nanos: 1_250,
            cache_write_usd_nanos: 15_625,
            output_usd_nanos: 75_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5.5",
        short: TokenRates {
            input_usd_nanos: 5_000,
            cached_input_usd_nanos: 500,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 30_000,
        },
        long: Some(TokenRates {
            input_usd_nanos: 10_000,
            cached_input_usd_nanos: 1_000,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 45_000,
        }),
    },
    TokenPricing {
        model: "gpt-5.5-pro",
        short: TokenRates {
            input_usd_nanos: 30_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 180_000,
        },
        long: Some(TokenRates {
            input_usd_nanos: 60_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 270_000,
        }),
    },
    TokenPricing {
        model: "gpt-5.4",
        short: TokenRates {
            input_usd_nanos: 2_500,
            cached_input_usd_nanos: 250,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 15_000,
        },
        long: Some(TokenRates {
            input_usd_nanos: 5_000,
            cached_input_usd_nanos: 500,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 22_500,
        }),
    },
    TokenPricing {
        model: "gpt-5.4-mini",
        short: TokenRates {
            input_usd_nanos: 750,
            cached_input_usd_nanos: 75,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 4_500,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5.4-nano",
        short: TokenRates {
            input_usd_nanos: 200,
            cached_input_usd_nanos: 20,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 1_250,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5.4-pro",
        short: TokenRates {
            input_usd_nanos: 30_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 180_000,
        },
        long: Some(TokenRates {
            input_usd_nanos: 60_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 270_000,
        }),
    },
    TokenPricing {
        model: "gpt-5.3-codex",
        short: TokenRates {
            input_usd_nanos: 1_750,
            cached_input_usd_nanos: 175,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 14_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5.2",
        short: TokenRates {
            input_usd_nanos: 1_750,
            cached_input_usd_nanos: 175,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 14_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5.2-pro",
        short: TokenRates {
            input_usd_nanos: 21_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 168_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5.1",
        short: TokenRates {
            input_usd_nanos: 1_250,
            cached_input_usd_nanos: 125,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 10_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5",
        short: TokenRates {
            input_usd_nanos: 1_250,
            cached_input_usd_nanos: 125,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 10_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5-mini",
        short: TokenRates {
            input_usd_nanos: 250,
            cached_input_usd_nanos: 25,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 2_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5-nano",
        short: TokenRates {
            input_usd_nanos: 50,
            cached_input_usd_nanos: 5,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 400,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-5-pro",
        short: TokenRates {
            input_usd_nanos: 15_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 120_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4.1",
        short: TokenRates {
            input_usd_nanos: 2_000,
            cached_input_usd_nanos: 500,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 8_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4.1-mini",
        short: TokenRates {
            input_usd_nanos: 400,
            cached_input_usd_nanos: 100,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 1_600,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4.1-nano",
        short: TokenRates {
            input_usd_nanos: 100,
            cached_input_usd_nanos: 25,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 400,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4o",
        short: TokenRates {
            input_usd_nanos: 2_500,
            cached_input_usd_nanos: 1_250,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 10_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4o-2024-05-13",
        short: TokenRates {
            input_usd_nanos: 5_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 15_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4o-mini",
        short: TokenRates {
            input_usd_nanos: 150,
            cached_input_usd_nanos: 75,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 600,
        },
        long: None,
    },
    TokenPricing {
        model: "o1",
        short: TokenRates {
            input_usd_nanos: 15_000,
            cached_input_usd_nanos: 7_500,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 60_000,
        },
        long: None,
    },
    TokenPricing {
        model: "o1-pro",
        short: TokenRates {
            input_usd_nanos: 150_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 600_000,
        },
        long: None,
    },
    TokenPricing {
        model: "o3",
        short: TokenRates {
            input_usd_nanos: 2_000,
            cached_input_usd_nanos: 500,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 8_000,
        },
        long: None,
    },
    TokenPricing {
        model: "o3-pro",
        short: TokenRates {
            input_usd_nanos: 20_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 80_000,
        },
        long: None,
    },
    TokenPricing {
        model: "o4-mini",
        short: TokenRates {
            input_usd_nanos: 1_100,
            cached_input_usd_nanos: 275,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 4_400,
        },
        long: None,
    },
    TokenPricing {
        model: "o3-mini",
        short: TokenRates {
            input_usd_nanos: 1_100,
            cached_input_usd_nanos: 550,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 4_400,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4-turbo-2024-04-09",
        short: TokenRates {
            input_usd_nanos: 10_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 30_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4-0613",
        short: TokenRates {
            input_usd_nanos: 30_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 60_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-3.5-turbo",
        short: TokenRates {
            input_usd_nanos: 500,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 1_500,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-3.5-turbo-0125",
        short: TokenRates {
            input_usd_nanos: 500,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 1_500,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-3.5-turbo-1106",
        short: TokenRates {
            input_usd_nanos: 1_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 2_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-3.5-turbo-instruct",
        short: TokenRates {
            input_usd_nanos: 1_500,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 2_000,
        },
        long: None,
    },
    TokenPricing {
        model: "davinci-002",
        short: TokenRates {
            input_usd_nanos: 2_000,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 2_000,
        },
        long: None,
    },
    TokenPricing {
        model: "babbage-002",
        short: TokenRates {
            input_usd_nanos: 400,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 400,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4o-transcribe",
        short: TokenRates {
            input_usd_nanos: 2_500,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 10_000,
        },
        long: None,
    },
    TokenPricing {
        model: "gpt-4o-mini-transcribe",
        short: TokenRates {
            input_usd_nanos: 1_250,
            cached_input_usd_nanos: 0,
            cache_write_usd_nanos: 0,
            output_usd_nanos: 5_000,
        },
        long: None,
    },
];

pub fn official_model_prices(available_model_ids: &[String]) -> Vec<ModelPrice> {
    TOKEN_PRICING
        .iter()
        .filter(|pricing| {
            available_model_ids
                .iter()
                .any(|model| model == pricing.model)
        })
        .map(|pricing| ModelPrice {
            model: pricing.model,
            short: model_price_rates(pricing.short),
            long: pricing.long.map(model_price_rates),
        })
        .collect()
}

fn model_price_rates(rates: TokenRates) -> ModelPriceRates {
    ModelPriceRates {
        input_usd_nanos: rates.input_usd_nanos * 1_000_000,
        cached_input_usd_nanos: (rates.cached_input_usd_nanos > 0)
            .then_some(rates.cached_input_usd_nanos * 1_000_000),
        cache_write_usd_nanos: (rates.cache_write_usd_nanos > 0)
            .then_some(rates.cache_write_usd_nanos * 1_000_000),
        output_usd_nanos: rates.output_usd_nanos * 1_000_000,
    }
}

fn official_cost_usd_nanos(model: &str, usage: Usage) -> Option<i64> {
    let pricing = TOKEN_PRICING
        .iter()
        .find(|pricing| pricing.model == model)?;
    let rates = pricing.rates(usage.input_tokens);
    let cached_tokens = usage.cached_tokens.clamp(0, usage.input_tokens.max(0));
    let uncached_tokens = usage.input_tokens.saturating_sub(cached_tokens);
    let cache_write_tokens = if rates.cache_write_usd_nanos == 0 {
        0
    } else {
        usage.cache_write_tokens.clamp(0, uncached_tokens)
    };
    Some(
        uncached_tokens
            .saturating_sub(cache_write_tokens)
            .saturating_mul(rates.input_usd_nanos)
            .saturating_add(cached_tokens.saturating_mul(rates.cached_input_usd_nanos))
            .saturating_add(cache_write_tokens.saturating_mul(rates.cache_write_usd_nanos))
            .saturating_add(
                usage
                    .output_tokens
                    .max(0)
                    .saturating_mul(rates.output_usd_nanos),
            ),
    )
}

fn apply_price_multiplier(official_cost_usd_nanos: i64, price_multiplier_nanos: i64) -> i64 {
    let actual_cost_usd_nanos = i128::from(official_cost_usd_nanos.max(0))
        * i128::from(price_multiplier_nanos.max(0))
        / i128::from(payments::USD_NANOS);
    i64::try_from(actual_cost_usd_nanos).expect("configured price multiplier keeps costs in range")
}

const SSE_EVENT_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct ResponseFailure {
    code: Option<String>,
    message: String,
}

fn response_failure(event_name: &str, value: &Value) -> Option<ResponseFailure> {
    let error = value
        .pointer("/response/error")
        .filter(|error| error.is_object());
    let failed = event_name == "response.failed"
        || value.get("type").and_then(Value::as_str) == Some("response.failed");
    if error.is_none() && !failed {
        return None;
    }
    Some(ResponseFailure {
        code: error
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        message: error
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("response.failed")
            .to_owned(),
    })
}

/// The model named by a terminal `response.completed` or `response.failed` event.
/// Both carry `response.model`, and a failed call is exactly where an upstream
/// downgrade is most useful to audit.
fn terminal_response_model<'a>(event_name: &str, value: &'a Value) -> Option<&'a str> {
    let terminal = matches!(event_name, "response.completed" | "response.failed")
        || matches!(
            value.get("type").and_then(Value::as_str),
            Some("response.completed" | "response.failed")
        );
    if !terminal {
        return None;
    }
    value
        .pointer("/response/model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
}

struct SseObservation {
    decoder: sse_core::SseDecoder,
    usage: Usage,
    upstream_model: Option<String>,
    failure: Option<ResponseFailure>,
}

impl Default for SseObservation {
    fn default() -> Self {
        Self {
            decoder: sse_core::SseDecoder::with_limit(
                std::num::NonZeroUsize::new(SSE_EVENT_LIMIT).expect("SSE event limit is positive"),
            ),
            usage: Usage::default(),
            upstream_model: None,
            failure: None,
        }
    }
}

impl SseObservation {
    fn capture(&mut self, mut bytes: &[u8]) {
        while let Some(event) = self.decoder.next(&mut bytes) {
            let message = match event {
                Ok(sse_core::SseEvent::Message(message)) => message,
                Ok(sse_core::SseEvent::Retry(_)) => continue,
                Err(error) => {
                    // RECOVERY: the decoder skips this oversized event and resumes at the next
                    // event boundary. Keep forwarding bytes, but do not report uninspected data as success.
                    self.failure.get_or_insert_with(|| ResponseFailure {
                        code: None,
                        message: format!("SSE inspection failed: {error}"),
                    });
                    continue;
                }
            };
            let value = match serde_json::from_str::<Value>(&message.data) {
                Ok(value) => value,
                Err(_) if message.event == "response.failed" => Value::Null,
                Err(_) => continue,
            };
            self.usage.merge(usage_from_value(&value));
            if let Some(model) = terminal_response_model(&message.event, &value) {
                self.upstream_model = Some(model.to_owned());
            }
            if let Some(failure) = response_failure(&message.event, &value) {
                self.failure = Some(failure);
            }
        }
    }
}

fn error_from(status: StatusCode, bytes: &[u8]) -> Option<String> {
    (!status.is_success()).then(|| {
        serde_json::from_slice::<Value>(bytes)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("upstream HTTP {}", status.as_u16()))
    })
}

fn ensure_model_is_available(state: &AppState, model: &str) -> Result<(), AppError> {
    state
        .config
        .load()
        .available_model_ids
        .iter()
        .any(|allowed| allowed == model)
        .then_some(())
        .ok_or_else(|| AppError::bad_request(format!("model is not available: {model}")))
}

fn models_response(models: &[String]) -> Result<Response, AppError> {
    let body = serde_json::to_vec(
        &json!({"object":"list","data":models.iter().map(|id| json!({"id":id,"object":"model","owned_by":"openai"})).collect::<Vec<_>>() }),
    )?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    build_response(
        StatusCode::OK,
        vec![(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        Body::from(body),
    )
}

#[cfg(test)]
mod tests {
    /// Minimal provider for identity-header and routing tests.
    fn test_provider(id: &str) -> Provider {
        test_provider_with_originator(id, CODEX_ORIGINATOR)
    }

    /// User agent a real CodeX CLI sends, used by the shared request helper.
    const CODEX_CLI_USER_AGENT: &str = "codex_cli_rs/0.51.0 (macos 15.0; arm64)";

    fn test_provider_with_originator(id: &str, originator: &str) -> Provider {
        Provider {
            id: id.to_owned(),
            name: id.to_owned(),
            account_id: "account-1".to_owned(),
            access_token: "access-token".to_owned(),
            refresh_token: "refresh-token".to_owned(),
            expires_at: None,
            status: "active".to_owned(),
            manual_disabled: 0,
            cooldown_until: None,
            rate_limit_json: None,
            last_error: None,
            last_used_at: None,
            created_at: 0,
            updated_at: 0,
            owner_id: None,
            originator: originator.to_owned(),
            allow_other_originator: false,
            visibility: PROVIDER_VISIBILITY_PRIVATE.to_owned(),
            official_provided_usd_nanos: 0,
            actual_provided_usd_nanos: 0,
            http_proxy_url: None,
            is_deleted: 0,
        }
    }

    use std::{
        io::Read,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use axum::{
        Json, Router,
        extract::{OriginalUri, State},
        response::IntoResponse,
        routing::post,
    };
    use http_body_util::BodyExt;
    use sqlx::Row;
    use tokio::sync::{Mutex, Notify};
    use tower::ServiceExt;

    use super::*;
    use crate::{balancer::PROVIDER_VISIBILITY_PRIVATE, test_downstream};

    #[derive(Clone)]
    struct RecordedRequest {
        path: String,
        headers: HeaderMap,
        body: Bytes,
    }

    #[derive(Clone)]
    struct MockUpstream {
        records: Arc<Mutex<Vec<RecordedRequest>>>,
        status: StatusCode,
    }

    async fn mock_upstream(
        State(mock): State<MockUpstream>,
        OriginalUri(uri): OriginalUri,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        mock.records.lock().await.push(RecordedRequest {
            path: uri.path().to_owned(),
            headers,
            body: body.clone(),
        });
        if mock.status != StatusCode::OK {
            return Response::builder()
                .status(mock.status)
                .header("retry-after", "30")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"error":{"message":"limited"}}"#))
                .unwrap();
        }
        if uri.path() == "/transcribe" {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::from(body))
                .unwrap();
        }
        if uri.path().ends_with("/realtime/calls") {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/sdp")
                .header(
                    header::LOCATION,
                    "/backend-api/codex/realtime/calls/rtc_test",
                )
                .body(Body::from("v=0\r\n"))
                .unwrap();
        }
        let image = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|value| value.get("tools").cloned())
            .is_some();
        if image {
            let events = concat!(
                "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"image_generation_call\",\"result\":\"aW1hZ2U=\"}}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":2,\"output_tokens\":3}}}\n\n"
            );
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .body(Body::from(events))
                .unwrap();
        }
        let stream = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|value| value.get("stream").and_then(Value::as_bool))
            .unwrap_or_default();
        if stream {
            let events = concat!(
                "event: response.created\n",
                "data: {\"type\":\"response.created\",\"response\":{\"usage\":null},\"sequence_number\":1}\n\n",
                "event: response.completed\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"model\":\"gpt-5.5\",\"usage\":{",
                "\"input_tokens\":19,\"input_tokens_details\":{\"cache_write_tokens\":0,\"cached_tokens\":0},",
                "\"output_tokens\":6,\"output_tokens_details\":{\"reasoning_tokens\":0},\"total_tokens\":25}},",
                "\"sequence_number\":9}\n\n",
            );
            return Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(events))
                .unwrap();
        }
        Json(json!({"id":"resp","usage":{"input_tokens":1,"output_tokens":2}})).into_response()
    }

    async fn spawn_mock(status: StatusCode) -> (String, Arc<Mutex<Vec<RecordedRequest>>>) {
        let records = Arc::new(Mutex::new(Vec::new()));
        let mock = MockUpstream {
            records: records.clone(),
            status,
        };
        let app = Router::new()
            .route("/responses", post(mock_upstream))
            .route("/responses/compact", post(mock_upstream))
            .route("/transcribe", post(mock_upstream))
            .route("/backend-api/codex/realtime/calls", post(mock_upstream))
            .route("/v1/realtime/calls", post(mock_upstream))
            .route("/realtime/calls", post(mock_upstream))
            .route("/v1/live", post(mock_upstream))
            .with_state(mock);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}"), records)
    }

    async fn spawn_usage_limit_then_success(reset: i64) -> (String, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let app = Router::new().route(
            "/responses",
            post(move || {
                let calls = counter.clone();
                async move {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        return Response::builder()
                            .status(StatusCode::TOO_MANY_REQUESTS)
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(Body::from(
                                json!({"error": {
                                    "type": "usage_limit_reached",
                                    "message": "The usage limit has been reached",
                                    "plan_type": "pro",
                                    "resets_at": reset,
                                    "resets_in_seconds": 3_600,
                                }})
                                .to_string(),
                            ))
                            .unwrap();
                    }
                    Json(json!({"id":"resp","usage":{"input_tokens":1,"output_tokens":2}}))
                        .into_response()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}"), calls)
    }

    async fn spawn_stream_mock(fail_after_first_chunk: bool) -> (String, Arc<Notify>) {
        let interrupt = Arc::new(Notify::new());
        let wait_for_interrupt = interrupt.clone();
        let app = Router::new().route(
            "/responses",
            post(move || {
                let wait_for_interrupt = wait_for_interrupt.clone();
                async move {
                    let output = async_stream::stream! {
                        yield Ok::<Bytes, std::io::Error>(Bytes::from_static(
                            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n",
                        ));
                        if fail_after_first_chunk {
                            wait_for_interrupt.notified().await;
                            yield Err(std::io::Error::other("test stream failure"));
                        } else {
                            yield Ok(Bytes::from_static(
                                b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n\n",
                            ));
                        }
                    };
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/event-stream")
                        .body(Body::from_stream(output))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}"), interrupt)
    }

    async fn seed_proxy(state: &AppState, provider: bool) {
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,allow_debt,created_at) VALUES('user-1','user',1,?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,request_archive,created_at) VALUES('key-1','user-1','test','sk-test',?,1,?)")
            .bind(crate::crypto::consumer_secret_hash("sk-test-secret")).bind(now).execute(&state.db).await.unwrap();
        if provider {
            sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,status,created_at,updated_at,owner_id) VALUES('provider-1','one','account-1',?,?,'active',?,?,'user-1')")
                .bind("access-token")
                .bind("refresh-token")
                .bind(now).bind(now).execute(&state.db).await.unwrap();
            state.balancer.reload_providers(&state.db).await.unwrap();
        }
    }

    /// Seeds an additional provider with an explicit client identity.
    async fn seed_provider_with_originator(state: &AppState, id: &str, originator: &str) {
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,status,created_at,updated_at,owner_id,originator) VALUES(?,?,?,?,?,'active',?,?,'user-1',?)")
            .bind(id)
            .bind(id)
            .bind(format!("account-{id}"))
            .bind("access-token")
            .bind("refresh-token")
            .bind(now)
            .bind(now)
            .bind(originator)
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
    }

    fn client_request(
        path: &str,
        user_agent: &str,
        originator: Option<&str>,
        body: Body,
    ) -> axum::http::Request<Body> {
        let mut request = proxy_request(path, "application/json", body);
        let headers = request.headers_mut();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_str(user_agent).unwrap(),
        );
        match originator {
            Some(originator) => {
                headers.insert(
                    ORIGINATOR_HEADER,
                    HeaderValue::from_str(originator).unwrap(),
                );
            }
            None => {
                headers.remove(ORIGINATOR_HEADER);
            }
        }
        request
    }

    /// User agent a real Pi Agent sends.
    const PI_USER_AGENT: &str = "pi (darwin 24.5.0; arm64)";

    /// Session id captured from a real Pi Agent run: the UUIDv7 the client derives from its start
    /// time. Pi validates this shape, so the proxy does too.
    const PI_SESSION_ID: &str = "01a0bd2a-0c12-7123-a3df-36f9d077060e";

    /// Pi Agent request as the deployed client sends it: Pi user agent, `originator: pi`, and a
    /// UUIDv7 session id.
    fn pi_request(path: &str, body: Body) -> axum::http::Request<Body> {
        let mut request = client_request(path, PI_USER_AGENT, Some("pi"), body);
        request
            .headers_mut()
            .insert("session-id", HeaderValue::from_static(PI_SESSION_ID));
        request
    }

    /// zstd-compressed JSON body, the encoding Pi Agent uses on every Codex request.
    fn zstd_body(json: &str) -> Bytes {
        Bytes::from(zstd::stream::encode_all(json.as_bytes(), PI_BODY_COMPRESSION_LEVEL).unwrap())
    }

    /// Headers the upstream received on the only recorded request.
    async fn recorded_headers(records: &Arc<Mutex<Vec<RecordedRequest>>>) -> axum::http::HeaderMap {
        let records = records.lock().await;
        assert_eq!(records.len(), 1, "expected exactly one upstream request");
        records[0].headers.clone()
    }

    #[tokio::test]
    async fn originator_is_enough_when_user_agent_is_unknown() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let app = crate::router(state.clone());
        let response = app
            .oneshot(client_request(
                "/v1/responses",
                "curl/8.7.1",
                Some(CODEX_ORIGINATOR),
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let _ = response.into_body().collect().await.unwrap();
        assert_eq!(records.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn downstream_identity_must_agree_with_the_user_agent() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let app = crate::router(state.clone());
        let mismatched = app
            .clone()
            .oneshot(client_request(
                "/v1/responses",
                CODEX_CLI_USER_AGENT,
                Some("pi"),
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(mismatched.status(), StatusCode::SERVICE_UNAVAILABLE);
        let missing = app
            .clone()
            .oneshot(client_request(
                "/v1/responses",
                CODEX_CLI_USER_AGENT,
                None,
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::OK);
        let _ = missing.into_body().collect().await.unwrap();
        assert_eq!(records.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn providers_only_serve_the_client_family_they_were_authorized_for() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        seed_provider_with_originator(&state, "provider-pi", "pi").await;
        let app = crate::router(state.clone());

        // The CodeX provider is unavailable for a Pi client, but the Pi provider is not.
        sqlx::query("UPDATE providers SET manual_disabled=1 WHERE id='provider-1'")
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
        let pi_response = app
            .clone()
            .oneshot(pi_request(
                "/v1/responses",
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(pi_response.status(), StatusCode::OK);
        let _ = pi_response.into_body().collect().await.unwrap();

        // With only a CodeX provider left, the same Pi client is rejected.
        sqlx::query("UPDATE providers SET manual_disabled=1 WHERE id='provider-pi'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE providers SET manual_disabled=0 WHERE id='provider-1'")
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
        let rejected = app
            .oneshot(pi_request(
                "/v1/responses",
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::SERVICE_UNAVAILABLE);
        let error: Value =
            serde_json::from_slice(&rejected.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(error["error"]["reason"], "provider_pool_empty");

        let records = records.lock().await;
        assert_eq!(records.len(), 1);
        let originators: Vec<&HeaderValue> = records[0]
            .headers
            .get_all(ORIGINATOR_HEADER)
            .iter()
            .collect();
        assert_eq!(originators.len(), 1);
        assert_eq!(originators[0], "pi");
    }

    #[tokio::test]
    async fn upstream_requests_carry_exactly_the_provider_originator() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let app = crate::router(state.clone());
        let response = app
            .oneshot(client_request(
                "/v1/responses",
                "codex_vscode/1.2.3 (linux; x86_64)",
                Some("codex_vscode"),
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let _ = response.into_body().collect().await.unwrap();
        let records = records.lock().await;
        let originators: Vec<&HeaderValue> = records[0]
            .headers
            .get_all(ORIGINATOR_HEADER)
            .iter()
            .collect();
        assert_eq!(originators.len(), 1);
        assert_eq!(originators[0], CODEX_ORIGINATOR);
    }

    #[tokio::test]
    async fn transcription_keeps_the_desktop_identity_without_duplicates() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let lease = select_ready_provider(&state, None, test_downstream())
            .await
            .unwrap();
        let mut inbound = HeaderMap::new();
        inbound.insert(ORIGINATOR_HEADER, HeaderValue::from_static("codex_cli_rs"));
        inbound.insert(header::USER_AGENT, HeaderValue::from_static("client-ua"));
        let _ = send_transcription_upstream(&state, &lease, &inbound, "audio".into())
            .await
            .unwrap();
        let _ = send_realtime_call_upstream(
            &state,
            &lease,
            &inbound,
            br#"{"sdp":"v=0","session":{"model":"gpt-realtime-1.5"}}"#,
        )
        .await
        .unwrap();
        let records = records.lock().await;
        let transcription = records
            .iter()
            .find(|record| record.path == "/transcribe")
            .expect("transcription request reached upstream");
        let originators: Vec<&HeaderValue> = transcription
            .headers
            .get_all(ORIGINATOR_HEADER)
            .iter()
            .collect();
        assert_eq!(
            originators,
            vec![&HeaderValue::from_static(CODEX_ORIGINATOR)]
        );
        let agents: Vec<&HeaderValue> = transcription
            .headers
            .get_all(header::USER_AGENT)
            .iter()
            .collect();
        assert_eq!(
            agents,
            vec![&HeaderValue::from_static(TRANSCRIPTION_USER_AGENT)]
        );
        let realtime = records
            .iter()
            .find(|record| record.path == "/realtime/calls")
            .expect("realtime call reached upstream");
        let originators: Vec<&HeaderValue> =
            realtime.headers.get_all(ORIGINATOR_HEADER).iter().collect();
        assert_eq!(
            originators,
            vec![&HeaderValue::from_static(CODEX_ORIGINATOR)]
        );
    }

    #[tokio::test]
    async fn request_quota_requires_credit_unless_the_user_can_carry_debt() {
        let state = crate::test_state("http://token.invalid").await;
        assert!(!state.config.load().allow_all_users_debt);
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('quota-user','user',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();

        let blocked = ensure_request_quota(&state, "quota-user", false)
            .await
            .unwrap_err();
        assert_eq!(blocked.status(), StatusCode::PAYMENT_REQUIRED);
        ensure_request_quota(&state, "quota-user", true)
            .await
            .unwrap();

        let mut config = (**state.config.load()).clone();
        config.allow_all_users_debt = true;
        state.config.store(std::sync::Arc::new(config));
        ensure_request_quota(&state, "quota-user", false)
            .await
            .unwrap();

        let mut config = (**state.config.load()).clone();
        config.allow_all_users_debt = false;
        state.config.store(std::sync::Arc::new(config));

        sqlx::query(
            "UPDATE users SET provided_usd_nanos=1,consumed_usd_nanos=1 WHERE id='quota-user'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let blocked = ensure_request_quota(&state, "quota-user", false)
            .await
            .unwrap_err();
        assert_eq!(blocked.status(), StatusCode::PAYMENT_REQUIRED);

        sqlx::query(
            "UPDATE users SET provided_usd_nanos=1,consumed_usd_nanos=0 WHERE id='quota-user'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        ensure_request_quota(&state, "quota-user", false)
            .await
            .unwrap();
    }

    async fn wait_for_audits(state: &AppState, expected: i64) {
        for _ in 0..100 {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_calls")
                .fetch_one(&state.db)
                .await
                .unwrap();
            if count >= expected {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {expected} audit events");
    }

    fn proxy_request(
        path: &str,
        content_type: &str,
        body: impl Into<Body>,
    ) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .method(if path == "/v1/models" {
                Method::GET
            } else {
                Method::POST
            })
            .uri(path)
            .header(header::AUTHORIZATION, "Bearer sk-test-secret")
            .header(header::CONTENT_TYPE, content_type)
            .header("x-request-id", "client-request-id")
            .header("x-lb-affinity-key", "explicit-secret")
            .header("session-id", "session-secret")
            .header("x-session-id", "session-secret")
            .header("x-codex-session-id", "codex-session-secret")
            .header("cookie", "private=1")
            .header("x-arbitrary", "drop-me")
            .header(header::USER_AGENT, CODEX_CLI_USER_AGENT)
            .header(ORIGINATOR_HEADER, CODEX_ORIGINATOR)
            .extension(ConnectInfo(
                "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            ))
            .body(body.into())
            .unwrap()
    }

    #[test]
    fn upstream_identity_replaces_only_the_installation_id_and_keeps_client_json() {
        let mut headers = HeaderMap::new();
        headers.append(header::USER_AGENT, HeaderValue::from_static("first-client"));
        headers.append(
            header::USER_AGENT,
            HeaderValue::from_static("second-client"),
        );
        headers.insert(
            "x-codex-turn-state",
            HeaderValue::from_static("client-turn"),
        );
        headers.insert(
            "x-codex-turn-metadata",
            HeaderValue::from_static(
                r#"{ "turn_id" : "turn", "installation_id" : "client-installation", "nested":{"installation_id":"nested-client"} }"#,
            ),
        );
        apply_upstream_identity_headers(
            &mut headers,
            &test_provider("provider-2"),
            Some("Global UA/1.0"),
            CODEX_ORIGINATOR,
        )
        .unwrap();
        assert_eq!(headers.get_all(header::USER_AGENT).iter().count(), 1);
        assert_eq!(headers[header::USER_AGENT], "Global UA/1.0");
        assert_eq!(headers["x-codex-turn-state"], "client-turn");
        assert_eq!(
            headers["x-codex-turn-metadata"],
            r#"{ "turn_id" : "turn", "installation_id" : "provider-2", "nested":{"installation_id":"nested-client"} }"#
        );
        apply_upstream_identity_headers(
            &mut headers,
            &test_provider("provider-3"),
            None,
            CODEX_ORIGINATOR,
        )
        .unwrap();
        assert_eq!(
            headers["x-codex-turn-metadata"],
            r#"{ "turn_id" : "turn", "installation_id" : "provider-3", "nested":{"installation_id":"nested-client"} }"#
        );
    }

    #[test]
    fn upstream_identity_replaces_every_occurrence_of_the_installation_id_value() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-codex-turn-metadata",
            HeaderValue::from_static(
                r#"{"installation_id":"client","turn_id":"turn","echo":{"installation_id":"client"}}"#,
            ),
        );
        apply_upstream_identity_headers(
            &mut headers,
            &test_provider("provider-2"),
            None,
            CODEX_ORIGINATOR,
        )
        .unwrap();
        assert_eq!(
            headers["x-codex-turn-metadata"],
            r#"{"installation_id":"provider-2","turn_id":"turn","echo":{"installation_id":"provider-2"}}"#
        );
    }

    #[test]
    fn upstream_identity_keeps_metadata_it_cannot_rewrite() {
        for metadata in [
            "not-json",
            "[]",
            "null",
            "42",
            r#"[{"installation_id":"client"}]"#,
            r#"{"nested":{"installation_id":"client"}}"#,
            r#"{"installation_id":null}"#,
            r#"{ "turn_id": "turn" }"#,
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(
                "x-codex-turn-metadata",
                HeaderValue::from_str(metadata).unwrap(),
            );
            let mut expected = headers.clone();
            expected.insert(
                ORIGINATOR_HEADER,
                HeaderValue::from_static(CODEX_ORIGINATOR),
            );
            apply_upstream_identity_headers(
                &mut headers,
                &test_provider("provider"),
                None,
                CODEX_ORIGINATOR,
            )
            .unwrap();
            assert_eq!(
                headers, expected,
                "metadata {metadata} must be forwarded as is"
            );
        }
        let mut absent = HeaderMap::new();
        apply_upstream_identity_headers(
            &mut absent,
            &test_provider("provider"),
            None,
            CODEX_ORIGINATOR,
        )
        .unwrap();
        assert_eq!(
            absent.get_all(ORIGINATOR_HEADER).iter().collect::<Vec<_>>(),
            vec![&HeaderValue::from_static(CODEX_ORIGINATOR)]
        );
    }

    #[test]
    fn terminal_model_is_read_from_completed_and_failed_events_only() {
        for terminal in [
            "event: response.completed\ndata: {\"response\":{\"model\":\"upstream-model\"}}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"model\":\"upstream-model\"}}\n\n",
            "event: response.failed\ndata: {\"response\":{\"model\":\"upstream-model\",\"error\":{\"code\":\"server_is_overloaded\"}}}\n\n",
            "data: {\"type\":\"response.failed\",\"response\":{\"model\":\"upstream-model\"}}\n\n",
        ] {
            let mut observation = SseObservation::default();
            observation.capture(b"data: {\"type\":\"response.created\",\"response\":{\"model\":\"created-model\"}}\n\n");
            assert_eq!(observation.upstream_model, None);
            for chunk in terminal.as_bytes().chunks(3) {
                observation.capture(chunk);
            }
            observation.capture(b"data: [DONE]\n\n");
            assert_eq!(
                observation.upstream_model.as_deref(),
                Some("upstream-model")
            );
        }
        for empty_model in [
            "data: {\"type\":\"response.completed\",\"response\":{\"model\":null}}\n\n",
            "data: {\"type\":\"response.failed\",\"response\":{\"model\":\"\"}}\n\n",
            "data: {\"type\":\"response.incomplete\",\"response\":{\"model\":\"upstream-model\"}}\n\n",
        ] {
            let mut observation = SseObservation::default();
            observation.capture(empty_model.as_bytes());
            assert_eq!(observation.upstream_model, None, "{empty_model}");
        }
    }

    #[tokio::test]
    async fn audit_keeps_downstream_ua_and_completed_model_with_or_without_archives() {
        let events = "event: response.completed\ndata: {\"response\":{\"model\":\"upstream-model\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2}}}\n\n";
        let response_json =
            r#"{"model":"upstream-model","usage":{"input_tokens":3,"output_tokens":2}}"#;
        let records = Arc::new(Mutex::new(Vec::<HeaderMap>::new()));
        let recorded = records.clone();
        let app = Router::new().route(
            "/responses",
            post(move |headers: HeaderMap, Json(payload): Json<Value>| {
                let recorded = recorded.clone();
                async move {
                    recorded.lock().await.push(headers);
                    let (content_type, body) = if payload["input"] == "json" {
                        ("application/json", response_json)
                    } else {
                        ("text/event-stream", events)
                    };
                    Response::builder()
                        .header(header::CONTENT_TYPE, content_type)
                        .body(Body::from(body))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        for (archive, stream, input) in [
            (false, true, "hello"),
            (true, true, "hello"),
            (false, false, "hello"),
            (true, false, "hello"),
            (false, false, "json"),
            (true, false, "json"),
        ] {
            let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
            seed_proxy(&state, true).await;
            sqlx::query("UPDATE consumers SET request_archive=?")
                .bind(archive)
                .execute(&state.db)
                .await
                .unwrap();
            let mut config = (**state.config.load()).clone();
            config.upstream_user_agents.codex_cli_rs = Some("Global UA/1.0".to_owned());
            state.config.store(Arc::new(config));
            let mut request = proxy_request(
                "/v1/responses",
                "application/json",
                json!({"model":"gpt-5.4","input":input,"stream":stream}).to_string(),
            );
            request.headers_mut().insert(
                header::USER_AGENT,
                HeaderValue::from_static(CODEX_CLI_USER_AGENT),
            );
            request.headers_mut().insert(
                "x-codex-turn-metadata",
                HeaderValue::from_static(
                    r#"{"installation_id":"downstream-installation","turn_id":"turn-1"}"#,
                ),
            );
            let response = crate::router(state.clone()).oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.into_body().collect().await.unwrap().to_bytes(),
                if input == "json" {
                    response_json
                } else {
                    events
                }
            );
            wait_for_audits(&state, 1).await;
            let row: (String, String, String) =
                sqlx::query_as("SELECT model,upstream_model,downstream_user_agent FROM api_calls")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(
                row,
                (
                    "gpt-5.4".into(),
                    "upstream-model".into(),
                    CODEX_CLI_USER_AGENT.into()
                )
            );
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_archives")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, i64::from(archive));
            if archive {
                let (inbound, outbound): (String, String) = sqlx::query_as("SELECT request_headers_json,upstream_request_headers_json FROM request_archives").fetch_one(&state.db).await.unwrap();
                assert!(inbound.contains("downstream-installation"));
                assert!(inbound.contains(CODEX_CLI_USER_AGENT));
                assert!(outbound.contains("provider-1"));
                assert!(outbound.contains("Global UA/1.0"));
                assert!(!outbound.contains("downstream-installation"));
            }
        }
        for headers in records.lock().await.iter() {
            assert_eq!(headers[header::USER_AGENT], "Global UA/1.0");
            let metadata: Value =
                serde_json::from_slice(headers["x-codex-turn-metadata"].as_bytes()).unwrap();
            assert_eq!(
                metadata,
                json!({"installation_id":"provider-1","turn_id":"turn-1"})
            );
        }
        server.abort();
    }

    #[tokio::test]
    async fn global_user_agent_covers_transcription_realtime_and_websocket() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let mut config = (**state.config.load()).clone();
        config.upstream_user_agents.codex_cli_rs = Some("Global UA/1.0".to_owned());
        state.config.store(Arc::new(config));
        let lease = select_ready_provider(&state, None, test_downstream())
            .await
            .unwrap();
        let mut inbound = HeaderMap::new();
        inbound.insert(header::USER_AGENT, HeaderValue::from_static("client"));
        inbound.insert(
            "x-codex-turn-metadata",
            HeaderValue::from_static(r#"{"installation_id":"client"}"#),
        );
        send_transcription_upstream(&state, &lease, &inbound, "audio".into())
            .await
            .unwrap();
        send_realtime_call_upstream(
            &state,
            &lease,
            &inbound,
            br#"{"sdp":"v=0","session":{"model":"gpt-realtime-1.5"}}"#,
        )
        .await
        .unwrap();
        let websocket =
            realtime_upstream_websocket_request(&state, &lease, &inbound, "rtc_test").unwrap();
        let records = records.lock().await;
        for headers in records
            .iter()
            .map(|record| &record.headers)
            .chain(std::iter::once(websocket.headers()))
        {
            assert_eq!(headers[header::USER_AGENT], "Global UA/1.0");
            assert_eq!(headers.get_all(header::USER_AGENT).iter().count(), 1);
            let metadata: Value =
                serde_json::from_slice(headers["x-codex-turn-metadata"].as_bytes()).unwrap();
            assert_eq!(metadata["installation_id"], "provider-1");
        }
    }

    #[tokio::test]
    async fn user_agent_overrides_are_selected_by_provider_originator() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        seed_provider_with_originator(&state, "provider-pi", "pi").await;
        let mut config = (**state.config.load()).clone();
        config.upstream_user_agents.codex_cli_rs = Some("Codex upstream/1.0".to_owned());
        config.upstream_user_agents.pi = Some("Pi upstream/1.0".to_owned());
        state.config.store(Arc::new(config));
        let app = crate::router(state.clone());

        let pi_response = app
            .clone()
            .oneshot(pi_request(
                "/v1/responses",
                Body::from(r#"{"model":"gpt-5.4","input":"pi"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(pi_response.status(), StatusCode::OK);
        let _ = pi_response.into_body().collect().await.unwrap();

        let codex_response = app
            .oneshot(client_request(
                "/v1/responses",
                "codex_cli_rs/0.51.0 (macos; arm64)",
                Some(CODEX_ORIGINATOR),
                Body::from(r#"{"model":"gpt-5.4","input":"codex"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(codex_response.status(), StatusCode::OK);
        let _ = codex_response.into_body().collect().await.unwrap();

        let records = records.lock().await;
        let mut user_agents = records
            .iter()
            .map(|record| {
                (
                    record.headers.get(ORIGINATOR_HEADER).unwrap().clone(),
                    record.headers.get(header::USER_AGENT).unwrap().clone(),
                )
            })
            .collect::<Vec<_>>();
        user_agents.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            user_agents,
            vec![
                (
                    HeaderValue::from_static(CODEX_ORIGINATOR),
                    HeaderValue::from_static("Codex upstream/1.0")
                ),
                (
                    HeaderValue::from_static("pi"),
                    HeaderValue::from_static("Pi upstream/1.0")
                )
            ]
        );
    }

    #[test]
    fn affinity_uses_only_session_id_header() {
        let mut headers = HeaderMap::new();
        headers.insert("session-id", HeaderValue::from_static("session"));
        headers.insert("x-session-id", HeaderValue::from_static("session"));
        headers.insert("x-lb-affinity-key", HeaderValue::from_static("explicit"));
        assert_eq!(
            affinity_key(&headers, Some(&json!({"previous_response_id":"body"})))
                .map(|key| key.value)
                .as_deref(),
            Some("session")
        );
        headers.remove("session-id");
        assert!(affinity_key(&headers, Some(&json!({"session_id":"body"}))).is_none());
    }

    #[tokio::test]
    async fn transcription_request_uses_backend_root_and_desktop_identity() {
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            "http://upstream.invalid/backend-api/codex",
        )
        .await;
        seed_proxy(&state, true).await;
        let lease = select_ready_provider(&state, None, test_downstream())
            .await
            .unwrap();
        let mut request = transcription_request(&state, &lease, &HeaderMap::new())
            .unwrap()
            .body(reqwest::Body::from("audio"))
            .build()
            .unwrap();
        apply_upstream_identity_headers(
            request.headers_mut(),
            &lease.provider,
            None,
            &lease.provider.originator,
        )
        .unwrap();
        assert_eq!(
            request.url().as_str(),
            "http://upstream.invalid/backend-api/transcribe"
        );
        assert_eq!(
            request
                .headers()
                .get_all("originator")
                .iter()
                .collect::<Vec<_>>(),
            vec![&HeaderValue::from_static(CODEX_ORIGINATOR)]
        );
        assert_eq!(
            request.headers().get(header::USER_AGENT).unwrap(),
            TRANSCRIPTION_USER_AGENT
        );
        assert_eq!(
            request.headers().get("chatgpt-account-id").unwrap(),
            "account-1"
        );
        assert!(request.headers().get("x-request-id").is_none());
    }

    #[tokio::test]
    async fn realtime_calls_translate_public_multipart_to_openai_realtime_api() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            &format!("{upstream}/backend-api/codex"),
        )
        .await;
        seed_proxy(&state, true).await;
        let body = concat!(
            "--test\r\n",
            "Content-Disposition: form-data; name=\"sdp\"\r\n\r\n",
            "v=offer\r\n",
            "--test\r\n",
            "Content-Disposition: form-data; name=\"session\"\r\n\r\n",
            "{\"type\":\"quicksilver\",\"model\":\"gpt-realtime-1.5\"}\r\n",
            "--test--\r\n"
        );
        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/realtime/calls",
                "multipart/form-data; boundary=test",
                body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/v1/realtime/calls/rtc_test"
        );
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/sdp"
        );
        let response_body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(response_body, Bytes::from_static(b"v=0\r\n"));

        let records = records.lock().await;
        assert_eq!(records.len(), 1);
        let request = &records[0];
        assert_eq!(request.path, "/v1/realtime/calls");
        assert_eq!(
            request.headers.get(header::AUTHORIZATION).unwrap(),
            "Bearer access-token"
        );
        assert_eq!(
            request.headers.get("chatgpt-account-id").unwrap(),
            "account-1"
        );
        assert!(
            request
                .headers
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("multipart/form-data; boundary="))
        );
        assert_eq!(
            request.headers.get("x-session-id").unwrap(),
            "session-secret"
        );
        let body = String::from_utf8_lossy(&request.body);
        assert!(body.contains("name=\"sdp\""));
        assert!(body.contains("v=offer"));
        assert!(body.contains("name=\"session\""));
        assert!(body.contains("\"type\":\"realtime\""));
        assert!(body.contains("\"model\":\"gpt-realtime-1.5\""));
        drop(records);
        assert_eq!(
            state
                .balancer
                .affinity_provider_id(&realtime_affinity_key("key-1", "rtc_test"))
                .as_deref(),
            Some("provider-1")
        );
        wait_for_audits(&state, 1).await;
        let path: String = sqlx::query_scalar("SELECT path FROM api_calls LIMIT 1")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(path, "/v1/realtime/calls");
    }

    #[tokio::test]
    async fn realtime_sideband_uses_the_direct_realtime_api() {
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            "https://upstream.invalid/backend-api/codex",
        )
        .await;
        seed_proxy(&state, true).await;
        let lease = state
            .balancer
            .select_provider("provider-1", test_downstream())
            .await
            .unwrap();
        let mut inbound = HeaderMap::new();
        inbound.insert("openai-alpha", HeaderValue::from_static("quicksilver=v1"));
        inbound.insert("x-session-id", HeaderValue::from_static("private"));
        let request =
            realtime_upstream_websocket_request(&state, &lease, &inbound, "rtc_test").unwrap();
        assert_eq!(
            request.uri(),
            "wss://upstream.invalid/v1/realtime?call_id=rtc_test"
        );
        assert_eq!(
            request.headers().get(header::AUTHORIZATION).unwrap(),
            "Bearer access-token"
        );
        assert_eq!(
            request.headers().get("chatgpt-account-id").unwrap(),
            "account-1"
        );
        assert!(request.headers().get("openai-alpha").is_none());
        assert_eq!(request.headers().get("x-session-id").unwrap(), "private");
    }

    #[tokio::test]
    async fn realtime_sideband_uses_only_the_call_id_query() {
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            "https://upstream.invalid/backend-api/codex",
        )
        .await;
        seed_proxy(&state, true).await;
        let lease = state
            .balancer
            .select_provider("provider-1", test_downstream())
            .await
            .unwrap();
        let request =
            realtime_upstream_websocket_request(&state, &lease, &HeaderMap::new(), "rtc_test")
                .unwrap();
        assert_eq!(
            request.uri(),
            "wss://upstream.invalid/v1/realtime?call_id=rtc_test"
        );
    }

    #[tokio::test]
    async fn realtime_call_parts_use_api_multipart_without_provider_query() {
        let state = crate::test_state_with_upstream(
            "http://token.invalid",
            "https://upstream.invalid/backend-api/codex",
        )
        .await;
        let (endpoint, body) = realtime_call_request_parts(
            &state,
            "v=offer\r\n",
            &json!({"type":"realtime","model":"gpt-realtime-1.5"}),
        )
        .unwrap();
        assert_eq!(endpoint, "https://upstream.invalid/v1/realtime/calls");
        let body = String::from_utf8(body).unwrap();
        assert!(body.contains("Content-Type: application/sdp"));
        assert!(body.contains("v=offer\r\n"));
        assert!(body.contains("\"type\":\"realtime\""));
    }

    #[test]
    fn realtime_session_normalization_keeps_public_shape_and_legacy_type() {
        let normalized = normalize_realtime_session(&json!({"type":"quicksilver"})).unwrap();
        assert_eq!(normalized["type"], "realtime");
        assert_eq!(normalized["model"], "gpt-realtime-1.5");
    }

    #[test]
    fn realtime_session_normalization_rejects_live_delegation() {
        let error = normalize_realtime_session(&json!({
            "type":"realtime",
            "delegation":{"type":"responses"}
        }))
        .unwrap_err();
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn console_realtime_calls_need_no_client_identity() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let identity = ApiIdentity {
            consumer_id: "key-1".to_owned(),
            user_id: "user-1".to_owned(),
            request_archive: true,
            intercept_degradation: false,
            is_admin: false,
            allow_debt: true,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"),
        );
        let result = start_realtime_call(
            &state,
            &identity,
            RequestAuditContext {
                request_id: Uuid::new_v4().to_string(),
                thread_id: None,
                session_id: None,
                method: Method::POST,
                client_ip: "127.0.0.1".to_owned(),
            },
            "/api/realtime/calls",
            &headers,
            RealtimeCallRequest {
                sdp: "v=0".to_owned(),
                session: json!({"model":"gpt-realtime-1.5"}),
            },
            RealtimeCallSource::Console,
        )
        .await
        .unwrap();
        assert_eq!(result.call_id, "rtc_test");
        let records = records.lock().await;
        let originators: Vec<&HeaderValue> = records[0]
            .headers
            .get_all(ORIGINATOR_HEADER)
            .iter()
            .collect();
        assert_eq!(
            originators,
            vec![&HeaderValue::from_static(CODEX_ORIGINATOR)]
        );
    }

    #[tokio::test]
    async fn console_image_generation_writes_a_complete_image_audit() {
        let (upstream, _) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let user = UserIdentity {
            id: "user-1".to_owned(),
            email: None,
            name: None,
            role: "user".to_owned(),
        };
        let response = handle_console_image_generation(
            State(state.clone()),
            Extension(user),
            ConnectInfo("127.0.0.1:12345".parse().unwrap()),
            HeaderMap::new(),
            Bytes::from_static(br#"{"model":"gpt-image-1","prompt":"diagram","n":1}"#),
        )
        .await
        .unwrap();

        assert_eq!(response.0["data"][0]["b64_json"], "aW1hZ2U=");
        wait_for_audits(&state, 1).await;
        let row: (String, String, i64, Vec<u8>, Vec<u8>, bool) = sqlx::query_as(
            "SELECT c.path,k.name,c.status,a.request_body,a.response_body,a.response_body_truncated FROM api_calls c JOIN consumers k ON k.id=c.consumer_id JOIN request_archives a ON a.api_call_id=c.id",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row.0, "/v1/responses");
        assert_eq!(row.1, "OpenAI-LB Console");
        assert_eq!(row.2, 200);
        assert!(!row.5);
        let request: Value = serde_json::from_slice(&row.3).unwrap();
        assert_eq!(request["tools"][0]["type"], "image_generation");
        assert_eq!(request["input"][0]["content"][0]["text"], "diagram");
        assert_eq!(
            serde_json::from_slice::<Value>(&row.4).unwrap()["data"][0]["b64_json"],
            "aW1hZ2U="
        );
    }

    #[test]
    fn image_audit_response_preview_keeps_four_mebibytes() {
        let body = vec![7; IMAGE_ARCHIVE_BODY_LIMIT - 1];
        let (preview, truncated) = body_preview(&body, response_archive_body_limit(true));
        assert_eq!(preview, body);
        assert!(!truncated);

        let (preview, truncated) = body_preview(
            &vec![7; IMAGE_ARCHIVE_BODY_LIMIT + 1],
            response_archive_body_limit(true),
        );
        assert_eq!(preview.len(), IMAGE_ARCHIVE_BODY_LIMIT);
        assert!(truncated);
    }

    #[test]
    fn image_stream_error_includes_upstream_diagnostic() {
        let error = images_from_sse(
            br#"data: {"type":"response.failed","response":{"error":{"message":"content policy violation"}}}

data: {"type":"response.completed","response":{"output":[]}}
"#,
        )
        .unwrap_err();

        assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(error.reason(), Some("image_data_missing"));
        assert!(error.message().contains("content policy violation"));
        assert!(error.message().contains("response.completed"));
    }

    #[test]
    fn image_stream_error_explains_empty_or_unreadable_response() {
        let error = images_from_sse(b"data: {\"type\":\"response.completed\"}\n\n").unwrap_err();

        assert_eq!(error.reason(), Some("image_data_missing"));
        assert!(error.message().contains("response.completed"));
        assert!(error.message().contains("1 readable event"));
    }

    #[test]
    fn image_stream_error_extracts_text_from_completed_response() {
        let error = images_from_sse(
            br#"data: {"type":"response.completed","response":{"output":[{"type":"message","content":[{"type":"output_text","text":"The request was not completed because the prompt was rejected."}]}]}}
"#,
        )
        .unwrap_err();

        assert!(error.message().contains(
            "Upstream response: The request was not completed because the prompt was rejected."
        ));
    }

    #[test]
    fn image_stream_error_summarizes_failed_image_call_without_image_bytes() {
        let error = images_from_sse(
            br#"data: {"type":"response.output_item.done","item":{"type":"image_generation_call","status":"failed","result":null}}
"#,
        )
        .unwrap_err();

        assert!(
            error
                .message()
                .contains("image_generation_call status=failed; result=null")
        );
    }

    #[test]
    fn image_stream_error_summarizes_image_call_fields_without_status() {
        let error = images_from_sse(
            br#"data: {"type":"response.output_item.done","item":{"type":"image_generation_call","id":"img_123"}}
"#,
        )
        .unwrap_err();

        assert!(error.message().contains(
            "image_generation_call did not return usable image data; result=missing; fields=id,type"
        ));
    }

    #[tokio::test]
    async fn reference_images_are_translated_to_responses_input_images() {
        let state = crate::test_state("http://token.invalid").await;
        let payload = transform_request(
            "/v1/images/generations",
            Some(json!({
                "prompt": "a product scene",
                "reference_images": [
                    "data:image/png;base64,cG5n",
                    "data:image/jpeg;base64,anBlZw=="
                ]
            })),
            &state,
        )
        .unwrap();
        let translated: Value = serde_json::from_slice(&payload).unwrap();
        let content = translated
            .pointer("/input/0/content")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(content.len(), 3);
        assert_eq!(content[1]["type"], "input_image");
        assert_eq!(content[1]["image_url"], "data:image/png;base64,cG5n");
        assert_eq!(content[2]["detail"], "auto");
    }

    #[test]
    fn thread_id_extracts_existing_downstream_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("thread-id", HeaderValue::from_static("app-thread"));
        assert_eq!(thread_id(&headers).as_deref(), Some("app-thread"));

        headers.insert(
            "x-codex-conversation-id",
            HeaderValue::from_static("codex-thread"),
        );
        assert_eq!(thread_id(&headers).as_deref(), Some("codex-thread"));

        headers.clear();
        assert_eq!(thread_id(&headers), None);
    }

    #[test]
    fn session_id_extracts_only_the_session_id_header() {
        let mut headers = HeaderMap::new();
        headers.insert("session-id", HeaderValue::from_static(" session-a "));
        headers.insert("x-session-id", HeaderValue::from_static("legacy-session"));
        assert_eq!(session_id(&headers).as_deref(), Some("session-a"));
        headers.remove("session-id");
        assert_eq!(session_id(&headers), None);
    }

    #[test]
    fn image_generation_mints_a_session_id_only_when_the_caller_omits_one() {
        let empty = HeaderMap::new();
        let minted = upstream_request_headers("/v1/images/generations", &empty);
        let session = minted.get("session-id").unwrap().to_str().unwrap();
        assert_eq!(Uuid::parse_str(session).unwrap().get_version_num(), 7);

        let mut supplied = HeaderMap::new();
        supplied.insert("session-id", HeaderValue::from_static("client-session"));
        let forwarded = upstream_request_headers("/v1/images/generations", &supplied);
        assert_eq!(forwarded.get("session-id").unwrap(), "client-session");

        // Every other endpoint forwards the caller's headers without minting anything.
        assert!(
            upstream_request_headers("/v1/responses", &empty)
                .get("session-id")
                .is_none()
        );
    }

    #[test]
    fn response_headers_exclude_x_request_id() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-request-id",
            HeaderValue::from_static("upstream-request-id"),
        );
        headers.insert("x-upstream", HeaderValue::from_static("preserved"));

        let forwarded = filtered_response_headers(&headers);
        assert!(forwarded.iter().all(|(name, _)| name != "x-request-id"));
        assert!(forwarded.iter().any(|(name, _)| name == "x-upstream"));
    }

    #[tokio::test]
    async fn models_calls_are_not_audited() {
        let state = crate::test_state("http://token.invalid").await;
        seed_proxy(&state, false).await;
        let mut config = (**state.config.load()).clone();
        config.available_model_ids = vec!["gpt-5.6-luna".to_owned(), "gpt-image-2".to_owned()];
        state.config.store(Arc::new(config));
        let app = crate::router(state.clone());
        let mut request = proxy_request("/v1/models", "application/json", Body::empty());
        request.headers_mut().insert(
            "x-codex-conversation-id",
            HeaderValue::from_static("codex-thread"),
        );
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(
            body["data"],
            json!([
                {"id":"gpt-5.6-luna","object":"model","owned_by":"openai"},
                {"id":"gpt-image-2","object":"model","owned_by":"openai"}
            ])
        );

        let calls: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_calls")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(calls, 0);
    }

    #[tokio::test]
    async fn gzip_compresses_client_response() {
        let state = crate::test_state("http://token.invalid").await;
        seed_proxy(&state, false).await;
        let mut request = proxy_request("/v1/models", "application/json", Body::empty());
        request
            .headers_mut()
            .insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip"));

        let response = crate::router(state).oneshot(request).await.unwrap();
        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING).unwrap(),
            "gzip"
        );
        let compressed = response.into_body().collect().await.unwrap().to_bytes();
        let mut decoded = String::new();
        flate2::read::GzDecoder::new(compressed.as_ref())
            .read_to_string(&mut decoded)
            .unwrap();
        assert!(decoded.contains("gpt-5.4"));
    }

    #[tokio::test]
    async fn rejects_responses_models_outside_the_available_list() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let mut config = (**state.config.load()).clone();
        config.available_model_ids = vec!["gpt-5.6-luna".to_owned()];
        state.config.store(Arc::new(config));

        let response = crate::router(state)
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"blocked"}"#),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(records.lock().await.is_empty());
    }

    #[tokio::test]
    async fn preserves_upstream_cookie_but_only_passes_through_turn_state() {
        let requests = Arc::new(Mutex::new(Vec::<HeaderMap>::new()));
        let recorded = requests.clone();
        let upstream = Router::new().route(
            "/responses",
            post(move |headers: HeaderMap| {
                let recorded = recorded.clone();
                async move {
                    let request_only = headers
                        .get("x-codex-turn-state")
                        .is_some_and(|value| value == "request-only-state");
                    recorded.lock().await.push(headers);
                    let mut response = Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json")
                        .header(
                            header::SET_COOKIE,
                            "__oailb=sticky-route; Path=/; Max-Age=3600",
                        );
                    if !request_only {
                        response = response.header("x-codex-turn-state", "turn-state-1");
                    }
                    response
                        .body(Body::from(
                            r#"{"output":[],"usage":{"input_tokens":1,"output_tokens":1}}"#,
                        ))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let state =
            crate::test_state_with_upstream("http://token.invalid", &format!("http://{address}"))
                .await;
        seed_proxy(&state, true).await;
        let app = crate::router(state.clone());
        let client_turn_states = [
            None,
            Some("turn-state-1"),
            Some("request-only-state"),
            Some("client-turn-state-2"),
        ];
        for turn_state in client_turn_states {
            let mut request = proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"hello"}"#),
            );
            request
                .headers_mut()
                .insert("session-id", HeaderValue::from_static("cookie-session"));
            if let Some(turn_state) = turn_state {
                request
                    .headers_mut()
                    .insert("x-codex-turn-state", HeaderValue::from_static(turn_state));
            }
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            if turn_state == Some("request-only-state") {
                assert!(response.headers().get("x-codex-turn-state").is_none());
            } else {
                assert_eq!(
                    response.headers().get("x-codex-turn-state").unwrap(),
                    "turn-state-1"
                );
            }
            let _ = response.into_body().collect().await.unwrap();
        }
        let requests = requests.lock().await;
        assert_eq!(requests.len(), client_turn_states.len());
        assert!(requests[0].get(header::COOKIE).is_none());
        for request in &requests[1..] {
            assert_eq!(request.get(header::COOKIE).unwrap(), "__oailb=sticky-route");
        }
        for (index, (request, expected)) in requests.iter().zip(client_turn_states).enumerate() {
            assert_eq!(
                request
                    .get("x-codex-turn-state")
                    .map(|value| value.to_str().unwrap()),
                expected,
                "request {index} must preserve the client's turn state, including its absence"
            );
        }
        wait_for_audits(&state, client_turn_states.len() as i64).await;
        let lengths = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let lengths: Vec<Option<i64>> = sqlx::query_scalar(
                    "SELECT codex_turn_state_length FROM api_calls ORDER BY created_at,id",
                )
                .fetch_all(&state.db)
                .await
                .unwrap();
                let mut sorted_lengths = lengths.clone();
                sorted_lengths.sort_unstable();
                if lengths.len() == client_turn_states.len()
                    && sorted_lengths == [Some(12), Some(12), Some(12), Some(18)]
                {
                    break lengths;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("response header length was persisted");
        let mut sorted_lengths = lengths;
        sorted_lengths.sort_unstable();
        assert_eq!(sorted_lengths, vec![Some(12), Some(12), Some(12), Some(18)]);
    }

    #[tokio::test]
    async fn forwards_client_accept_encoding_upstream() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let mut request = proxy_request(
            "/v1/responses",
            "application/json",
            Body::from(r#"{"model":"gpt-5.4","input":"compressed"}"#),
        );
        request
            .headers_mut()
            .insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip"));

        let response = crate::router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            records.lock().await[0]
                .headers
                .get(header::ACCEPT_ENCODING)
                .unwrap(),
            "gzip"
        );
    }

    #[tokio::test]
    async fn proxy_response_header_links_to_the_persisted_audit() {
        let (upstream, _) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"audit link"}"#),
            ))
            .await
            .unwrap();

        let audit_id = response
            .headers()
            .get("x-openai-lb-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        response.into_body().collect().await.unwrap();
        wait_for_audits(&state, 1).await;
        let persisted_id: String = sqlx::query_scalar("SELECT id FROM api_calls")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(audit_id, persisted_id);
    }

    #[tokio::test]
    async fn only_injects_configured_openai_beta_header() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"configured"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(records.lock().await[0].headers.get("openai-beta").is_none());

        let mut config = (**state.config.load()).clone();
        config.upstream_openai_beta = Some("responses=experimental".to_owned());
        state.config.store(Arc::new(config));
        let response = crate::router(state)
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"configured"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            records.lock().await[1].headers.get("openai-beta").unwrap(),
            "responses=experimental"
        );
    }

    #[tokio::test]
    async fn streaming_responses_are_not_compressed() {
        let (upstream, _) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let mut request = proxy_request(
            "/v1/responses",
            "application/json",
            Body::from(r#"{"model":"gpt-5.4","input":"stream","stream":true}"#),
        );
        request
            .headers_mut()
            .insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip"));

        let response = crate::router(state).oneshot(request).await.unwrap();
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
    }

    #[tokio::test]
    async fn preserves_the_responses_stream_contract() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let app = crate::router(state);

        let response = app
            .clone()
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"non-streaming"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );

        let response = app
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"streaming","stream":true}"#),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.starts_with(b"event: response.created\n"));

        let records = records.lock().await;
        let non_streaming: Value = serde_json::from_slice(&records[0].body).unwrap();
        assert_eq!(non_streaming.get("stream"), None);
        let streaming: Value = serde_json::from_slice(&records[1].body).unwrap();
        assert_eq!(streaming.get("stream").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn reads_usage_without_retaining_prompt() {
        let usage = usage_from_value(
            &json!({"usage":{"input_tokens":10,"output_tokens":4,"input_tokens_details":{"cached_tokens":3,"cache_write_tokens":2}}}),
        );
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_tokens,
                usage.cache_write_tokens,
            ),
            (10, 4, 3, 2)
        );
    }

    #[test]
    fn prices_uncached_cached_and_cache_write_tokens_once() {
        assert_eq!(
            official_cost_usd_nanos(
                "gpt-5.6-terra",
                Usage {
                    input_tokens: 20_000,
                    output_tokens: 1_000,
                    cached_tokens: 10_000,
                    cache_write_tokens: 2_000,
                },
            ),
            Some(35_000_000)
        );
        assert_eq!(official_cost_usd_nanos("unknown", Usage::default()), None);
        assert_eq!(apply_price_multiplier(35_000_000, 100_000_000), 3_500_000);
    }

    #[test]
    fn exposes_the_official_price_snapshot_per_million_tokens() {
        let prices = official_model_prices(&crate::config::default_available_model_ids());
        let gpt_6_astra = prices
            .iter()
            .find(|price| price.model == "gpt-6-astra")
            .unwrap();
        assert_eq!(gpt_6_astra.short.input_usd_nanos, 10_000_000_000);
        assert_eq!(
            gpt_6_astra.long.as_ref().unwrap().cache_write_usd_nanos,
            Some(25_000_000_000)
        );
        assert_eq!(
            gpt_6_astra.long.as_ref().unwrap().output_usd_nanos,
            75_000_000_000
        );
        let gpt_5_6_sol = prices
            .iter()
            .find(|price| price.model == "gpt-5.6-sol")
            .unwrap();
        assert_eq!(gpt_5_6_sol.short.input_usd_nanos, 4_000_000_000);
        assert_eq!(
            gpt_5_6_sol.long.as_ref().unwrap().output_usd_nanos,
            30_000_000_000
        );
        assert!(prices.iter().all(|price| price.model != "gpt-4-0613"));
    }

    #[test]
    fn exposes_gpt_6_sol_and_luna_prices() {
        let prices = official_model_prices(&["gpt-6-sol".to_owned(), "gpt-6-luna".to_owned()]);
        let gpt_6_sol = prices
            .iter()
            .find(|price| price.model == "gpt-6-sol")
            .unwrap();
        assert_eq!(gpt_6_sol.short.input_usd_nanos, 2_000_000_000);
        assert_eq!(
            gpt_6_sol.long.as_ref().unwrap().output_usd_nanos,
            15_000_000_000
        );
        let gpt_6_luna = prices
            .iter()
            .find(|price| price.model == "gpt-6-luna")
            .unwrap();
        assert_eq!(gpt_6_luna.short.cached_input_usd_nanos, Some(10_000_000));
        assert_eq!(
            gpt_6_luna.long.as_ref().unwrap().cache_write_usd_nanos,
            Some(250_000_000)
        );
    }

    #[test]
    fn diagnostic_archive_excludes_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        headers.insert("x-api-key", HeaderValue::from_static("secret"));
        headers.insert("x-openai-api-key", HeaderValue::from_static("secret"));
        headers.insert("x-auth", HeaderValue::from_static("secret"));
        headers.insert("x-credential", HeaderValue::from_static("secret"));
        headers.insert("access-key", HeaderValue::from_static("secret"));
        headers.insert("x-session-id", HeaderValue::from_static("session-secret"));
        headers.insert("x-arbitrary", HeaderValue::from_static("private"));
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let archived = archive_headers(&headers);
        let archived: Vec<(String, String)> = serde_json::from_str(&archived).unwrap();
        assert_eq!(archived.len(), 8);
        for (name, value) in [
            ("x-api-key", "secret"),
            ("x-openai-api-key", "secret"),
            ("x-auth", "secret"),
            ("x-credential", "secret"),
            ("access-key", "secret"),
            ("x-session-id", "session-secret"),
            ("x-arbitrary", "private"),
            ("content-type", "application/json"),
        ] {
            assert!(
                archived
                    .iter()
                    .any(|header| header == &(name.to_owned(), value.to_owned()))
            );
        }
        assert!(archived.iter().all(|(name, _)| name != "authorization"));

        let (preview, truncated) =
            body_preview(&vec![7; ARCHIVE_BODY_LIMIT + 1], ARCHIVE_BODY_LIMIT);
        assert_eq!(preview.len(), ARCHIVE_BODY_LIMIT);
        assert!(truncated);
    }

    #[test]
    fn diagnostic_preview_keeps_a_1280190_byte_request() {
        let body = vec![7; 1_280_190];
        let (preview, truncated) = body_preview(&body, ARCHIVE_BODY_LIMIT);

        assert_eq!(preview.len(), body.len());
        assert!(!truncated);
    }

    #[test]
    fn reads_usage_from_response_completed_sse_across_chunks() {
        let event = concat!(
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{",
            "\"input_tokens\":11,\"output_tokens\":5,",
            "\"input_tokens_details\":{\"cached_tokens\":7}}}}\n\n",
        );
        let mut observation = SseObservation::default();

        for chunk in event.as_bytes().chunks(17) {
            observation.capture(chunk);
        }
        let usage = observation.usage;

        assert_eq!(
            (usage.input_tokens, usage.cached_tokens, usage.output_tokens),
            (11, 7, 5)
        );
    }

    #[test]
    fn sse_library_decodes_failures_across_byte_and_event_boundaries() {
        for ending in ["\n", "\r\n", "\r"] {
            let input = concat!(
                "\u{feff}:keepalive\nretry: 1000\n\n",
                "event: response.failed\n",
                "data:{\"response\":{\"error\":{\"code\":\"future_error_code\",\n",
                "data: \"message\":\"服务器繁忙\"},\"usage\":{\"input_tokens\":11,\"output_tokens\":5}}}\n\n",
                "data: [DONE]\n\n",
            ).replace('\n', ending);
            for chunk_size in [1, 7, 17, input.len()] {
                let mut observation = SseObservation::default();
                for chunk in input.as_bytes().chunks(chunk_size) {
                    observation.capture(chunk);
                }
                assert_eq!(
                    observation.failure,
                    Some(ResponseFailure {
                        code: Some("future_error_code".to_owned()),
                        message: "服务器繁忙".to_owned(),
                    })
                );
                assert_eq!(observation.usage.input_tokens, 11);
                assert_eq!(observation.usage.output_tokens, 5);
            }
        }
    }

    #[test]
    fn sse_failures_do_not_require_a_known_code_or_event_header() {
        for (data, expected_code, expected_message) in [
            (
                r#"{"type":"response.failed","response":{"error":{"code":"server_is_overloaded","message":"overloaded"}}}"#,
                Some("server_is_overloaded"),
                "overloaded",
            ),
            (
                r#"{"type":"response.failed","response":{"error":{"message":"failed without code"}}}"#,
                None,
                "failed without code",
            ),
            (
                r#"{"type":"response.failed","response":{"error":null}}"#,
                None,
                "response.failed",
            ),
            (
                r#"{"type":"response.in_progress","response":{"error":{"code":"new_code","message":"new failure"}}}"#,
                Some("new_code"),
                "new failure",
            ),
        ] {
            let mut observation = SseObservation::default();
            observation.capture(format!("data:{data}\n\n").as_bytes());
            assert_eq!(
                observation.failure,
                Some(ResponseFailure {
                    code: expected_code.map(str::to_owned),
                    message: expected_message.to_owned(),
                })
            );
        }
        let mut observation = SseObservation::default();
        observation.capture(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"response.failed server_is_overloaded\"}\n\ndata: [DONE]\n\n");
        assert!(observation.failure.is_none());
    }

    #[test]
    fn sse_inspection_recovers_after_an_oversized_event() {
        let mut observation = SseObservation {
            decoder: sse_core::SseDecoder::with_limit(std::num::NonZeroUsize::new(128).unwrap()),
            ..SseObservation::default()
        };
        observation.capture(format!("data:{}\n\n", "x".repeat(256)).as_bytes());
        assert!(
            observation
                .failure
                .as_ref()
                .unwrap()
                .message
                .starts_with("SSE inspection failed:")
        );
        observation.capture(b"event: response.failed\ndata:{\"response\":{\"error\":{\"code\":\"overloaded\",\"message\":\"busy\"}}}\n\n");
        assert_eq!(
            observation.failure.unwrap().code.as_deref(),
            Some("overloaded")
        );
    }

    #[tokio::test]
    async fn sse_failure_is_persisted_without_changing_the_forwarded_stream() {
        let failure = concat!(
            "event: response.failed\r\n",
            "data:{\"type\":\"response.failed\",\"response\":{\"model\":\"upstream-model\",\"error\":{\"code\":\"server_is_overloaded\",\r\n",
            "data: \"message\":\"Our servers are currently overloaded. Please try again later.\"},\"usage\":null}}\r\n\r\n",
            "data: [DONE]\r\n\r\n",
        );
        // The failure arrives after the archived preview has already reached its limit.
        let events = format!(
            "data: {{\"response\":{{\"usage\":{{\"input_tokens\":11,\"output_tokens\":5}}}}}}\n\n:{}\n\n{failure}",
            "x".repeat(ARCHIVE_BODY_LIMIT)
        );
        let upstream_body = events.clone();
        let app = Router::new().route(
            "/responses",
            post(move || {
                let body = upstream_body.clone();
                async move {
                    Response::builder()
                        .header(header::CONTENT_TYPE, "text/event-stream")
                        .body(Body::from_stream(futures_util::stream::iter(
                            body.into_bytes()
                                .chunks(4096)
                                .map(|chunk| Ok::<_, Infallible>(Bytes::copy_from_slice(chunk)))
                                .collect::<Vec<_>>(),
                        )))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        for (request_archive, stream) in [(true, true), (false, true), (true, false)] {
            let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
            seed_proxy(&state, true).await;
            sqlx::query("UPDATE consumers SET request_archive=? WHERE id='key-1'")
                .bind(request_archive)
                .execute(&state.db)
                .await
                .unwrap();
            let response = crate::router(state.clone())
                .oneshot(proxy_request(
                    "/v1/responses",
                    "application/json",
                    Body::from(
                        json!({"model":"gpt-5.4","input":"audit","stream":stream}).to_string(),
                    ),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let audit_id = response.headers()["x-openai-lb-request-id"]
                .to_str()
                .unwrap()
                .to_owned();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(body.as_ref(), events.as_bytes());
            wait_for_audits(&state, 1).await;
            let row: (i64, String, String, i64, i64, i64, Option<String>) = sqlx::query_as(
                "SELECT status,error_code,error,input_tokens,output_tokens,response_bytes,upstream_model FROM api_calls WHERE id=?"
            ).bind(&audit_id).fetch_one(&state.db).await.unwrap();
            assert_eq!(
                row,
                (
                    200,
                    "server_is_overloaded".to_owned(),
                    "Our servers are currently overloaded. Please try again later.".to_owned(),
                    11,
                    5,
                    events.len() as i64,
                    Some("upstream-model".to_owned())
                )
            );
            let archives: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_archives")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(archives, i64::from(request_archive));
            if request_archive {
                let truncated: bool =
                    sqlx::query_scalar("SELECT response_body_truncated FROM request_archives")
                        .fetch_one(&state.db)
                        .await
                        .unwrap();
                assert!(truncated);
            }
        }
        server.abort();
    }

    #[test]
    fn reads_reasoning_effort_from_supported_request_shapes() {
        assert_eq!(
            reasoning_effort(&json!({"reasoning":{"effort":"high"}})),
            Some("high".to_owned())
        );
        assert_eq!(
            reasoning_effort(&json!({"reasoning_effort":"medium"})),
            Some("medium".to_owned())
        );
        assert_eq!(reasoning_effort(&json!({"reasoning":{}})), None);
    }

    #[test]
    fn identifies_fast_service_tiers() {
        assert!(fast_mode(&json!({"service_tier":"fast"})));
        assert!(fast_mode(&json!({"service_tier":"priority"})));
        assert!(!fast_mode(&json!({"service_tier":"default"})));
        assert!(!fast_mode(&json!({})));
    }

    #[test]
    fn fast_mode_doubles_the_request_price_multiplier() {
        assert_eq!(
            effective_price_multiplier_nanos(100_000_000, false),
            100_000_000
        );
        assert_eq!(
            effective_price_multiplier_nanos(100_000_000, true),
            200_000_000
        );
        assert_eq!(
            effective_price_multiplier_nanos(200_000_000, true),
            400_000_000
        );
    }

    #[tokio::test]
    async fn audit_persists_usage_without_request_body() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('user-1','user',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('key-1','user-1','test','sk-test','hash',?)")
            .bind(now).execute(&state.db).await.unwrap();
        let identity = ApiIdentity {
            consumer_id: "key-1".to_owned(),
            user_id: "user-1".to_owned(),
            request_archive: false,
            intercept_degradation: false,
            is_admin: false,
            allow_debt: true,
        };
        let mut audit = AuditTracker::begin(
            &state,
            &identity,
            AuditStart {
                audit_id: "request-1",
                thread_id: Some("thread-1"),
                session_id: Some("session-1"),
                method: &Method::GET,
                path: "/v1/models",
                client_ip: "127.0.0.1",
                response_body_limit: ARCHIVE_BODY_LIMIT,
            },
        );
        let mut updated_config = (**state.config.load()).clone();
        updated_config.model_price_multiplier_nanos = 200_000_000;
        state.config.store(std::sync::Arc::new(updated_config));
        audit.set_affinity(Some("key-1:session-1"), Some("session-id"));
        audit.set_reasoning_effort(Some("high"));
        audit.set_fast_mode(true);
        audit.mark_first_byte();
        audit.set_request_size(42);
        audit.set_response_size(64);
        audit.finish(
            StatusCode::OK,
            Some("gpt-5.4"),
            Usage {
                input_tokens: 12,
                output_tokens: 4,
                cached_tokens: 3,
                cache_write_tokens: 0,
            },
            None,
        );
        wait_for_audits(&state, 1).await;
        let row: (i64, i64, i64, Option<i64>, i64, i64, i64, String, String) = sqlx::query_as("SELECT input_tokens,output_tokens,cached_tokens,first_byte_latency_ms,request_bytes,response_bytes,COUNT(*),affinity_hash,affinity_source FROM api_calls WHERE request_id='request-1'")
            .fetch_one(&state.db).await.unwrap();
        assert_eq!(
            row,
            (
                12,
                4,
                3,
                Some(1),
                42,
                64,
                1,
                affinity_hash("key-1:session-1"),
                "session-id".to_owned(),
            )
        );
        let costs: (i64, i64, i64) =
            sqlx::query_as("SELECT official_cost_usd_nanos,actual_cost_usd_nanos,price_multiplier_nanos FROM api_calls WHERE request_id='request-1'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(costs, (83_250, 16_650, 200_000_000));
        let effort: Option<String> = sqlx::query_scalar(
            "SELECT reasoning_effort FROM api_calls WHERE request_id='request-1'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(effort.as_deref(), Some("high"));
        let session_id: Option<String> =
            sqlx::query_scalar("SELECT session_id FROM api_calls WHERE request_id='request-1'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(session_id.as_deref(), Some("session-1"));
        let fast_mode: i64 =
            sqlx::query_scalar("SELECT fast_mode FROM api_calls WHERE request_id='request-1'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(fast_mode, 1);
        let archives: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_archives")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(archives, 0);
    }

    #[tokio::test]
    async fn stream_cancellation_preserves_captured_usage() {
        let state = crate::test_state("http://token.invalid").await;
        seed_proxy(&state, false).await;
        let identity = ApiIdentity {
            consumer_id: "key-1".to_owned(),
            user_id: "user-1".to_owned(),
            request_archive: true,
            intercept_degradation: false,
            is_admin: false,
            allow_debt: true,
        };
        let mut audit = AuditTracker::begin(
            &state,
            &identity,
            AuditStart {
                audit_id: "cancelled-stream",
                thread_id: Some("cancelled-thread"),
                session_id: None,
                method: &Method::POST,
                path: "/v1/responses",
                client_ip: "127.0.0.1",
                response_body_limit: ARCHIVE_BODY_LIMIT,
            },
        );
        let mut completion = audit.take_stream(StatusCode::OK, Some("gpt-5.5"));
        completion.capture_sse(
            concat!(
                "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{",
                "\"input_tokens\":11,\"output_tokens\":5,",
                "\"input_tokens_details\":{\"cached_tokens\":7}}}}\n\n",
            )
            .as_bytes(),
        );

        drop(completion);
        wait_for_audits(&state, 1).await;

        let row: (i64, i64, i64, i64, String, bool) = sqlx::query_as(
            "SELECT c.input_tokens,c.cached_tokens,c.output_tokens,c.status,c.error,a.response_body_truncated FROM api_calls c JOIN request_archives a ON a.api_call_id=c.id WHERE c.request_id='cancelled-stream'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, (11, 7, 5, 499, "client_cancelled".to_owned(), true));
        let mut audit = AuditTracker::begin(
            &state,
            &identity,
            AuditStart {
                audit_id: "cancelled-after-failure",
                thread_id: None,
                session_id: None,
                method: &Method::POST,
                path: "/v1/responses",
                client_ip: "127.0.0.1",
                response_body_limit: ARCHIVE_BODY_LIMIT,
            },
        );
        let mut completion = audit.take_stream(StatusCode::OK, Some("gpt-5.5"));
        completion.capture_sse(b"event: response.failed\ndata:{\"response\":{\"error\":{\"code\":\"server_is_overloaded\",\"message\":\"busy\"}}}\n\n");
        drop(completion);
        wait_for_audits(&state, 2).await;
        let failure: (String, String) = sqlx::query_as(
            "SELECT error_code,error FROM api_calls WHERE request_id='cancelled-after-failure'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            failure,
            ("server_is_overloaded".to_owned(), "busy".to_owned())
        );
    }

    #[tokio::test]
    async fn requested_stream_audits_real_sse_without_content_type() {
        let (upstream, _) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"audit","stream":true}"#),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let audit_id = response
            .headers()
            .get("x-openai-lb-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.ends_with(b"\n\n"));
        wait_for_audits(&state, 1).await;

        let row: (String, i64, i64, i64, i64, Option<String>) = sqlx::query_as(
            "SELECT id,input_tokens,cached_tokens,output_tokens,response_bytes,upstream_http_version FROM api_calls",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            (row.1, row.2, row.3, row.4, row.5),
            (19, 0, 6, body.len() as i64, Some("HTTP/1.1".to_owned()))
        );
        assert_eq!(audit_id, row.0);
    }

    #[tokio::test]
    async fn audit_survives_provider_deletion_during_an_inflight_call() {
        let state = crate::test_state("http://token.invalid").await;
        seed_proxy(&state, true).await;
        let identity = ApiIdentity {
            consumer_id: "key-1".to_owned(),
            user_id: "user-1".to_owned(),
            request_archive: true,
            intercept_degradation: false,
            is_admin: false,
            allow_debt: true,
        };
        let mut audit = AuditTracker::begin(
            &state,
            &identity,
            AuditStart {
                audit_id: "deleted-provider-request",
                thread_id: Some("deleted-provider-thread"),
                session_id: None,
                method: &Method::POST,
                path: "/v1/responses",
                client_ip: "127.0.0.1",
                response_body_limit: ARCHIVE_BODY_LIMIT,
            },
        );
        audit.event.as_mut().unwrap().provider_id = Some("provider-1".to_owned());
        sqlx::query("UPDATE providers SET is_deleted=1 WHERE id='provider-1'")
            .execute(&state.db)
            .await
            .unwrap();
        audit.finish(StatusCode::OK, Some("gpt-5.4"), Usage::default(), None);

        wait_for_audits(&state, 1).await;
        let row: (i64, Option<String>) = sqlx::query_as(
            "SELECT status,provider_id FROM api_calls WHERE request_id='deleted-provider-request'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, (200, Some("provider-1".to_owned())));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_proxy_calls_batch_audit_without_sqlite_contention() {
        let (upstream, _) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let app = crate::router(state.clone());
        let mut tasks = Vec::new();
        for _ in 0..200 {
            let service = app.clone();
            tasks.push(tokio::spawn(async move {
                service
                    .oneshot(proxy_request(
                        "/v1/responses",
                        "application/json",
                        Body::from(r#"{"model":"gpt-5.4","input":"audit"}"#),
                    ))
                    .await
                    .unwrap()
                    .status()
            }));
        }
        for task in tasks {
            assert_eq!(task.await.unwrap(), StatusCode::OK);
        }
        wait_for_audits(&state, 200).await;
        let row: (i64, i64) =
            sqlx::query_as("SELECT COUNT(*),COUNT(DISTINCT request_id) FROM api_calls")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(row, (200, 200));
    }

    #[tokio::test]
    async fn routes_forward_requested_headers_and_preserve_binary_audio() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let app = crate::router(state.clone());
        let mut request = proxy_request(
            "/v1/responses",
            "application/vnd.client+json",
            Body::from(r#"{"model":"gpt-5.4","input":"hello"}"#),
        );
        let headers = request.headers_mut();
        headers.insert("session-id", HeaderValue::from_static("client-session"));
        headers.insert("thread-id", HeaderValue::from_static("client-thread"));
        headers.insert(
            "x-client-request-id",
            HeaderValue::from_static("client-request"),
        );
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .clone()
            .oneshot(proxy_request(
                "/backend-api/codex/responses/compact",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"compact"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let audio = Bytes::from_static(b"\0RIFF\xffbinary-audio");
        let response = app
            .clone()
            .oneshot(proxy_request(
                "/v1/audio/transcriptions",
                "multipart/form-data; boundary=test",
                Body::from(audio.clone()),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            audio
        );
        let response = app
            .clone()
            .oneshot(proxy_request(
                "/v1/models",
                "application/json",
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let models: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert!(
            models
                .pointer("/data")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .any(|model| model.get("id").and_then(Value::as_str) == Some("gpt-image-2"))
        );
        let response = app
            .oneshot(proxy_request(
                "/v1/images/generations",
                "application/json",
                Body::from(
                    r#"{"model":"gpt-image-2","prompt":"diagram","n":1,"size":"2048x1152"}"#,
                ),
            ))
            .await
            .unwrap();
        let image: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(
            image.pointer("/data/0/b64_json").and_then(Value::as_str),
            Some("aW1hZ2U=")
        );

        let records = records.lock().await;
        assert_eq!(
            records
                .iter()
                .map(|record| record.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "/responses",
                "/responses/compact",
                "/transcribe",
                "/responses"
            ]
        );
        for record in records.iter() {
            assert!(record.headers.get("x-request-id").is_none());
            assert!(record.headers.get("x-lb-affinity-key").is_none());
            assert!(record.headers.get("x-session-id").is_none());
            assert!(record.headers.get("x-codex-session-id").is_none());
            assert!(record.headers.get("cookie").is_none());
            assert!(record.headers.get("x-arbitrary").is_none());
            assert_eq!(
                record.headers.get(header::AUTHORIZATION).unwrap(),
                "Bearer access-token"
            );
        }
        let image_request: Value = serde_json::from_slice(&records[3].body).unwrap();
        assert_eq!(
            image_request
                .pointer("/tools/0/size")
                .and_then(Value::as_str),
            Some("2048x1152")
        );
        let first = &records[0];
        assert_eq!(
            first
                .headers
                .get_all("originator")
                .iter()
                .collect::<Vec<_>>(),
            vec![&HeaderValue::from_static(CODEX_ORIGINATOR)]
        );
        assert_eq!(first.headers.get("session-id").unwrap(), "client-session");
        assert_eq!(first.headers.get("thread-id").unwrap(), "client-thread");
        assert_eq!(
            first.headers.get("x-client-request-id").unwrap(),
            "client-request"
        );
        assert_eq!(
            records[1]
                .headers
                .get_all("originator")
                .iter()
                .collect::<Vec<_>>(),
            vec![&HeaderValue::from_static(CODEX_ORIGINATOR)]
        );
        let response_body: Value = serde_json::from_slice(&records[0].body).unwrap();
        assert_eq!(response_body.get("store"), Some(&Value::Bool(false)));
        assert!(response_body.get("instructions").is_some());
        let compact_body: Value = serde_json::from_slice(&records[1].body).unwrap();
        assert!(compact_body.get("store").is_none());
        for record in [&records[0], &records[1], &records[3]] {
            assert_eq!(
                record.headers.get_all(header::CONTENT_TYPE).iter().count(),
                1
            );
            assert_eq!(
                record.headers.get(header::CONTENT_TYPE).unwrap(),
                "application/json"
            );
        }
        assert_eq!(
            records[2].headers.get(header::CONTENT_TYPE).unwrap(),
            "multipart/form-data; boundary=test"
        );
        assert_eq!(
            records[2].headers.get("originator").unwrap(),
            CODEX_ORIGINATOR
        );
        assert_eq!(
            records[2].headers.get(header::USER_AGENT).unwrap(),
            TRANSCRIPTION_USER_AGENT
        );
        assert_eq!(records[2].body, audio);
        drop(records);
        wait_for_audits(&state, 4).await;
        let model_calls: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM api_calls WHERE path='/v1/models'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(model_calls, 0);
        let archive: (String, Option<String>, Vec<u8>, Vec<u8>, bool, bool) = sqlx::query_as(
            "SELECT a.request_headers_json,a.upstream_request_headers_json,a.request_body,a.response_body,a.request_body_truncated,a.response_body_truncated FROM request_archives a JOIN api_calls c ON c.id=a.api_call_id WHERE c.path='/v1/responses' AND a.request_body LIKE '%hello%'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert!(!archive.0.contains("authorization"));
        assert!(!archive.0.contains("sk-test-secret"));
        assert!(archive.0.contains("session-secret"));
        assert!(archive.0.contains("client-request-id"));
        let upstream_headers = archive.1.as_deref().unwrap();
        assert!(!upstream_headers.contains("authorization"));
        assert!(!upstream_headers.contains("access-token"));
        assert!(std::str::from_utf8(&archive.2).unwrap().contains("hello"));
        assert!(std::str::from_utf8(&archive.3).unwrap().contains("resp"));
        assert!(!archive.4);
        assert!(!archive.5);

        let image_audit: (String, Vec<u8>, i64, i64) = sqlx::query_as(
            "SELECT c.path,a.request_body,c.input_tokens,c.output_tokens FROM api_calls c JOIN request_archives a ON a.api_call_id=c.id WHERE a.request_body LIKE '%image_generation%'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(image_audit.0, "/v1/responses");
        let image_audit_request: Value = serde_json::from_slice(&image_audit.1).unwrap();
        assert_eq!(image_audit_request["tools"][0]["type"], "image_generation");
        assert_eq!(image_audit_request["tools"][0]["model"], "gpt-image-2");
        assert_eq!(image_audit.2, 2);
        assert_eq!(image_audit.3, 3);

        let archived_audio: Vec<u8> = sqlx::query_scalar(
            "SELECT a.request_body FROM request_archives a JOIN api_calls c ON c.id=a.api_call_id WHERE c.path='/v1/audio/transcriptions'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(archived_audio, audio);
        let audio_sizes: (i64, i64) = sqlx::query_as(
            "SELECT request_bytes,response_bytes FROM api_calls WHERE path='/v1/audio/transcriptions'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(audio_sizes, (audio.len() as i64, audio.len() as i64));
    }

    #[tokio::test]
    async fn experimental_312_turn_state_filter_is_disabled_by_default_and_scoped() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let app = crate::router(state.clone());
        let turn_state_312 = HeaderValue::from_str(&"x".repeat(312)).unwrap();
        let turn_state_311 = HeaderValue::from_str(&"x".repeat(311)).unwrap();

        let mut config = (**state.config.load()).clone();
        config.experimental_filter_codex_turn_state_312 = true;
        state.config.store(Arc::new(config));
        for value in [turn_state_312.clone(), turn_state_311] {
            let mut request = proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"filter"}"#),
            );
            request.headers_mut().insert("x-codex-turn-state", value);
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            response.into_body().collect().await.unwrap();
        }
        let mut config = (**state.config.load()).clone();
        config.experimental_filter_codex_turn_state_312 = false;
        state.config.store(Arc::new(config));
        let mut request = proxy_request(
            "/v1/responses",
            "application/json",
            Body::from(r#"{"model":"gpt-5.4","input":"disabled"}"#),
        );
        request
            .headers_mut()
            .insert("x-codex-turn-state", turn_state_312);
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.into_body().collect().await.unwrap();

        let records = records.lock().await;
        assert!(records[0].headers.get("x-codex-turn-state").is_none());
        assert_eq!(
            records[1]
                .headers
                .get("x-codex-turn-state")
                .unwrap()
                .as_bytes()
                .len(),
            311
        );
        assert_eq!(
            records[2]
                .headers
                .get("x-codex-turn-state")
                .unwrap()
                .as_bytes()
                .len(),
            312
        );
    }

    #[tokio::test]
    async fn consumer_degradation_interception_cancels_312_turn_state_responses() {
        let upstream = Router::new().route(
            "/responses",
            post(|headers: HeaderMap| async move {
                let session = headers
                    .get("session-id")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default();
                let length = if session == "short-311" { 311 } else { 312 };
                let builder = Response::builder().status(StatusCode::OK).header(
                    "x-codex-turn-state",
                    HeaderValue::from_str(&"x".repeat(length)).unwrap(),
                );
                if session == "stream-312" {
                    builder
                        .header(header::CONTENT_TYPE, "text/event-stream")
                        .body(Body::from(
                            "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n\n",
                        ))
                        .unwrap()
                } else {
                    builder
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            r#"{"id":"resp","usage":{"input_tokens":1,"output_tokens":2}}"#,
                        ))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let state =
            crate::test_state_with_upstream("http://token.invalid", &format!("http://{address}"))
                .await;
        seed_proxy(&state, true).await;

        // Default off: the 312 response passes through with its signal intact.
        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"degraded"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["x-codex-turn-state"].as_bytes().len(),
            312
        );
        response.into_body().collect().await.unwrap();

        sqlx::query("UPDATE consumers SET intercept_degradation=1 WHERE id='key-1'")
            .execute(&state.db)
            .await
            .unwrap();

        // Opted in: buffered and streaming 312 responses are cancelled before the body
        // reaches the caller.
        for (session, body) in [
            ("degraded-312", r#"{"model":"gpt-5.4","input":"degraded"}"#),
            (
                "stream-312",
                r#"{"model":"gpt-5.4","input":"degraded","stream":true}"#,
            ),
        ] {
            let mut request = proxy_request("/v1/responses", "application/json", Body::from(body));
            request
                .headers_mut()
                .insert("session-id", HeaderValue::from_str(session).unwrap());
            let response = crate::router(state.clone()).oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(response.headers().get("x-codex-turn-state").is_none());
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let body: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["error"]["reason"], "degradation_intercepted");
            assert_eq!(body["error"]["code"], 503);
        }

        // Only the exact 312-byte length counts as the degradation signal.
        let mut request = proxy_request(
            "/v1/responses",
            "application/json",
            Body::from(r#"{"model":"gpt-5.4","input":"short"}"#),
        );
        request
            .headers_mut()
            .insert("session-id", HeaderValue::from_static("short-311"));
        let response = crate::router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["x-codex-turn-state"].as_bytes().len(),
            311
        );
        response.into_body().collect().await.unwrap();

        wait_for_audits(&state, 4).await;
        let intercepted: Vec<(i64, Option<String>)> = sqlx::query_as(
            "SELECT status,error FROM api_calls WHERE error_code='degradation_intercepted'",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(intercepted.len(), 2);
        for (status, error) in intercepted {
            assert_eq!(status, 503);
            assert!(error.unwrap().contains("降级已拦截"));
        }
    }

    #[tokio::test]
    async fn audio_retries_another_provider_from_the_spool_file() {
        let records = Arc::new(Mutex::new(Vec::<RecordedRequest>::new()));
        let recorded = records.clone();
        let upstream = Router::new().route(
            "/transcribe",
            post(move |headers: HeaderMap, body: Bytes| {
                let recorded = recorded.clone();
                async move {
                    recorded.lock().await.push(RecordedRequest {
                        path: "/transcribe".to_owned(),
                        headers: headers.clone(),
                        body: body.clone(),
                    });
                    if headers.get(header::AUTHORIZATION).unwrap() == "Bearer access-token" {
                        return Response::builder()
                            .status(StatusCode::TOO_MANY_REQUESTS)
                            .header("retry-after", "30")
                            .body(Body::from(r#"{"error":{"message":"limited"}}"#))
                            .unwrap();
                    }
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/octet-stream")
                        .body(Body::from(body))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });

        let state =
            crate::test_state_with_upstream("http://token.invalid", &format!("http://{address}"))
                .await;
        let data_dir = std::env::temp_dir().join(format!("openai-lb-audio-{}", Uuid::new_v4()));
        let mut config = (**state.config.load()).clone();
        config.data_dir = data_dir.clone();
        state.config.store(Arc::new(config));
        seed_proxy(&state, true).await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,status,created_at,updated_at,owner_id) VALUES('provider-2','two','account-2','access-token-2','refresh-token-2','active',?,?,'user-1')")
            .bind(now).bind(now).execute(&state.db).await.unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();

        let audio = Bytes::from_static(b"\0RIFF\xffretryable-audio");
        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/audio/transcriptions",
                "multipart/form-data; boundary=test",
                Body::from(audio.clone()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            audio
        );

        let records = records.lock().await;
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].body, audio);
        assert_eq!(records[1].body, audio);
        assert_eq!(
            records[1].headers.get(header::AUTHORIZATION).unwrap(),
            "Bearer access-token-2"
        );
        drop(records);
        wait_for_audits(&state, 1).await;
        let provider_id: String = sqlx::query_scalar(
            "SELECT provider_id FROM api_calls WHERE path='/v1/audio/transcriptions'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(provider_id, "provider-2");

        let mut entries = tokio::fs::read_dir(crate::audio_spool::directory(&data_dir))
            .await
            .unwrap();
        assert!(entries.next_entry().await.unwrap().is_none());
        tokio::fs::remove_dir_all(data_dir).await.unwrap();
    }

    #[tokio::test]
    async fn network_failure_is_returned_and_archived() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (connection, _) = listener.accept().await.unwrap();
            drop(connection);
        });
        let state =
            crate::test_state_with_upstream("http://token.invalid", &format!("http://{address}"))
                .await;
        seed_proxy(&state, true).await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"network"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        wait_for_audits(&state, 1).await;

        let archived: (i64, String, Vec<u8>, Vec<u8>) = sqlx::query_as(
            "SELECT c.status,c.error,a.request_body,a.response_body FROM api_calls c JOIN request_archives a ON a.api_call_id=c.id",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(archived.0, 500);
        assert_eq!(archived.1, "internal server error");
        assert!(
            std::str::from_utf8(&archived.2)
                .unwrap()
                .contains("network")
        );
        assert!(
            std::str::from_utf8(&archived.3)
                .unwrap()
                .contains("internal server error")
        );
    }

    async fn wait_for_provider_queue(state: &AppState, queued: usize) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while state.balancer.load("provider-1").queued != queued {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("provider queue reached expected length");
    }

    #[tokio::test]
    async fn cancelled_response_archives_downstream_headers_and_transport_bytes() {
        for read_first_chunk in [false, true] {
            let (upstream, _) = spawn_stream_mock(false).await;
            let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
            seed_proxy(&state, true).await;
            let response = crate::router(state.clone())
                .oneshot(proxy_request(
                    "/v1/responses",
                    "application/json",
                    Body::from(r#"{"model":"gpt-5.4","input":"cancel","stream":true}"#),
                ))
                .await
                .unwrap();
            let id = response.headers()["x-openai-lb-request-id"]
                .to_str()
                .unwrap()
                .to_owned();
            let expected_headers = archive_headers(response.headers());
            let mut body = response.into_body();
            let expected_bytes = if read_first_chunk {
                body.frame()
                    .await
                    .unwrap()
                    .unwrap()
                    .into_data()
                    .unwrap()
                    .len() as i64
            } else {
                0
            };
            drop(body);

            let recorded = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let row: Option<(i64, i64, Option<String>, Option<String>)> = sqlx::query_as(
                        "SELECT c.status,c.response_transport_bytes,c.downstream_content_encoding,a.downstream_response_headers_json FROM api_calls c JOIN request_archives a ON a.api_call_id=c.id WHERE c.id=?",
                    )
                    .bind(&id)
                    .fetch_optional(&state.db)
                    .await
                    .unwrap();
                    if let Some(row) = row
                        && row.3.is_some()
                    {
                        break row;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("cancelled responses retain downstream header diagnostics");
            assert_eq!(
                recorded,
                (
                    499,
                    expected_bytes,
                    Some("identity".to_owned()),
                    Some(expected_headers)
                )
            );
        }
    }

    #[tokio::test]
    async fn stream_slots_last_until_body_completion_or_cancellation() {
        for complete in [false, true] {
            let (upstream, _) = spawn_stream_mock(false).await;
            let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
            seed_proxy(&state, true).await;
            state.balancer.set_concurrency_limit(1);
            let app = crate::router(state.clone());
            let request = || {
                proxy_request(
                    "/v1/responses",
                    "application/json",
                    Body::from(r#"{"model":"gpt-5.4","input":"queue","stream":true}"#),
                )
            };
            let response = app.clone().oneshot(request()).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let mut body = response.into_body();
            assert!(body.frame().await.unwrap().is_ok());
            assert_eq!(state.balancer.inflight("provider-1"), 1);
            let waiting = tokio::spawn(app.clone().oneshot(request()));
            wait_for_provider_queue(&state, 1).await;
            let cancelled = tokio::spawn(app.clone().oneshot(request()));
            wait_for_provider_queue(&state, 2).await;
            cancelled.abort();
            assert!(cancelled.await.unwrap_err().is_cancelled());
            wait_for_provider_queue(&state, 1).await;
            if complete {
                body.collect().await.unwrap();
            } else {
                drop(body);
            }
            let response = tokio::time::timeout(Duration::from_secs(3), waiting)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(state.balancer.inflight("provider-1"), 1);
            assert_eq!(state.balancer.load("provider-1").queued, 0);
            response.into_body().collect().await.unwrap();
            assert_eq!(state.balancer.inflight("provider-1"), 0);
        }
    }

    #[tokio::test]
    async fn saturated_providers_can_retry_each_other_without_holding_old_slots() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let app = Router::new().route("/responses", post(move || {
            let seen = seen.clone();
            let barrier = barrier.clone();
            async move {
                if seen.fetch_add(1, Ordering::SeqCst) < 2 {
                    barrier.wait().await;
                    (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":{"message":"retry"}})))
                } else {
                    (StatusCode::OK, Json(json!({"id":"resp_retry","output":[],"usage":{"input_tokens":1,"output_tokens":1}})))
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let state =
            crate::test_state_with_upstream("http://token.invalid", &format!("http://{address}"))
                .await;
        seed_proxy(&state, true).await;
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,created_at,updated_at,visibility) VALUES('provider-2','two','account-2','access','refresh',0,0,'public')").execute(&state.db).await.unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
        state.balancer.set_concurrency_limit(1);
        state
            .balancer
            .pin_affinity(&state, "key-1:left", "provider-1");
        state
            .balancer
            .pin_affinity(&state, "key-1:right", "provider-2");
        let app = crate::router(state.clone());
        let request = |affinity: &'static str| {
            let mut request = proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"retry"}"#),
            );
            request
                .headers_mut()
                .insert("session-id", HeaderValue::from_static(affinity));
            request
        };
        let responses = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(
                app.clone().oneshot(request("left")),
                app.oneshot(request("right"))
            )
        })
        .await
        .expect("retries must not deadlock");
        for response in [responses.0.unwrap(), responses.1.unwrap()] {
            assert_eq!(response.status(), StatusCode::OK);
            response.into_body().collect().await.unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(state.balancer.inflight("provider-1"), 0);
        assert_eq!(state.balancer.inflight("provider-2"), 0);
    }

    #[tokio::test]
    async fn streaming_response_body_is_archived_and_interruption_is_marked_truncated() {
        for failed in [false, true] {
            let (upstream, interrupt) = spawn_stream_mock(failed).await;
            let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
            seed_proxy(&state, true).await;
            let response = crate::router(state.clone())
                .oneshot(proxy_request(
                    "/v1/responses",
                    "application/json",
                    Body::from(r#"{"model":"gpt-5.4","input":"stream","stream":true}"#),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            if failed {
                interrupt.notify_one();
            }
            let client_body = response.into_body().collect().await.unwrap().to_bytes();
            assert!(std::str::from_utf8(&client_body).unwrap().contains("hello"));
            wait_for_audits(&state, 1).await;

            let archived: (i64, Option<String>, Vec<u8>, bool) = sqlx::query_as(
                "SELECT c.status,c.error,a.response_body,a.response_body_truncated FROM api_calls c JOIN request_archives a ON a.api_call_id=c.id",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert!(std::str::from_utf8(&archived.2).unwrap().contains("hello"));
            if failed {
                assert_eq!(archived.0, 502);
                assert_eq!(archived.1.as_deref(), Some("upstream_stream_error"));
                assert!(archived.3);
            } else {
                assert_eq!(archived.0, 200);
                assert_eq!(archived.1, None);
                assert!(!archived.3);
                assert!(
                    std::str::from_utf8(&archived.2)
                        .unwrap()
                        .contains("response.completed")
                );
            }
        }
    }

    #[tokio::test]
    async fn temporary_archive_write_failure_does_not_change_response_and_retries() {
        let (upstream, _) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        sqlx::query("DROP TABLE request_archives")
            .execute(&state.db)
            .await
            .unwrap();
        let retries_before = crate::audit::write_retries();

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"archive"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&body).unwrap().contains("resp"));

        for _ in 0..100 {
            if crate::audit::write_retries() > retries_before {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(crate::audit::write_retries() > retries_before);
        sqlx::query(
            "CREATE TABLE request_archives (api_call_id TEXT PRIMARY KEY REFERENCES api_calls(id) ON DELETE CASCADE,request_headers_json TEXT NOT NULL,upstream_request_headers_json TEXT,request_body BLOB NOT NULL,request_body_truncated INTEGER NOT NULL CHECK(request_body_truncated IN (0,1)),response_headers_json TEXT,downstream_response_headers_json TEXT,response_body BLOB,response_body_truncated INTEGER NOT NULL CHECK(response_body_truncated IN (0,1)),created_at INTEGER NOT NULL)",
        )
        .execute(&state.db)
        .await
        .unwrap();

        wait_for_audits(&state, 1).await;
        let archives: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_archives")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(archives, 1);
    }

    #[tokio::test]
    async fn full_audit_queue_does_not_delay_proxy_or_capture_diagnostics() {
        let (upstream, _) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let mut reservations = Vec::new();
        while let Some(reservation) = state.audit.try_reserve() {
            reservations.push(reservation);
        }
        assert!(!reservations.is_empty());

        let dropped_before = crate::audit::dropped_events();
        let identity = ApiIdentity {
            consumer_id: "key-1".to_owned(),
            user_id: "user-1".to_owned(),
            request_archive: true,
            intercept_degradation: false,
            is_admin: false,
            allow_debt: true,
        };
        let mut audit = AuditTracker::begin(
            &state,
            &identity,
            AuditStart {
                audit_id: "dropped-diagnostic",
                thread_id: None,
                session_id: None,
                method: &Method::POST,
                path: "/v1/responses",
                client_ip: "127.0.0.1",
                response_body_limit: ARCHIVE_BODY_LIMIT,
            },
        );
        audit.set_request(&HeaderMap::new(), &vec![0; ARCHIVE_BODY_LIMIT], false);
        assert!(audit.event.is_none());
        drop(audit);

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            crate::router(state.clone()).oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"full queue"}"#),
            )),
        )
        .await
        .expect("full audit queue must not block the proxy")
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(crate::audit::dropped_events() >= dropped_before + 2);
        let calls: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_calls")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(calls, 0);

        drop(reservations);
    }

    #[tokio::test]
    async fn full_archive_budget_preserves_base_audit_without_diagnostics() {
        let (upstream, _) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let mut archive_budgets = Vec::new();
        while let Some(budget) = state.audit.try_reserve_archive(ARCHIVE_BODY_LIMIT) {
            archive_budgets.push(budget);
        }
        assert!(!archive_budgets.is_empty());

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"archive budget"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        wait_for_audits(&state, 1).await;
        let stored: (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*),COUNT(a.api_call_id) FROM api_calls c LEFT JOIN request_archives a ON a.api_call_id=c.id",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(stored, (1, 0));

        drop(archive_budgets);
    }

    #[tokio::test]
    async fn single_provider_error_is_forwarded_and_failed_calls_are_audited() {
        for expected in [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::BAD_GATEWAY,
        ] {
            let (upstream, _) = spawn_mock(expected).await;
            let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
            seed_proxy(&state, true).await;
            let response = crate::router(state.clone())
                .oneshot(proxy_request(
                    "/v1/responses",
                    "application/json",
                    Body::from(r#"{"model":"gpt-5.4","input":"hello"}"#),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert!(std::str::from_utf8(&body).unwrap().contains("limited"));
            wait_for_audits(&state, 1).await;
            let status: i64 = sqlx::query_scalar("SELECT status FROM api_calls")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(status, expected.as_u16() as i64);
            let archived_response: Vec<u8> =
                sqlx::query_scalar("SELECT response_body FROM request_archives")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert!(
                std::str::from_utf8(&archived_response)
                    .unwrap()
                    .contains("limited")
            );
        }

        let state = crate::test_state("http://token.invalid").await;
        seed_proxy(&state, false).await;
        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"hello"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let audit_id = response
            .headers()
            .get("x-openai-lb-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        wait_for_audits(&state, 1).await;
        let (persisted_id, status): (String, i64) =
            sqlx::query_as("SELECT id,status FROM api_calls")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(status, 503);
        assert_eq!(audit_id, persisted_id);
        let archived_response: Vec<u8> =
            sqlx::query_scalar("SELECT response_body FROM request_archives")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(
            std::str::from_utf8(&archived_response)
                .unwrap()
                .contains("no available provider for this downstream client")
        );
    }

    #[tokio::test]
    async fn usage_limit_429_retries_the_other_provider() {
        let now = chrono::Utc::now().timestamp();
        let (upstream, calls) = spawn_usage_limit_then_success(now + 3_600).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,status,created_at,updated_at,owner_id) VALUES('provider-2','two','account-2','access-2','refresh-2','active',?,?,'user-1')")
            .bind(now + 1)
            .bind(now + 1)
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"hello"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let selected = state
            .balancer
            .select(&state, None, None, test_downstream())
            .await
            .unwrap();
        assert_eq!(selected.provider.id, "provider-2");
    }

    #[tokio::test]
    async fn image_parameters_and_header_policy_fail_closed() {
        let state = crate::test_state("http://token.invalid").await;
        for payload in [
            json!({"prompt":"x","n":2}),
            json!({"prompt":"x","n":"1"}),
            json!({"prompt":"x","n":-1}),
            json!({"prompt":"x","stream":"false"}),
            json!({"prompt":"x","response_format":1}),
            json!({"prompt":"x","output_compression":"90"}),
            json!({"prompt":"x","output_compression":-1}),
            json!({"prompt":"x","model":4}),
            json!({"prompt":"x","model":"gpt-image-1","size":"2048x2048"}),
            json!({"prompt":"x","model":"gpt-image-2","size":"2047x2048"}),
            json!({"prompt":"x","model":"gpt-image-2","size":"2048x8192"}),
            json!({"prompt":"x","model":"gpt-image-2","size":"1024x1024x1024"}),
            json!({"prompt":"x","reference_images":"data:image/png;base64,cG5n"}),
            json!({"prompt":"x","reference_images":["data:image/svg+xml;base64,PHN2Zz4="]}),
            json!({"prompt":"x","reference_images":[
                "data:image/png;base64,x",
                "data:image/png;base64,x",
                "data:image/png;base64,x",
                "data:image/png;base64,x",
                "data:image/png;base64,x"
            ]}),
        ] {
            assert!(transform_request("/v1/images/generations", Some(payload), &state,).is_err());
        }
        for size in ["2048x2048", "2048x1152", "3840x2160", "2160x3840"] {
            assert!(
                transform_request(
                    "/v1/images/generations",
                    Some(json!({"prompt":"x","model":"gpt-image-2","size":size})),
                    &state,
                )
                .is_ok()
            );
        }
        for size in ["1024x1024", "1536x1024", "1024x1536"] {
            assert!(
                transform_request(
                    "/v1/images/generations",
                    Some(json!({"prompt":"x","model":"gpt-image-1.5","size":size})),
                    &state,
                )
                .is_ok()
            );
        }
        let mut headers = HeaderMap::new();
        headers.insert("x-lb-affinity-key", HeaderValue::from_static("secret"));
        assert!(!should_forward_request_header(
            "/v1/responses",
            headers.keys().next().unwrap()
        ));
    }

    #[tokio::test]
    async fn concurrent_refresh_is_singleflight() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let token_app = Router::new().route("/token", post(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Json(json!({"access_token":"access-new","refresh_token":"refresh-new","expires_in":3600}))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, token_app).await.unwrap();
        });
        let state = crate::test_state(&format!("http://{address}/token")).await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,expires_at,status,created_at,updated_at,visibility) VALUES('provider-1','one','account-1',?,?,?,'active',?,?,'public')")
            .bind("access-old")
            .bind("refresh-old")
            .bind(now - 1).bind(now).bind(now).execute(&state.db).await.unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
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
        let state_one = state.clone();
        let state_two = state.clone();
        let one = tokio::spawn(async move {
            let mut lease = first;
            refresh_if_needed(&state_one, &mut lease).await
        });
        let two = tokio::spawn(async move {
            let mut lease = second;
            refresh_if_needed(&state_two, &mut lease).await
        });
        assert!(one.await.unwrap().is_ok());
        assert!(two.await.unwrap().is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancelled_stream_finalizes_pending_audit() {
        let state = crate::test_state("http://token.invalid").await;
        seed_proxy(&state, false).await;
        let identity = ApiIdentity {
            consumer_id: "key-1".to_owned(),
            user_id: "user-1".to_owned(),
            request_archive: true,
            intercept_degradation: false,
            is_admin: false,
            allow_debt: true,
        };
        let mut audit = AuditTracker::begin(
            &state,
            &identity,
            AuditStart {
                audit_id: "stream-request",
                thread_id: Some("stream-thread"),
                session_id: None,
                method: &Method::POST,
                path: "/v1/responses",
                client_ip: "127.0.0.1",
                response_body_limit: ARCHIVE_BODY_LIMIT,
            },
        );
        drop(audit.take_stream(StatusCode::OK, Some("gpt-5.4")));
        wait_for_audits(&state, 1).await;
        let row: (i64, bool) = sqlx::query_as(
            "SELECT c.status,a.response_body_truncated FROM api_calls c JOIN request_archives a ON a.api_call_id=c.id WHERE c.request_id='stream-request'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, (499, true));
    }

    #[tokio::test]
    async fn abort_before_upstream_headers_finalizes_pending_audit() {
        let started = Arc::new(Notify::new());
        let signal = started.clone();
        let upstream_app = Router::new().route(
            "/responses",
            post(move || {
                let signal = signal.clone();
                async move {
                    signal.notify_one();
                    std::future::pending::<Response>().await
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, upstream_app).await.unwrap();
        });
        let state =
            crate::test_state_with_upstream("http://token.invalid", &format!("http://{address}"))
                .await;
        seed_proxy(&state, true).await;
        let app = crate::router(state.clone());
        let request_task = tokio::spawn(async move {
            app.oneshot(proxy_request(
                "/v1/responses",
                "application/json",
                Body::from(r#"{"model":"gpt-5.4","input":"wait"}"#),
            ))
            .await
        });
        started.notified().await;
        request_task.abort();
        assert!(request_task.await.unwrap_err().is_cancelled());
        wait_for_audits(&state, 1).await;
        let row: (i64, bool) = sqlx::query_as(
            "SELECT c.status,a.response_body_truncated FROM api_calls c JOIN request_archives a ON a.api_call_id=c.id",
        )
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(row, (499, true));
        let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_calls WHERE status=0")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(pending, 0);
    }

    async fn seed_pi_fallback(state: &AppState) {
        seed_provider_with_originator(state, "provider-pi", "pi").await;
        sqlx::query("UPDATE providers SET allow_other_originator=1 WHERE id='provider-pi'")
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
    }

    #[tokio::test]
    async fn cross_originator_fallback_uses_pi_headers_and_audits_without_archiving() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, false).await;
        sqlx::query("UPDATE consumers SET request_archive=0 WHERE id='key-1'")
            .execute(&state.db)
            .await
            .unwrap();
        seed_pi_fallback(&state).await;
        sqlx::query("UPDATE providers SET manual_disabled=1 WHERE id='provider-1'")
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
        let cases = [
            (
                CODEX_CLI_USER_AGENT,
                Some(CODEX_ORIGINATOR),
                Some("provider_pool_empty"),
            ),
            ("opencode/1", Some("opencode"), Some("provider_pool_empty")),
            (
                "curl/8",
                Some("custom-agent"),
                Some("downstream_client_unknown"),
            ),
            ("curl/8", None, Some("downstream_client_unknown")),
            ("pi/1", None, None),
        ];
        for (user_agent, originator, _) in cases {
            let response = crate::router(state.clone())
                .oneshot(client_request(
                    "/v1/responses",
                    user_agent,
                    originator,
                    Body::from(r#"{"model":"gpt-5.4","input":"fallback"}"#),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            response.into_body().collect().await.unwrap();
        }
        wait_for_audits(&state, cases.len() as i64).await;
        let rows = sqlx::query("SELECT id,provider_id,downstream_originator,upstream_originator,originator_fallback_reason FROM api_calls ORDER BY rowid")
            .fetch_all(&state.db).await.unwrap();
        let recorded = records.lock().await;
        assert_eq!(recorded.len(), cases.len());
        for ((row, record), (_, originator, reason)) in rows.iter().zip(recorded.iter()).zip(cases)
        {
            assert_eq!(row.get::<String, _>("provider_id"), "provider-pi");
            assert_eq!(
                row.get::<Option<String>, _>("downstream_originator")
                    .as_deref(),
                originator
            );
            assert_eq!(row.get::<String, _>("upstream_originator"), "pi");
            assert_eq!(
                row.get::<Option<String>, _>("originator_fallback_reason")
                    .as_deref(),
                reason
            );
            assert_eq!(record.headers.get_all(ORIGINATOR_HEADER).iter().count(), 1);
            assert_eq!(record.headers[ORIGINATOR_HEADER], "pi");
            assert_eq!(
                record.headers[header::USER_AGENT],
                if reason.is_some() { "pi" } else { "pi/1" }
            );
        }
        drop(recorded);
        let archives: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_archives")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(archives, 0);
        let detail = crate::api::audit_detail(
            State(state.clone()),
            Extension(UserIdentity {
                id: "user-1".to_owned(),
                email: None,
                name: None,
                role: "user".to_owned(),
            }),
            axum::extract::Path(rows[0].get::<String, _>("id")),
        )
        .await
        .unwrap();
        assert_eq!(detail.0["upstream_originator"], "pi");
        assert_eq!(
            detail.0["originator_fallback_reason"],
            "provider_pool_empty"
        );

        let mut config = (**state.config.load()).clone();
        config.upstream_user_agents.pi = Some("pi/override".to_owned());
        state.config.store(Arc::new(config));
        let response = crate::router(state.clone())
            .oneshot(client_request(
                "/v1/responses",
                CODEX_CLI_USER_AGENT,
                Some(CODEX_ORIGINATOR),
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.into_body().collect().await.unwrap();
        assert_eq!(
            records.lock().await.last().unwrap().headers[header::USER_AGENT],
            "pi/override"
        );
    }

    #[tokio::test]
    async fn originator_fallback_does_not_bypass_authentication_conflicts_or_capabilities() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, false).await;
        seed_pi_fallback(&state).await;
        let response = crate::router(state.clone())
            .oneshot(client_request(
                "/v1/responses",
                CODEX_CLI_USER_AGENT,
                Some("pi"),
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let error: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(error["error"]["reason"], "downstream_identity_mismatch");
        let mut unauthenticated = client_request(
            "/v1/responses",
            "curl/8",
            Some("custom"),
            Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
        );
        unauthenticated.headers_mut().remove(header::AUTHORIZATION);
        let response = crate::router(state.clone())
            .oneshot(unauthenticated)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = crate::router(state.clone())
            .oneshot(client_request(
                "/v1/audio/transcriptions",
                "pi/1",
                Some("pi"),
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(records.lock().await.is_empty());
    }

    #[tokio::test]
    async fn retry_can_use_pi_fallback_and_keeps_the_existing_two_attempt_limit() {
        let (upstream, records) = spawn_mock(StatusCode::SERVICE_UNAVAILABLE).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        seed_pi_fallback(&state).await;
        let response = crate::router(state.clone())
            .oneshot(client_request(
                "/v1/responses",
                CODEX_CLI_USER_AGENT,
                Some(CODEX_ORIGINATOR),
                Body::from(r#"{"model":"gpt-5.4","input":"x"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        response.into_body().collect().await.unwrap();
        let recorded = records.lock().await;
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].headers[ORIGINATOR_HEADER], CODEX_ORIGINATOR);
        assert_eq!(recorded[1].headers[ORIGINATOR_HEADER], "pi");
        assert_eq!(recorded[1].headers[header::USER_AGENT], "pi");
        drop(recorded);
        wait_for_audits(&state, 1).await;
        let reason: String = sqlx::query_scalar("SELECT originator_fallback_reason FROM api_calls")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(reason, "provider_pool_empty");
        assert_eq!(state.balancer.inflight("provider-1"), 0);
        assert_eq!(state.balancer.inflight("provider-pi"), 0);
    }

    #[tokio::test]
    async fn realtime_fallback_websocket_uses_the_pinned_pi_identity() {
        let state = crate::test_state("http://token.invalid").await;
        seed_proxy(&state, false).await;
        seed_pi_fallback(&state).await;
        let lease = state
            .balancer
            .select_provider("provider-pi", crate::test_downstream())
            .await
            .unwrap();
        let mut inbound = HeaderMap::new();
        inbound.insert(
            header::USER_AGENT,
            HeaderValue::from_static(CODEX_CLI_USER_AGENT),
        );
        inbound.insert(
            ORIGINATOR_HEADER,
            HeaderValue::from_static(CODEX_ORIGINATOR),
        );
        let request =
            realtime_upstream_websocket_request(&state, &lease, &inbound, "rtc_fallback").unwrap();
        assert_eq!(request.headers()[ORIGINATOR_HEADER], "pi");
        assert_eq!(request.headers()[header::USER_AGENT], "pi");
        assert_eq!(
            request.headers().get_all(ORIGINATOR_HEADER).iter().count(),
            1
        );
    }
    #[tokio::test]
    async fn transcription_fallback_preserves_desktop_ua_and_pi_originator() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, false).await;
        seed_pi_fallback(&state).await;
        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/audio/transcriptions",
                "multipart/form-data; boundary=test",
                Body::from("audio"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.into_body().collect().await.unwrap();
        let records = records.lock().await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].headers[ORIGINATOR_HEADER], "pi");
        assert_eq!(
            records[0].headers[header::USER_AGENT],
            TRANSCRIPTION_USER_AGENT
        );
    }

    /// Seeds a Pi Agent state whose only enabled provider is authorized for the Pi identity.
    async fn seed_pi_proxy() -> (crate::AppState, Arc<Mutex<Vec<RecordedRequest>>>) {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, false).await;
        seed_provider_with_originator(&state, "provider-pi", "pi").await;
        (state, records)
    }

    #[tokio::test]
    async fn pi_requests_forward_exactly_the_headers_pi_sends() {
        let (state, records) = seed_pi_proxy().await;
        let mut request = pi_request(
            "/v1/responses",
            Body::from(zstd_body(r#"{"model":"gpt-5.4","input":"hello"}"#)),
        );
        let headers = request.headers_mut();
        // Everything below is a header Pi never sends, plus the values the proxy must derive
        // instead of trusting.
        headers.insert("accept", HeaderValue::from_static("*/*"));
        headers.insert("accept-language", HeaderValue::from_static("zh-CN"));
        headers.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
        headers.insert("thread-id", HeaderValue::from_static("client-thread"));
        headers.insert("x-codex-turn-state", HeaderValue::from_static("turn-state"));
        headers.insert(
            "x-codex-turn-metadata",
            HeaderValue::from_static(r#"{"installation_id":"client"}"#),
        );
        headers.insert(
            "x-openai-internal-codex-responses-lite",
            HeaderValue::from_static("1"),
        );
        headers.insert("x-api-key", HeaderValue::from_static("secret"));
        headers.insert("content-encoding", HeaderValue::from_static("zstd"));
        headers.insert(
            "x-client-request-id",
            HeaderValue::from_static("caller-supplied"),
        );
        let response = crate::router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.into_body().collect().await.unwrap();

        let headers = recorded_headers(&records).await;
        // Derived or fixed by the proxy.
        assert_eq!(headers["session-id"], PI_SESSION_ID);
        assert_eq!(headers["x-client-request-id"], PI_SESSION_ID);
        assert_eq!(headers[header::ACCEPT], "text/event-stream");
        assert_eq!(headers["accept-language"], "*");
        assert_eq!(headers["sec-fetch-mode"], "cors");
        assert_eq!(headers["openai-beta"], PI_OPENAI_BETA);
        assert_eq!(headers[header::CONTENT_TYPE], "application/json");
        // Forwarded from Pi's own set.
        assert_eq!(headers["content-encoding"], "zstd");
        assert_eq!(headers[ORIGINATOR_HEADER], "pi");
        assert_eq!(headers[header::USER_AGENT], PI_USER_AGENT);
        assert_eq!(headers[header::AUTHORIZATION], "Bearer access-token");
        assert_eq!(headers["chatgpt-account-id"], "account-provider-pi");
        // Nothing else, including the transport headers the mock server records. `accept-encoding`
        // is present because Pi always negotiates response compression: the caller's value is
        // forwarded, and reqwest supplies its own when the caller sent none.
        let mut names = headers
            .keys()
            .map(|name| name.as_str().to_owned())
            .filter(|name| !matches!(name.as_str(), "host" | "content-length"))
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            vec![
                "accept",
                "accept-encoding",
                "accept-language",
                "authorization",
                "chatgpt-account-id",
                "content-encoding",
                "content-type",
                "openai-beta",
                "originator",
                "sec-fetch-mode",
                "session-id",
                "user-agent",
                "x-client-request-id",
            ]
        );
        assert!(!headers["accept-encoding"].is_empty());
    }

    #[tokio::test]
    async fn pi_requests_without_a_session_id_are_rejected_and_audited() {
        let (state, records) = seed_pi_proxy().await;
        // Pi's summarization calls omit the header; there is nothing to derive an id from, and a
        // synthesized one would carry a timestamp the upstream can read.
        let mut request = pi_request("/v1/responses", Body::from(r#"{"model":"gpt-5.4"}"#));
        request.headers_mut().remove("session-id");
        let response = crate::router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            response.headers().contains_key("x-openai-lb-request-id"),
            "the rejection must reach the call audit"
        );
        let error: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(error["error"]["reason"], "downstream_session_id_missing");
        assert!(
            records.lock().await.is_empty(),
            "a rejected request must never reach an upstream"
        );

        // The audit row records the rejection against the authenticating Consumer.
        wait_for_audits(&state, 1).await;
        let (status, session_id, consumer): (i64, Option<String>, String) =
            sqlx::query_as("SELECT status,session_id,consumer_id FROM api_calls")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(status, 400);
        assert_eq!(session_id, None);
        assert_eq!(consumer, "key-1");
    }

    #[tokio::test]
    async fn pi_requests_with_another_session_id_shape_are_rejected_and_audited() {
        let (state, records) = seed_pi_proxy().await;
        // A custom `--session-id`, a legacy value, and a UUID of the wrong version: OpenAI-LB rejects
        // each instead of substituting an id whose timestamp it would have had to invent.
        for session_id in [
            "my-task-1",
            "session-secret",
            "01a0bd2a-0c12-6123-a3df-36f9d077060e",
        ] {
            let mut request = pi_request("/v1/responses", Body::from(r#"{"model":"gpt-5.4"}"#));
            request
                .headers_mut()
                .insert("session-id", HeaderValue::from_str(session_id).unwrap());
            let response = crate::router(state.clone()).oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert!(
                response.headers().contains_key("x-openai-lb-request-id"),
                "the rejection must reach the call audit"
            );
            let error: Value =
                serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(error["error"]["reason"], "downstream_session_id_invalid");
        }
        assert!(
            records.lock().await.is_empty(),
            "a rejected request must never reach an upstream"
        );
        // Each rejection is recorded against the Consumer that sent it.
        wait_for_audits(&state, 3).await;
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT status,consumer_id FROM api_calls ORDER BY created_at,id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(rows.len(), 3);
        for (status, consumer) in rows {
            assert_eq!((status, consumer.as_str()), (400, "key-1"));
        }
    }

    #[tokio::test]
    async fn pi_requests_only_accept_the_zstd_encoding_pi_sends() {
        let (state, records) = seed_pi_proxy().await;
        let mut request = pi_request("/v1/responses", Body::from(r#"{"model":"gpt-5.4"}"#));
        request
            .headers_mut()
            .insert("content-encoding", HeaderValue::from_static("gzip"));
        let response = crate::router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let error: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(
            error["error"]["reason"],
            "downstream_content_encoding_invalid"
        );
        // A body that claims zstd without being zstd is refused for the same reason.
        let mut request = pi_request("/v1/responses", Body::from("not-zstd"));
        request
            .headers_mut()
            .insert("content-encoding", HeaderValue::from_static("zstd"));
        let response = crate::router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(records.lock().await.is_empty());
    }

    #[tokio::test]
    async fn pi_zstd_bodies_are_decoded_validated_and_re_encoded() {
        let (state, records) = seed_pi_proxy().await;
        let mut request = pi_request(
            "/v1/responses",
            Body::from(zstd_body(r#"{"model":"gpt-5.4","input":"hello"}"#)),
        );
        request
            .headers_mut()
            .insert("content-encoding", HeaderValue::from_static("zstd"));
        let response = crate::router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.into_body().collect().await.unwrap();

        let records = records.lock().await;
        assert_eq!(records.len(), 1);
        let upstream = &records[0];
        assert_eq!(upstream.headers["content-encoding"], PI_CONTENT_ENCODING);
        // The forwarded body is zstd again and carries the transformation the proxy applies to a
        // parsed request, which a body it could not decompress would never receive.
        let decoded = zstd::stream::decode_all(upstream.body.as_ref()).unwrap();
        let body: Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(body["model"], "gpt-5.4");
        assert_eq!(body["store"], false);
        drop(records);

        // The model read out of the compressed body reaches the audit row.
        wait_for_audits(&state, 1).await;
        let model: String = sqlx::query_scalar("SELECT model FROM api_calls")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(model, "gpt-5.4");
    }

    #[tokio::test]
    async fn codex_requests_keep_the_generic_header_policy() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let mut request = client_request(
            "/v1/responses",
            CODEX_CLI_USER_AGENT,
            Some(CODEX_ORIGINATOR),
            Body::from(r#"{"model":"gpt-5.4"}"#),
        );
        let headers = request.headers_mut();
        headers.insert("thread-id", HeaderValue::from_static("client-thread"));
        headers.insert(
            "x-codex-turn-metadata",
            HeaderValue::from_static(r#"{"installation_id":"provider-1"}"#),
        );
        let response = crate::router(state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.into_body().collect().await.unwrap();

        let headers = recorded_headers(&records).await;
        assert_eq!(headers["thread-id"], "client-thread");
        assert_eq!(
            headers["x-codex-turn-metadata"],
            r#"{"installation_id":"provider-1"}"#
        );
        assert_eq!(headers["session-id"], "session-secret");
        assert_eq!(headers[ORIGINATOR_HEADER], CODEX_ORIGINATOR);
    }

    #[tokio::test]
    async fn cross_originator_fallback_forwards_the_callers_session_id() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, false).await;
        seed_pi_fallback(&state).await;
        sqlx::query("UPDATE providers SET manual_disabled=1 WHERE id='provider-1'")
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
        let response = crate::router(state.clone())
            .oneshot(client_request(
                "/v1/responses",
                CODEX_CLI_USER_AGENT,
                Some(CODEX_ORIGINATOR),
                Body::from(r#"{"model":"gpt-5.4"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.into_body().collect().await.unwrap();

        // OpenAI-LB never fabricates a session id for the Pi provider it falls back to: the caller's
        // own id goes upstream, and the derived request id matches it so the two never disagree.
        let headers = recorded_headers(&records).await;
        assert_eq!(headers["session-id"], "session-secret");
        assert_eq!(headers["x-client-request-id"], "session-secret");
        assert_eq!(headers[header::USER_AGENT], "pi");
        assert_eq!(headers[ORIGINATOR_HEADER], "pi");
    }

    #[tokio::test]
    async fn image_generation_needs_no_session_id_and_mints_one_for_the_upstream() {
        let (upstream, records) = spawn_mock(StatusCode::OK).await;
        let state = crate::test_state_with_upstream("http://token.invalid", &upstream).await;
        seed_proxy(&state, true).await;
        let mut request = proxy_request(
            "/v1/images/generations",
            "application/json",
            Body::from(r#"{"model":"gpt-image-2","prompt":"diagram","n":1,"size":"1024x1024"}"#),
        );
        request.headers_mut().remove("session-id");
        let response = crate::router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let image: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(image["data"][0]["b64_json"], "aW1hZ2U=");

        let records = records.lock().await;
        assert_eq!(records.len(), 1);
        let session = records[0].headers["session-id"].to_str().unwrap();
        assert_eq!(Uuid::parse_str(session).unwrap().get_version_num(), 7);
    }
}
