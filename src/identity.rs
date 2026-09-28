//! Downstream client identity and upstream provider originator policy.
//!
//! Providers keep their OAuth/upstream identity. Routing prefers the matching client family;
//! only explicitly opted-in providers may back up other or unidentified clients. Conflicting
//! recognized identity signals are rejected even when fallback providers exist.

use axum::http::{HeaderMap, HeaderName, header};

use crate::AppError;

/// Originators a provider may declare. Values mirror the clients shipped for each family.
pub const PROVIDER_ORIGINATORS: &[&str] = &["codex_cli_rs", "pi", "opencode"];

/// Originator of the Pi Agent client family, whose requests are normalized to the header set that
/// client actually sends. See [`require_pi_session_id`].
pub const PI_ORIGINATOR: &str = "pi";

/// Paths where Pi Agent sends a Codex request body. Every other capability (transcriptions,
/// images, realtime) belongs to a different client family, so the Pi header policy never applies
/// to them.
pub const PI_RESPONSES_PATHS: &[&str] = &[
    "/v1/responses",
    "/v1/responses/compact",
    "/backend-api/codex/responses",
    "/backend-api/codex/responses/compact",
];

/// Earliest UUIDv7 timestamp accepted for a Pi session id (2020-01-01), matching the oldest
/// session Pi could still resume on a deployment that keeps session files.
const PI_SESSION_ID_FLOOR_MS: i64 = 1_577_836_800_000;

/// Slack allowed above the current time so a mildly skewed client clock does not reject a session
/// id Pi generated moments ago.
const PI_SESSION_ID_FUTURE_SLACK_MS: i64 = 24 * 60 * 60 * 1000;

/// Whether `path` is one of the endpoints carrying the Pi Agent request shape.
pub fn is_pi_responses_path(path: &str) -> bool {
    PI_RESPONSES_PATHS.contains(&path)
}

/// Whether the request claims the Pi identity through its own `originator` header.
fn claims_pi_originator(headers: &HeaderMap) -> bool {
    headers
        .get(ORIGINATOR_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case(PI_ORIGINATOR))
}

/// The `session-id` a request presents, trimmed, if it presents one at all.
pub fn session_id_header(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Validates a Pi Agent `session-id`: a lowercase UUIDv7 whose embedded millisecond timestamp is
/// plausible for a session this deployment can serve.
///
/// Pi derives the id from its own clock when it starts a session, and the value doubles as the
/// affinity key OpenAI-LB routes on. The upstream can read that timestamp, so a fabricated one is a
/// risk signal OpenAI-LB never emits: a request that does not carry a UUIDv7 of its own is rejected
/// rather than given a synthesized id.
pub fn valid_pi_session_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_digit() && !(b'a'..=b'f').contains(byte) {
                    return false;
                }
            }
        }
    }
    // Version and variant nibbles, per RFC 9562.
    if bytes[14] != b'7' || !matches!(bytes[19], b'8' | b'9' | b'a' | b'b') {
        return false;
    }
    // The leading 48 bits hold the millisecond timestamp; the hyphen at index 8 is not a digit.
    let digits = bytes[..13].iter().filter(|byte| **byte != b'-');
    let mut timestamp = 0i64;
    for byte in digits {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            _ => byte - b'a' + 10,
        };
        timestamp = timestamp * 16 + i64::from(digit);
    }
    let now = chrono::Utc::now().timestamp_millis();
    timestamp >= PI_SESSION_ID_FLOOR_MS && timestamp <= now + PI_SESSION_ID_FUTURE_SLACK_MS
}

/// Rejects a Pi Agent request that does not carry its own UUIDv7 `session-id`.
///
/// Pi mints one per session and sends it on every request, including the ones it labels as
/// one-off summarization calls, so a Pi-identified request without a UUIDv7 is not a request Pi
/// would have produced. OpenAI-LB never invents an id here: the timestamp inside a UUIDv7 is
/// readable upstream, and neither a fabricated one nor a caller id of another shape is worth
/// presenting as Pi traffic.
pub fn require_pi_session_id(headers: &HeaderMap, path: &str) -> Result<(), AppError> {
    if !is_pi_responses_path(path) || !claims_pi_originator(headers) {
        return Ok(());
    }
    match session_id_header(headers) {
        Some(value) if valid_pi_session_id(value) => Ok(()),
        Some(_) => Err(AppError::bad_request_with_reason(
            "session-id must be a UUIDv7 for Pi Agent requests",
            "downstream_session_id_invalid",
        )),
        None => Err(AppError::bad_request_with_reason(
            "session-id request header is required",
            "downstream_session_id_missing",
        )),
    }
}

