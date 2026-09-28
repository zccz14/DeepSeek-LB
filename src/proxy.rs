use std::{convert::Infallible, net::SocketAddr, time::Instant};

use axum::{
    body::{Body, Bytes},
    extract::{ConnectInfo, OriginalUri, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version, header},
    response::Response,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

use crate::{
    AppError, AppState,
    audit::{ARCHIVE_BODY_LIMIT, AuditEvent, AuditReservation},
    auth::{ApiIdentity, api_identity},
    balancer::{Downstream, Lease, affinity_hash, track_response},
    midas,
    pricing::{self, Usage},
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

const PROXY_ONLY_HEADERS: &[&str] = &["x-lb-affinity-key"];

struct CallContext {
    model: Option<String>,
    stream: bool,
}

struct RequestAuditContext {
    request_id: String,
    thread_id: Option<String>,
    method: Method,
    client_ip: String,
}

/// Which DeepSeek endpoint a request is bound for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Endpoint {
    ChatCompletions,
    Responses,
}

impl Endpoint {
    fn for_path(path: &str) -> Option<Self> {
        match path {
            "/v1/chat/completions" | "/chat/completions" => Some(Self::ChatCompletions),
            "/v1/responses" | "/responses" => Some(Self::Responses),
            _ => None,
        }
    }

    /// Stored in the audit row so bare and `/v1`-prefixed routes share one identity.
    fn audit_path(self) -> &'static str {
        match self {
            Self::ChatCompletions => "/v1/chat/completions",
            Self::Responses => "/v1/responses",
        }
    }

    /// Appended to the configured upstream base URL.
    fn upstream_suffix(self) -> &'static str {
        match self {
            Self::ChatCompletions => "/chat/completions",
            Self::Responses => "/responses",
        }
    }
}

struct ProxyRequest {
    body: Bytes,
    model: Option<String>,
    stream: bool,
}