/// Originator used by historical providers and by every CodeX harness client.
pub const CODEX_ORIGINATOR: &str = "codex_cli_rs";

pub const ORIGINATOR_HEADER: HeaderName = HeaderName::from_static("originator");

/// Human-readable list of supported originators for validation errors.
pub fn supported_originators() -> String {
    PROVIDER_ORIGINATORS.join(", ")
}

/// Validates an operator-supplied provider originator, defaulting to the CodeX CLI identity.
pub fn provider_originator(value: Option<&str>) -> Result<String, AppError> {
    let value = value.unwrap_or(CODEX_ORIGINATOR).trim();
    if PROVIDER_ORIGINATORS.contains(&value) {
        return Ok(value.to_owned());
    }
    Err(AppError::bad_request(format!(
        "originator must be one of {}",
        supported_originators()
    )))
}

/// Client family a downstream request is served as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DownstreamClient {
    /// Official CodeX harnesses: CLI, TUI, VS Code extension, desktop app.
    Codex,
    /// Pi coding agent.
    Pi,
    /// OpenCode.
    OpenCode,
    /// OpenAI-LB's own browser console, which is authenticated by an Auth Mini session.
    Console,
    /// No recognized client family; only opted-in fallback providers may serve this request.
    Unknown,
}

impl DownstreamClient {
    /// Preferred provider identity; unidentified requests have no exact provider pool.
    pub fn provider_originator(self) -> &'static str {
        match self {
            Self::Codex | Self::Console => CODEX_ORIGINATOR,
            Self::Pi => PI_ORIGINATOR,
            Self::OpenCode => "opencode",
            // Empty is not a valid provider identity, so this cannot form an exact pool.
            // It is a routing key only and is never sent upstream.
            Self::Unknown => "",
        }
    }

    /// Stable label used in operator-facing errors and audit reasons.
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
            Self::Console => "console",
            Self::Unknown => "unknown",
        }
    }
}

/// User agent prefixes accepted for each client family.
const CODEX_USER_AGENT_PREFIXES: &[&str] = &[
    "codex_cli_rs/",
    "codex_cli_rs ",
    "codex-tui/",
    "codex_tui/",
    "codex_vscode/",
    "codex-vscode/",
    "codex_exec/",
    "codex-desktop",
    "codex desktop",
    "codex/",
];
const PI_USER_AGENT_PREFIXES: &[&str] = &["pi/", "pi "];
const OPENCODE_USER_AGENT_PREFIXES: &[&str] = &["opencode/", "opencode "];
const PI_USER_AGENTS: &[&str] = &["pi"];
const OPENCODE_USER_AGENTS: &[&str] = &["opencode"];

/// Classifies a downstream request from its `User-Agent`.
fn client_from_user_agent(user_agent: &str) -> Option<DownstreamClient> {
    let user_agent = user_agent.trim();
    if user_agent.is_empty() {
        return None;
    }
    let lower = user_agent.to_ascii_lowercase();
    if CODEX_USER_AGENT_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return Some(DownstreamClient::Codex);
    }
    if PI_USER_AGENTS.contains(&lower.as_str())
        || PI_USER_AGENT_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
    {
        return Some(DownstreamClient::Pi);
    }
    if OPENCODE_USER_AGENTS.contains(&lower.as_str())
        || OPENCODE_USER_AGENT_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
    {
        return Some(DownstreamClient::OpenCode);
    }
    None
}

/// Whether a downstream `originator` value belongs to the CodeX harness family.
fn is_codex_originator(value: &str) -> bool {
    value.to_ascii_lowercase().starts_with("codex")
}

/// Identifies the client family of a Consumer-authenticated request.
///
/// `originator` is the authoritative signal when it is present. `User-Agent` is the fallback for
/// clients such as Pi versions that do not send an originator header. When both signals identify a
/// client, they must agree; otherwise the request is rejected instead of being routed to an
/// upstream subscription that was issued for somebody else.
pub fn identify_client(headers: &HeaderMap) -> Result<DownstreamClient, AppError> {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let originator = headers
        .get(ORIGINATOR_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let from_originator = originator.as_deref().and_then(client_from_originator);
    if originator.is_some() && from_originator.is_none() {
        return Ok(DownstreamClient::Unknown);
    }
    let from_user_agent = client_from_user_agent(user_agent);

    match (from_originator, from_user_agent) {
        (Some(originator_client), Some(user_agent_client))
            if originator_client != user_agent_client =>
        {
            Err(AppError::unavailable_with_reason(
                "no available provider for this downstream client identity",
                "downstream_identity_mismatch",
            ))
        }
        (Some(originator_client), _) => Ok(originator_client),
        (None, Some(user_agent_client)) => Ok(user_agent_client),
        (None, None) => Ok(DownstreamClient::Unknown),
    }
}

/// Versionless defaults for cross-family traffic, where forwarding the caller's UA would
/// contradict the selected provider identity. Operators can override these in settings.
pub fn default_upstream_user_agent(originator: &str) -> &'static str {
    match originator {
        "pi" => "pi",
        "opencode" => "opencode",
        _ => "codex_cli_rs (OpenAI-LB)",
    }
}

/// Maps an upstream-style originator header to the client family it represents.
fn client_from_originator(value: &str) -> Option<DownstreamClient> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("pi") {
        return Some(DownstreamClient::Pi);
    }
    if value.eq_ignore_ascii_case("opencode") {
        return Some(DownstreamClient::OpenCode);
    }
    if is_codex_originator(value) {
        return Some(DownstreamClient::Codex);
    }
    None
}