struct AuditStart<'a> {
    audit_id: &'a str,
    thread_id: Option<&'a str>,
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
            consumer_id: identity.consumer_id.clone(),
            user_id: identity.user_id.clone(),
            request_archive: archive_budget.is_some(),
            provider_id: None,
            affinity_hash: None,
            affinity_source: None,
            method: start.method.as_str().to_owned(),
            path: start.path.to_owned(),
            model: None,
            reasoning_effort: None,
            peak: false,
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
        }
    }

    fn set_requested_model(&mut self, model: Option<&str>) {
        if let (Some(model), Some(event)) = (model, self.event.as_mut()) {
            event.model = Some(model.to_owned());
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

    fn set_request(&mut self, headers: &HeaderMap, body: &[u8], truncated: bool) {
        if let Some(event) = &mut self.event {
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
        usage_from_value(&value)
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
    event.model = model.map(str::to_owned).or_else(|| event.model.take());
    event.latency_ms = started.elapsed().as_millis().max(1) as i64;
    event.input_tokens = usage.input_tokens;
    event.output_tokens = usage.output_tokens;
    event.cached_tokens = usage.cached_tokens;
    event.peak = pricing::is_peak(event.created_at);
    event.official_cost_usd_nanos = model
        .and_then(|model| pricing::official_cost_usd_nanos(model, usage, event.peak))
        .unwrap_or_default();
    event.actual_cost_usd_nanos = pricing::apply_price_multiplier(
        event.official_cost_usd_nanos,
        event.price_multiplier_nanos,
    );
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
        method,
        client_ip: peer.ip().to_string(),
    };
    dispatch(state, identity, audit, uri.path(), &headers, &body).await
}

pub async fn handle_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    api_identity(&state, &headers).await?;
    let config = state.config.load();
    models_response(&config.available_model_ids)
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

async fn dispatch(
    state: AppState,
    identity: ApiIdentity,
    audit_context: RequestAuditContext,
    path: &str,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, AppError> {
    let endpoint =
        Endpoint::for_path(path).ok_or_else(|| AppError::not_found("API route not found"))?;
    let parsed = serde_json::from_slice::<Value>(body).ok();
    let affinity_key = affinity_key(headers);
    let affinity = affinity_key
        .as_ref()
        .map(|key| format!("{}:{}", identity.consumer_id, key.value));
    // INVARIANT: The audit row starts before validation so rejected requests stay visible.
    let mut audit = begin_request_audit(&state, &identity, &audit_context, endpoint, headers, body);
    audit.set_affinity(
        affinity.as_deref(),
        affinity_key.as_ref().map(|key| key.source),
    );
    let result = async {
        let request = prepare_request(endpoint, parsed, &state)?;
        ensure_request_quota(&state, &identity.user_id, identity.allow_debt).await?;
        let downstream = Downstream::new(&identity.user_id);
        let (lease, upstream) = dispatch_to_provider(
            &state,
            endpoint,
            headers,
            &request,
            affinity.as_deref(),
            &mut audit,
            downstream,
        )
        .await?;
        let context = CallContext {
            model: request.model.clone(),
            stream: request.stream,
        };
        relay_response(&state, context, lease, upstream, &mut audit).await
    }
    .await;
    finish_audit_error(&mut audit, result)
}

fn prepare_request(
    endpoint: Endpoint,
    parsed: Option<Value>,
    state: &AppState,
) -> Result<ProxyRequest, AppError> {
    let value = parsed.ok_or_else(|| AppError::bad_request("JSON request body required"))?;
    if !value.is_object() {
        return Err(AppError::bad_request("JSON object required"));
    }
    let model = value
        .get("model")
        .map(|model| {
            model
                .as_str()
                .filter(|model| !model.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| AppError::bad_request("model must be a non-empty string"))
        })
        .transpose()?
        .ok_or_else(|| AppError::bad_request("model is required"))?;
    ensure_model_is_available(state, &model)?;
    let stream = value
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or_default();
    let _ = endpoint;
    Ok(ProxyRequest {
        body: serde_json::to_vec(&value)?.into(),
        model: Some(model),
        stream,
    })
}

fn begin_request_audit(
    state: &AppState,
    identity: &ApiIdentity,
    context: &RequestAuditContext,
    endpoint: Endpoint,
    headers: &HeaderMap,
    body: &[u8],
) -> AuditTracker {
    let mut audit = AuditTracker::begin(
        state,
        identity,
        AuditStart {
            audit_id: &context.request_id,
            thread_id: context.thread_id.as_deref(),
            method: &context.method,
            path: endpoint.audit_path(),
            client_ip: &context.client_ip,
            response_body_limit: ARCHIVE_BODY_LIMIT,
        },
    );
    audit.set_request(headers, body, false);
    audit.set_compression_headers(headers, None);
    audit.set_request_size(body.len() as i64);
    let request_json = serde_json::from_slice::<Value>(body).ok();
    audit.set_reasoning_effort(request_json.as_ref().and_then(reasoning_effort).as_deref());
    audit.set_requested_model(
        request_json
            .as_ref()
            .and_then(|value| value.get("model"))
            .and_then(Value::as_str),
    );
    audit
}

fn finish_audit_error<T>(
    audit: &mut AuditTracker,
    result: Result<T, AppError>,
) -> Result<T, AppError> {
    result.map_err(|error| {
        let reason = error.reason().map(str::to_owned);
        if let (Some(reason), Some(event)) = (reason, audit.event.as_mut()) {
            event.error_code = Some(reason);
        }
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

async fn dispatch_to_provider(
    state: &AppState,
    endpoint: Endpoint,
    headers: &HeaderMap,
    request: &ProxyRequest,
    affinity: Option<&str>,
    audit: &mut AuditTracker,
    downstream: Downstream<'_>,
) -> Result<(Lease, UpstreamResponse), AppError> {
    let mut first = state
        .balancer
        .select(state, affinity, None, downstream)
        .await?;
    audit.set_provider(&first);
    let first_id = first.provider.id.clone();
    let response = send_upstream(
        state,
        &first,
        endpoint,
        headers,
        request.body.clone().into(),
        audit,
    )
    .await?;
    let response = track_upstream(state, &first_id, UpstreamResponse::Live(response)).await?;
    if !retryable(response.status()) {
        return Ok((first, response));
    }
    let response = finish_retry_attempt(&mut first, response).await?;
    let second = state
        .balancer
        .select(state, affinity, Some(&first_id), downstream)
        .await
        .ok();
    let Some(second) = second else {
        return Ok((first, response));
    };
    drop(response);
    drop(first);
    audit.set_provider(&second);
    let second_response = send_upstream(
        state,
        &second,
        endpoint,
        headers,
        request.body.clone().into(),
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

struct AffinityKey {
    source: &'static str,
    value: String,
}

fn affinity_key(headers: &HeaderMap) -> Option<AffinityKey> {
    [
        ("x-lb-affinity-key", "x-lb-affinity-key"),
        ("thread-id", "thread-id"),
        ("x-deepseek-session-id", "x-deepseek-session-id"),
        ("session-id", "session-id"),
    ]
    .iter()
    .find_map(|(name, source)| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| AffinityKey {
                source,
                value: value.to_owned(),
            })
    })
}

async fn send_upstream(
    state: &AppState,
    lease: &Lease,
    endpoint: Endpoint,
    inbound: &HeaderMap,
    body: reqwest::Body,
    audit: &mut AuditTracker,
) -> Result<reqwest::Response, AppError> {
    let mut request = state.client.post(format!(
        "{}{}",
        state.config.load().upstream_base,
        endpoint.upstream_suffix()
    ));
    for (name, value) in inbound {
        if should_forward_request_header(name) {
            request = request.header(name, value);
        }
    }
    audit.set_upstream_accept_encoding(inbound);
    request = request
        .header(header::AUTHORIZATION, format!("Bearer {}", lease.api_key))
        .header(header::CONTENT_TYPE, "application/json");
    let request = request.body(body).build()?;
    audit.set_upstream_request_headers(request.headers());
    Ok(state.client.execute(request).await?)
}

fn header_value(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn should_forward_request_header(name: &HeaderName) -> bool {
    let lower = name.as_str().to_ascii_lowercase();
    if lower == "authorization"
        || HOP_HEADERS.contains(&lower.as_str())
        || PROXY_ONLY_HEADERS.contains(&lower.as_str())
    {
        return false;
    }
    matches!(lower.as_str(), "accept" | "accept-encoding" | "user-agent")
}

fn retryable(status: StatusCode) -> bool {
    matches!(status.as_u16(), 401 | 402 | 403 | 429) || status.is_server_error()
}

async fn track_upstream(
    state: &AppState,
    provider_id: &str,
    upstream: UpstreamResponse,
) -> Result<UpstreamResponse, AppError> {
    let status = upstream.status();
    if !matches!(status.as_u16(), 401 | 402 | 403 | 429) {
        track_response(state, provider_id, status, &[]).await?;
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
    let _ = state;
    audit.mark_first_byte();
    let status = upstream.status();
    audit.set_upstream_http_version(upstream.version());
    audit.set_compression_headers(&HeaderMap::new(), Some(upstream.headers()));
    audit.set_response_headers(upstream.headers());
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

fn filtered_response_headers(headers: &HeaderMap) -> Vec<(HeaderName, HeaderValue)> {
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

fn usage_from_value(value: &Value) -> Usage {
    let usage = value
        .get("usage")
        .or_else(|| value.pointer("/response/usage"));
    let Some(usage) = usage else {
        return Usage::default();
    };
    let input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage.get("prompt_tokens").and_then(Value::as_i64))
        .unwrap_or_default();
    let output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage.get("completion_tokens").and_then(Value::as_i64))
        .unwrap_or_default();
    let cached_tokens = usage
        .get("prompt_cache_hit_tokens")
        .and_then(Value::as_i64)
        .or_else(|| {
            usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(Value::as_i64)
        })
        .or_else(|| {
            usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_i64)
        })
        .unwrap_or_default();
    Usage {
        input_tokens,
        output_tokens,
        cached_tokens,
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
}

impl StreamingPreview {
    fn capture(&mut self, bytes: &[u8]) {
        let remaining = ARCHIVE_BODY_LIMIT.saturating_sub(self.body.len());
        let copied = remaining.min(bytes.len());
        self.body.extend_from_slice(&bytes[..copied]);
        self.truncated |= copied < bytes.len();
    }
}

fn thread_id(headers: &HeaderMap) -> Option<String> {
    ["x-deepseek-session-id", "thread-id", "session-id"]
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

struct SseObservation {
    decoder: sse_core::SseDecoder,
    usage: Usage,
    failure: Option<ResponseFailure>,
}

impl Default for SseObservation {
    fn default() -> Self {
        Self {
            decoder: sse_core::SseDecoder::with_limit(
                std::num::NonZeroUsize::new(SSE_EVENT_LIMIT).expect("SSE event limit is positive"),
            ),
            usage: Usage::default(),
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
        &json!({"object":"list","data":models.iter().map(|id| json!({"id":id,"object":"model","owned_by":"deepseek"})).collect::<Vec<_>>() }),
    )?;
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
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use axum::{
        Json, Router,
        extract::{OriginalUri, State},
        http::{HeaderMap, Request, StatusCode, header},
        response::{IntoResponse, Response},
        routing::{get, post},
    };
    use http_body_util::BodyExt;
    use sqlx::Row;
    use tower::ServiceExt;

    use super::*;
    use crate::{AppState, crypto::consumer_secret_hash, payments};

    const CONSUMER_SECRET: &str = "sk-consumer-test-secret";
    const FAILING_PROVIDER_KEY: &str = "sk-provider-failing";

    #[derive(Clone)]
    struct RecordedRequest {
        path: String,
        headers: HeaderMap,
        body: Bytes,
    }

    #[derive(Clone)]
    struct MockUpstream {
        records: Arc<Mutex<Vec<RecordedRequest>>>,
    }

    async fn mock_upstream(
        State(mock): State<MockUpstream>,
        OriginalUri(uri): OriginalUri,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        mock.records.lock().unwrap().push(RecordedRequest {
            path: uri.path().to_owned(),
            headers: headers.clone(),
            body: body.clone(),
        });
        let authorization = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        if authorization == format!("Bearer {FAILING_PROVIDER_KEY}") {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"error":{"message":"upstream exploded","type":"server_error"}}"#,
                ))
                .unwrap();
        }
        let payload = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
        let stream = payload
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or_default();
        match uri.path() {
            "/user/balance" => Json(json!({
                "is_available": true,
                "balance_infos": [{"currency":"USD","total_balance":"12.34","granted_balance":"0.00","topped_up_balance":"12.34"}]
            }))
            .into_response(),
            "/chat/completions" if stream => {
                let events = concat!(
                    "data: {\"id\":\"chatcmpl-stream\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"po\"}}]}\n\n",
                    "data: {\"id\":\"chatcmpl-stream\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ng\"}}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":5,\"prompt_cache_hit_tokens\":3,\"prompt_cache_miss_tokens\":8}}\n\n",
                    "data: [DONE]\n\n",
                );
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .body(Body::from(events))
                    .unwrap()
            }
            "/responses" if stream => {
                let events = concat!(
                    "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-1\"}}\n\n",
                    "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"model\":\"deepseek-flash\",\"usage\":{\"input_tokens\":7,\"input_tokens_details\":{\"cached_tokens\":2},\"output_tokens\":4}}}\n\n",
                );
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .body(Body::from(events))
                    .unwrap()
            }
            "/chat/completions" => Json(json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "model": "deepseek-flash",
                "choices": [{"index":0,"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}],
                "usage": {"prompt_tokens":11,"completion_tokens":5,"prompt_cache_hit_tokens":3,"prompt_cache_miss_tokens":8}
            }))
            .into_response(),
            _ => Json(json!({
                "id": "resp-test",
                "object": "response",
                "model": "deepseek-flash",
                "output": [{"type":"message","role":"assistant","content":[{"type":"output_text","text":"pong"}]}],
                "usage": {"input_tokens":7,"input_tokens_details":{"cached_tokens":2},"output_tokens":4}
            }))
            .into_response(),
        }
    }

    async fn spawn_mock() -> (String, Arc<Mutex<Vec<RecordedRequest>>>) {
        let records = Arc::new(Mutex::new(Vec::new()));
        let mock = MockUpstream {
            records: records.clone(),
        };
        let app = Router::new()
            .route("/chat/completions", post(mock_upstream))
            .route("/responses", post(mock_upstream))
            .route("/user/balance", get(mock_upstream))
            .with_state(mock);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}"), records)
    }

    async fn seed_consumer(state: &AppState, credit_usd_nanos: i64) {
        sqlx::query(
            "INSERT INTO users(id,role,created_at,provided_usd_nanos) VALUES('user','user',0,?)",
        )
        .bind(credit_usd_nanos)
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('consumer','user','test','sk-consumer',?,0)")
            .bind(consumer_secret_hash(CONSUMER_SECRET))
            .execute(&state.db)
            .await
            .unwrap();
    }

    async fn seed_provider(state: &AppState, id: &str, api_key: &str) {
        sqlx::query("INSERT INTO providers(id,name,api_key,status,created_at,updated_at,owner_id,visibility) VALUES(?,?,?,'active',0,0,'user','public')")
            .bind(id)
            .bind(id)
            .bind(api_key)
            .execute(&state.db)
            .await
            .unwrap();
        state.balancer.reload_providers(&state.db).await.unwrap();
    }

    fn proxy_request(path: &str, body: Value) -> Request<Body> {
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {CONSUMER_SECRET}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                40_000,
            ))));
        request
    }

    async fn wait_for_audits(state: &AppState, expected: i64) -> Vec<sqlx::sqlite::SqliteRow> {
        for _ in 0..200 {
            let rows = sqlx::query("SELECT * FROM api_calls ORDER BY created_at,id")
                .fetch_all(&state.db)
                .await
                .unwrap();
            if rows.len() as i64 >= expected {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("audit rows were not written in time");
    }

    #[tokio::test]
    async fn chat_completions_relays_the_response_and_audits_deepseek_usage() {
        let (upstream, records) = spawn_mock().await;
        let state = crate::test_state(&upstream).await;
        seed_consumer(&state, payments::USD_NANOS).await;
        seed_provider(&state, "provider-1", "sk-provider-ok").await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/chat/completions",
                json!({"model":"deepseek-flash","messages":[{"role":"user","content":"ping"}]}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(
            std::str::from_utf8(&body).unwrap().contains("pong"),
            "upstream response body is relayed"
        );

        let recorded = records.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].path, "/chat/completions");
        assert_eq!(
            recorded[0]
                .headers
                .get(header::AUTHORIZATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer sk-provider-ok"
        );
        assert!(
            !String::from_utf8_lossy(&recorded[0].body).contains(CONSUMER_SECRET),
            "the consumer credential never reaches DeepSeek"
        );

        let audits = wait_for_audits(&state, 1).await;
        let audit = &audits[0];
        assert_eq!(audit.get::<String, _>("path"), "/v1/chat/completions");
        assert_eq!(audit.get::<String, _>("model"), "deepseek-flash");
        assert_eq!(audit.get::<String, _>("provider_id"), "provider-1");
        assert_eq!(audit.get::<i64, _>("status"), 200);
        assert_eq!(audit.get::<i64, _>("input_tokens"), 11);
        assert_eq!(audit.get::<i64, _>("output_tokens"), 5);
        assert_eq!(audit.get::<i64, _>("cached_tokens"), 3);
        let created_at = audit.get::<i64, _>("created_at");
        assert_eq!(
            audit.get::<i64, _>("peak") != 0,
            pricing::is_peak(created_at),
            "the audit row records the DeepSeek tariff of the request"
        );
        assert!(
            audit.get::<i64, _>("official_cost_usd_nanos") > 0,
            "usage is priced with the DeepSeek price table"
        );
        assert_eq!(
            audit.get::<i64, _>("actual_cost_usd_nanos"),
            audit.get::<i64, _>("official_cost_usd_nanos")
        );
    }

    #[tokio::test]
    async fn responses_streams_event_stream_data_and_audits_the_completed_usage() {
        let (upstream, _) = spawn_mock().await;
        let state = crate::test_state(&upstream).await;
        seed_consumer(&state, payments::USD_NANOS).await;
        seed_provider(&state, "provider-1", "sk-provider-ok").await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/responses",
                json!({"model":"deepseek-v4-pro","stream":true,"input":"ping"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("response.completed"));
        assert!(text.contains("data:"));

        let audits = wait_for_audits(&state, 1).await;
        let audit = &audits[0];
        assert_eq!(audit.get::<String, _>("path"), "/v1/responses");
        assert_eq!(audit.get::<String, _>("model"), "deepseek-v4-pro");
        assert_eq!(audit.get::<i64, _>("input_tokens"), 7);
        assert_eq!(audit.get::<i64, _>("output_tokens"), 4);
        assert_eq!(audit.get::<i64, _>("cached_tokens"), 2);
        assert!(
            audit
                .get::<Option<i64>, _>("first_byte_latency_ms")
                .is_some()
        );
    }

    #[tokio::test]
    async fn a_second_provider_absorbs_upstream_failures() {
        let (upstream, records) = spawn_mock().await;
        let state = crate::test_state(&upstream).await;
        seed_consumer(&state, payments::USD_NANOS).await;
        seed_provider(&state, "provider-failing", FAILING_PROVIDER_KEY).await;
        seed_provider(&state, "provider-healthy", "sk-provider-ok").await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/chat/completions",
                json!({"model":"deepseek-flash","messages":[{"role":"user","content":"ping"}]}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&body).unwrap().contains("pong"));

        let recorded = records.lock().unwrap().clone();
        assert_eq!(recorded.len(), 2, "the request is retried once");
        let keys = recorded
            .iter()
            .map(|record| {
                record
                    .headers
                    .get(header::AUTHORIZATION)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert!(keys.contains(&format!("Bearer {FAILING_PROVIDER_KEY}")));
        assert!(keys.contains(&"Bearer sk-provider-ok".to_owned()));
        assert_ne!(keys[0], keys[1], "the retry uses a different provider");

        let audits = wait_for_audits(&state, 1).await;
        assert_eq!(
            audits[0].get::<String, _>("provider_id"),
            "provider-healthy"
        );
        assert_eq!(audits[0].get::<i64, _>("status"), 200);
    }

    #[tokio::test]
    async fn models_outside_the_allowlist_are_rejected_before_the_upstream_call() {
        let (upstream, records) = spawn_mock().await;
        let state = crate::test_state(&upstream).await;
        seed_consumer(&state, payments::USD_NANOS).await;
        seed_provider(&state, "provider-1", "sk-provider-ok").await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/chat/completions",
                json!({"model":"deepseek-v3-legacy","messages":[]}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("deepseek-v3-legacy"));
        assert!(records.lock().unwrap().is_empty());

        let audits = wait_for_audits(&state, 1).await;
        assert_eq!(audits[0].get::<i64, _>("status"), 400);
        assert_eq!(audits[0].get::<Option<String>, _>("provider_id"), None);
    }

    #[tokio::test]
    async fn requests_without_credit_are_rejected_with_402() {
        let (upstream, records) = spawn_mock().await;
        let state = crate::test_state(&upstream).await;
        seed_consumer(&state, 0).await;
        seed_provider(&state, "provider-1", "sk-provider-ok").await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/chat/completions",
                json!({"model":"deepseek-flash","messages":[]}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
        assert!(records.lock().unwrap().is_empty());
        let audits = wait_for_audits(&state, 1).await;
        assert_eq!(audits[0].get::<i64, _>("status"), 402);
    }

    #[tokio::test]
    async fn an_empty_provider_pool_reports_unavailable() {
        let (upstream, _) = spawn_mock().await;
        let state = crate::test_state(&upstream).await;
        seed_consumer(&state, payments::USD_NANOS).await;

        let response = crate::router(state.clone())
            .oneshot(proxy_request(
                "/v1/chat/completions",
                json!({"model":"deepseek-flash","messages":[]}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let audits = wait_for_audits(&state, 1).await;
        assert_eq!(audits[0].get::<i64, _>("status"), 503);
        assert_eq!(
            audits[0].get::<Option<String>, _>("error_code").as_deref(),
            Some("provider_pool_empty")
        );
    }

    #[tokio::test]
    async fn the_models_endpoint_lists_the_deepseek_family() {
        let (upstream, _) = spawn_mock().await;
        let state = crate::test_state(&upstream).await;
        seed_consumer(&state, payments::USD_NANOS).await;

        let mut request = Request::builder()
            .method("GET")
            .uri("/v1/models")
            .header(header::AUTHORIZATION, format!("Bearer {CONSUMER_SECRET}"))
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                40_000,
            ))));
        let response = crate::router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        let ids = value["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["deepseek-flash", "deepseek-v4-pro"]);
        assert_eq!(value["data"][0]["owned_by"], "deepseek");
    }
}