/// Rejection used when a request family cannot use a capability reserved for another family.
pub fn capability_requires_codex_client() -> AppError {
    AppError::unavailable_with_reason(
        "no available provider for this downstream client identity",
        "codex_client_required",
    )
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(user_agent: &str, originator: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            axum::http::HeaderValue::from_str(user_agent).unwrap(),
        );
        if let Some(originator) = originator {
            headers.insert(
                ORIGINATOR_HEADER,
                axum::http::HeaderValue::from_str(originator).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn accepts_each_supported_client_family() {
        for (user_agent, originator, expected) in [
            (
                "codex_cli_rs/0.51.0 (macos 15.0; arm64)",
                "codex_cli_rs",
                DownstreamClient::Codex,
            ),
            (
                "codex_vscode/1.2.3 (linux; x86_64)",
                "codex_vscode",
                DownstreamClient::Codex,
            ),
            (
                "Codex Desktop/26.519.81530 (macos; aarch64)",
                "Codex Desktop",
                DownstreamClient::Codex,
            ),
            ("pi (darwin 24.5.0; arm64)", "pi", DownstreamClient::Pi),
            (
                "opencode/1.18.31 (linux; x86_64)",
                "opencode",
                DownstreamClient::OpenCode,
            ),
        ] {
            assert_eq!(
                identify_client(&headers(user_agent, Some(originator))).unwrap(),
                expected,
                "{user_agent} should classify as {expected:?}"
            );
        }
    }

    #[test]
    fn originator_is_preferred_over_an_unknown_user_agent() {
        assert_eq!(
            identify_client(&headers("curl/8.7.1", Some("pi"))).unwrap(),
            DownstreamClient::Pi
        );
        assert_eq!(
            identify_client(&headers("pi (darwin 24.5.0; arm64)", None)).unwrap(),
            DownstreamClient::Pi
        );
        assert_eq!(
            identify_client(&headers("", None)).unwrap(),
            DownstreamClient::Unknown
        );
    }

    #[test]
    fn rejects_identity_that_contradicts_the_user_agent() {
        let error = identify_client(&headers("codex_cli_rs/0.51.0", Some("pi"))).unwrap_err();
        assert_eq!(error.reason(), Some("downstream_identity_mismatch"));
        assert_eq!(
            identify_client(&headers("pi (darwin 24.5.0; arm64)", Some("unknown"))).unwrap(),
            DownstreamClient::Unknown,
        );
    }

    #[test]
    fn maps_client_families_to_provider_originators() {
        assert_eq!(
            DownstreamClient::Codex.provider_originator(),
            CODEX_ORIGINATOR
        );
        assert_eq!(
            DownstreamClient::Console.provider_originator(),
            CODEX_ORIGINATOR
        );
        assert_eq!(DownstreamClient::Pi.provider_originator(), "pi");
        assert_eq!(DownstreamClient::OpenCode.provider_originator(), "opencode");
    }

    #[test]
    fn validates_provider_originators() {
        assert_eq!(provider_originator(None).unwrap(), CODEX_ORIGINATOR);
        assert_eq!(provider_originator(Some(" pi ")).unwrap(), "pi");
        assert_eq!(
            provider_originator(Some("opencode")).unwrap(),
            "opencode".to_owned()
        );
        assert!(provider_originator(Some("unsloth_studio")).is_err());
    }
    #[test]
    fn fallback_user_agents_identify_as_the_actual_provider() {
        for originator in PROVIDER_ORIGINATORS {
            assert_eq!(
                client_from_user_agent(default_upstream_user_agent(originator))
                    .unwrap()
                    .provider_originator(),
                *originator,
            );
        }
    }

    /// A session id captured from a real Pi Agent run.
    const PI_SESSION_ID: &str = "01a0bd2a-0c12-7123-a3df-36f9d077060e";

    fn pi_headers(session_id: Option<&str>) -> HeaderMap {
        let mut headers = headers("pi (darwin 24.5.0; arm64)", Some(PI_ORIGINATOR));
        if let Some(session_id) = session_id {
            headers.insert("session-id", HeaderValue::from_str(session_id).unwrap());
        }
        headers
    }

    #[test]
    fn accepts_pi_session_ids_and_rejects_every_other_shape() {
        assert!(valid_pi_session_id(PI_SESSION_ID));
        for rejected in [
            "",
            "session-secret",
            "01a0bd2a0c127123a3df36f9d077060e",
            // Not version 7.
            "01a0bd2a-0c12-6123-a3df-36f9d077060e",
            // Reserved variant.
            "01a0bd2a-0c12-7123-c3df-36f9d077060e",
            // Uppercase hex.
            "01A0BD2A-0C12-7123-A3DF-36F9D077060E",
            // Before the accepted floor.
            "00000000-0000-7000-8000-000000000000",
            // A century in the future.
            "ffffffff-ffff-7fff-8fff-ffffffffffff",
        ] {
            assert!(
                !valid_pi_session_id(rejected),
                "{rejected} must be rejected"
            );
        }
    }

    #[test]
    fn requires_a_uuid_v7_session_id_only_from_pi_requests() {
        // Pi's own id passes through untouched, without OpenAI-LB inventing anything.
        let valid = pi_headers(Some(PI_SESSION_ID));
        require_pi_session_id(&valid, "/v1/responses").unwrap();
        assert_eq!(session_id_header(&valid), Some(PI_SESSION_ID));

        // A custom `--session-id`, a legacy value, and a UUID of another version are all rejected
        // rather than replaced by a synthesized id: the upstream can read the timestamp inside a
        // UUIDv7, and a fabricated one would be a risk signal.
        for rejected in [
            "my-task-1",
            "session-secret",
            "01a0bd2a-0c12-6123-a3df-36f9d077060e",
        ] {
            let error =
                require_pi_session_id(&pi_headers(Some(rejected)), "/v1/responses").unwrap_err();
            assert_eq!(error.status(), axum::http::StatusCode::BAD_REQUEST);
            assert_eq!(error.reason(), Some("downstream_session_id_invalid"));
        }

        // Pi's summarization calls omit the header, and there is nothing to derive it from.
        for missing in [None, Some("   ")] {
            let error = require_pi_session_id(&pi_headers(missing), "/v1/responses").unwrap_err();
            assert_eq!(error.reason(), Some("downstream_session_id_missing"));
        }

        // Other families and other capabilities keep whatever the caller sent.
        let mut codex = headers(
            "codex_cli_rs/0.51.0 (macos 15.0; arm64)",
            Some(CODEX_ORIGINATOR),
        );
        codex.insert("session-id", HeaderValue::from_static("session-secret"));
        require_pi_session_id(&codex, "/v1/responses").unwrap();
        assert_eq!(session_id_header(&codex), Some("session-secret"));

        require_pi_session_id(
            &pi_headers(Some("session-secret")),
            "/v1/audio/transcriptions",
        )
        .unwrap();
        require_pi_session_id(&pi_headers(None), "/v1/images/generations").unwrap();
    }
}
