//! DeepSeek native web search.
//!
//! DeepSeek publishes no dedicated search endpoint, so one search is one
//! Anthropic-compatible `/messages` turn carrying the native
//! `web_search_20250305` server tool: DeepSeek searches server-side and bills
//! the turn as ordinary tokens. The request shape and the `sources[]`
//! normalization follow `@deepseek-ai/dsh-web-search-deepseek`, the DeepSeek
//! Harness provider written for exactly this protocol.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use crate::{AppError, pricing::Usage};

/// Anthropic-compatible Messages path appended to the configured upstream base.
pub const MESSAGES_PATH: &str = "/anthropic/v1/messages";

/// `anthropic-version` header value. DeepSeek ignores the header value itself,
/// but the compatible protocol requires it on every Messages request.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Native server tool that performs the search inside the model turn.
const WEB_SEARCH_TOOL: &str = "web_search_20250305";

/// Generated-token cap of the auxiliary search turn.
const MAX_TOKENS: i64 = 4096;

/// Server-side searches allowed in one turn when the caller asks for none.
pub const DEFAULT_MAX_USES: i64 = 5;

/// Largest accepted `max_uses`.
pub const MAX_MAX_USES: i64 = 5;

/// Source bound applied when the caller sends none, matching the DeepSeek
/// Harness `web_search` consumer default.
pub const DEFAULT_MAX_RESULTS: usize = 8;

/// Largest accepted `max_results`.
pub const MAX_MAX_RESULTS: usize = 50;

#[derive(Debug)]
/// One validated `/v1/web-search` request.
pub struct SearchRequest {
    pub query: String,
    pub options: SearchOptions,
}

/// Caller-selectable search parameters, already validated against the native
/// `web_search` server tool.
#[derive(Debug)]
pub struct SearchOptions {
    /// Sources returned to the caller. DeepSeek exposes no result-count knob,
    /// so this bound is enforced here on the way back.
    pub max_results: usize,
    /// Server-side searches the turn may run.
    pub max_uses: i64,
    pub domains: DomainFilter,
    /// The tool's `approximate` location object, already normalized.
    pub user_location: Option<Value>,
}

/// Domain filter of one search. The server tool accepts either an allow list or
/// a block list, never both.
#[derive(Debug)]
pub enum DomainFilter {
    All,
    Allowed(Vec<String>),
    Blocked(Vec<String>),
}

/// Read one `/v1/web-search` body. Every optional parameter is rejected rather
/// than ignored when it is malformed, so a misspelled search control cannot
/// silently change what DeepSeek searched.
pub fn search_request_from_json(value: &Value) -> Result<SearchRequest, AppError> {
    let query = value
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .ok_or_else(|| AppError::bad_request("query must be a non-empty string"))?;
    let allowed_domains = optional_domains(value, "allowed_domains")?;
    let blocked_domains = optional_domains(value, "blocked_domains")?;
    let domains = match (allowed_domains, blocked_domains) {
        (Some(_), Some(_)) => {
            return Err(AppError::bad_request(
                "allowed_domains and blocked_domains cannot be combined",
            ));
        }
        (Some(domains), None) => DomainFilter::Allowed(domains),
        (None, Some(domains)) => DomainFilter::Blocked(domains),
        (None, None) => DomainFilter::All,
    };
    Ok(SearchRequest {
        query: query.to_owned(),
        options: SearchOptions {
            max_results: bounded_usize(value, "max_results", DEFAULT_MAX_RESULTS, MAX_MAX_RESULTS)?,
            max_uses: bounded_i64(value, "max_uses", DEFAULT_MAX_USES, MAX_MAX_USES)?,
            domains,
            user_location: value
                .get("user_location")
                .map(user_location_from_json)
                .transpose()?,
        },
    })
}

/// The Messages request body that asks DeepSeek for one search.
pub fn request_body(model: &str, request: &SearchRequest) -> Value {
    let mut tool = json!({
        "type": WEB_SEARCH_TOOL,
        "name": "web_search",
        "max_uses": request.options.max_uses,
    });
    match &request.options.domains {
        DomainFilter::All => {}
        DomainFilter::Allowed(domains) => tool["allowed_domains"] = json!(domains),
        DomainFilter::Blocked(domains) => tool["blocked_domains"] = json!(domains),
    }
    if let Some(location) = &request.options.user_location {
        tool["user_location"] = location.clone();
    }
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "messages": [{
            "role": "user",
            "content": [{
                "type": "text",
                "text": format!("Perform a web search for the query: {}", request.query),
            }],
        }],
        "tools": [tool],
    })
}

/// Citeable sources of one Messages response, deduplicated by URL and capped at
/// the request's bound, together with whether the cap dropped anything.
///
/// Only structured `web_search_tool_result` blocks are read: model prose is not
/// an answer, and a response without a citeable result is a failure rather than
/// an empty success.
pub fn sources_from_response(
    response: &Value,
    max_results: usize,
) -> Result<(Vec<Value>, bool), AppError> {
    let blocks = content_blocks(response);
    let snippets = citation_snippets(blocks);
    let mut seen = HashSet::new();
    let mut sources = Vec::new();
    for item in result_items(blocks) {
        let Some(url) = item
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| !url.trim().is_empty())
        else {
            continue;
        };
        // A `max_uses > 1` turn can surface the same page across searches.
        if !seen.insert(url) {
            continue;
        }
        sources.push(source_json(item, url, snippets.get(url)));
    }
    if sources.is_empty() {
        return Err(AppError::upstream_with_reason(
            502,
            "web_search_data_missing",
            format!("Web search failed: {}.", failed_turn_context(response)),
        ));
    }
    let truncated = sources.len() > max_results;
    sources.truncate(max_results);
    Ok((sources, truncated))
}

/// The queries DeepSeek actually searched for, in order and deduplicated. The
/// server may rewrite the caller's wording, so this is what the turn did.
pub fn queries_from_response(response: &Value) -> Vec<String> {
    let mut seen = HashSet::new();
    content_blocks(response)
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("server_tool_use"))
        .filter(|block| block.get("name").and_then(Value::as_str) == Some("web_search"))
        .filter_map(|block| block.pointer("/input/query").and_then(Value::as_str))
        .map(str::trim)
        .filter(|query| !query.is_empty() && seen.insert(query.to_owned()))
        .map(str::to_owned)
        .collect()
}

/// Token usage of one Messages response in this proxy's accounting vocabulary.
///
/// ASSUMPTION: Anthropic-compatible usage counts are disjoint — `input_tokens`
/// is uncached input, and cached input is reported separately — so billing
/// input is their sum. DeepSeek's OpenAI-compatible `prompt_tokens` instead
/// folds cache hits into the total; if its Anthropic-compatible layer does the
/// same, a search turn charges its cached prefix at the cache-miss rate. The
/// raw upstream numbers stay in the audit row's archived response body, so a
/// misread is visible per call and recomputable before prices change.
pub fn usage_from_response(response: &Value) -> Usage {
    let count = |field: &str| {
        response
            .get("usage")
            .and_then(|usage| usage.get(field))
            .and_then(Value::as_i64)
            .unwrap_or_default()
    };
    let cached_tokens = count("cache_read_input_tokens");
    Usage {
        input_tokens: count("input_tokens") + count("cache_creation_input_tokens") + cached_tokens,
        output_tokens: count("output_tokens"),
        cached_tokens,
    }
}

fn bounded_usize(
    value: &Value,
    field: &str,
    fallback: usize,
    maximum: usize,
) -> Result<usize, AppError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(fallback),
        Some(bound) => bound
            .as_u64()
            .filter(|bound| (1..=maximum as u64).contains(bound))
            .map(|bound| bound as usize)
            .ok_or_else(|| {
                AppError::bad_request(format!(
                    "{field} must be an integer between 1 and {maximum}"
                ))
            }),
    }
}

fn bounded_i64(value: &Value, field: &str, fallback: i64, maximum: i64) -> Result<i64, AppError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(fallback),
        Some(bound) => bound
            .as_i64()
            .filter(|bound| (1..=maximum).contains(bound))
            .ok_or_else(|| {
                AppError::bad_request(format!(
                    "{field} must be an integer between 1 and {maximum}"
                ))
            }),
    }
}

fn optional_domains(value: &Value, field: &str) -> Result<Option<Vec<String>>, AppError> {
    let Some(value) = value.get(field) else {
        return Ok(None);
    };
    let domains = value
        .as_array()
        .filter(|domains| !domains.is_empty())
        .ok_or_else(|| AppError::bad_request(format!("{field} must be a non-empty array")))?;
    let domains = domains
        .iter()
        .map(|domain| {
            domain
                .as_str()
                .map(str::trim)
                .filter(|domain| {
                    !domain.is_empty()
                        && !domain.contains(['/', '\\'])
                        && !domain.contains(char::is_whitespace)
                })
                .map(str::to_owned)
                .ok_or_else(|| {
                    AppError::bad_request(format!(
                        "{field} must contain bare domain names like example.com"
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(domains))
}

/// Normalize the tool's `user_location` object: the location type is always
/// `approximate`, and the locality fields are optional free-form strings.
fn user_location_from_json(value: &Value) -> Result<Value, AppError> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::bad_request("user_location must be an object"))?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "type" | "country" | "region" | "city" | "timezone"
        )
    }) {
        return Err(AppError::bad_request(
            "user_location supports country, region, city, timezone only",
        ));
    }
    if let Some(kind) = object.get("type")
        && kind.as_str() != Some("approximate")
    {
        return Err(AppError::bad_request(
            "user_location type must be approximate",
        ));
    }
    let mut location = serde_json::Map::new();
    location.insert("type".to_owned(), json!("approximate"));
    for key in ["country", "region", "city", "timezone"] {
        if let Some(field) = object.get(key) {
            let field = field
                .as_str()
                .map(str::trim)
                .filter(|field| !field.is_empty())
                .ok_or_else(|| {
                    AppError::bad_request(format!("user_location {key} must be a non-empty string"))
                })?;
            location.insert(key.to_owned(), json!(field));
        }
    }
    Ok(Value::Object(location))
}

fn content_blocks(response: &Value) -> &[Value] {
    response
        .get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn source_json(item: &Value, url: &str, snippet: Option<&String>) -> Value {
    let mut source = json!({"url": url});
    for (field, output) in [("title", "title"), ("page_age", "published_at")] {
        if let Some(value) = item
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            source[output] = json!(value);
        }
    }
    if let Some(snippet) = snippet {
        source["snippet"] = json!(snippet);
    }
    source
}

/// Every `web_search_result` item of every `web_search_tool_result` block.
fn result_items(blocks: &[Value]) -> impl Iterator<Item = &Value> {
    blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("web_search_tool_result"))
        .filter_map(|block| block.get("content").and_then(Value::as_array))
        .flatten()
}

/// `url → cited_text` from the citations of every `text` block, which is where
/// DeepSeek puts the excerpt: `web_search_result` items carry the URL, title,
/// and page age but no inline snippet. The first excerpt for a URL wins.
fn citation_snippets(blocks: &[Value]) -> HashMap<String, String> {
    let mut snippets = HashMap::new();
    for block in blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
    {
        for citation in block
            .get("citations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let url = citation.get("url").and_then(Value::as_str);
            let text = citation.get("cited_text").and_then(Value::as_str);
            if let (Some(url), Some(text)) = (url, text)
                && !url.is_empty()
                && !text.is_empty()
            {
                snippets
                    .entry(url.to_owned())
                    .or_insert_with(|| text.to_owned());
            }
        }
    }
    snippets
}

/// Why a 200 Messages response held no citeable result: how many searches the
/// server actually ran, and how the turn ended.
fn failed_turn_context(response: &Value) -> String {
    let searches = response
        .pointer("/usage/server_tool_use/web_search_requests")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let stop_reason = response
        .get("stop_reason")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    format!(
        "the DeepSeek response carried no web search results after {searches} server-side search(es) (stop_reason: {stop_reason})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with(content: Value) -> Value {
        json!({"content": content, "usage": {"input_tokens": 9, "output_tokens": 30, "cache_read_input_tokens": 4, "cache_creation_input_tokens": 2}})
    }

    fn request(body: Value) -> SearchRequest {
        search_request_from_json(&body).unwrap()
    }

    #[test]
    fn a_bare_query_becomes_one_native_search() {
        let body = request_body("deepseek-flash", &request(json!({"query":" rust 1.93 "})));

        assert_eq!(body["model"], "deepseek-flash");
        assert_eq!(
            body["tools"][0],
            json!({"type":"web_search_20250305","name":"web_search","max_uses":5})
        );
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            "Perform a web search for the query: rust 1.93"
        );
    }

    #[test]
    fn optional_parameters_reach_the_native_tool() {
        let search = request(json!({
            "query": "rust 1.93",
            "max_uses": 2,
            "blocked_domains": [" example.com "],
            "user_location": {"country": "CN", "city": "Shanghai", "timezone": "Asia/Shanghai"},
        }));

        assert_eq!(search.options.max_uses, 2);
        assert_eq!(search.options.max_results, 8);
        assert_eq!(
            request_body("deepseek-flash", &search)["tools"][0],
            json!({
                "type":"web_search_20250305",
                "name":"web_search",
                "max_uses":2,
                "blocked_domains":["example.com"],
                "user_location":{"type":"approximate","country":"CN","city":"Shanghai","timezone":"Asia/Shanghai"},
            })
        );
    }

    #[test]
    fn malformed_search_controls_are_rejected_instead_of_ignored() {
        for (body, expected) in [
            (json!({}), "query must be a non-empty string"),
            (
                json!({"query":"rust","max_results":51}),
                "max_results must be an integer between 1 and 50",
            ),
            (
                json!({"query":"rust","max_uses":6}),
                "max_uses must be an integer between 1 and 5",
            ),
            (
                json!({"query":"rust","allowed_domains":[]}),
                "allowed_domains must be a non-empty array",
            ),
            (
                json!({"query":"rust","allowed_domains":["https://example.com"]}),
                "allowed_domains must contain bare domain names like example.com",
            ),
            (
                json!({"query":"rust","allowed_domains":["example.com"],"blocked_domains":["example.org"]}),
                "allowed_domains and blocked_domains cannot be combined",
            ),
            (
                json!({"query":"rust","user_location":{"city":""}}),
                "user_location city must be a non-empty string",
            ),
            (
                json!({"query":"rust","user_location":{"type":"exact"}}),
                "user_location type must be approximate",
            ),
            (
                json!({"query":"rust","user_location":{"latitude":1}}),
                "user_location supports country, region, city, timezone only",
            ),
        ] {
            let error = search_request_from_json(&body).unwrap_err();
            assert_eq!(error.message(), expected, "{body}");
        }
    }

    #[test]
    fn sources_join_citations_dedupe_by_url_and_truncate() {
        let response = response_with(json!([
            {
                "type": "web_search_tool_result",
                "content": [
                    {"type": "web_search_result", "url": "https://example.com/one", "title": "One", "page_age": "2026-10-01"},
                    {"type": "web_search_result", "url": "https://example.com/two"},
                ],
            },
            { "type": "text", "text": "prose", "citations": [
                {"type": "web_search_result_location", "url": "https://example.com/two", "cited_text": "two excerpt"},
                {"type": "web_search_result_location", "url": "https://example.com/two", "cited_text": "ignored second excerpt"},
            ]},
            {
                "type": "web_search_tool_result",
                "content": [
                    {"type": "web_search_result", "url": "https://example.com/one", "title": "One again"},
                    {"type": "web_search_result", "url": "https://example.com/three", "title": "Three"},
                ],
            },
        ]));

        let (sources, truncated) = sources_from_response(&response, 2).unwrap();

        assert!(truncated, "the bound is enforced on the way back");
        assert_eq!(
            sources,
            vec![
                json!({"url": "https://example.com/one", "title": "One", "published_at": "2026-10-01"}),
                json!({"url": "https://example.com/two", "snippet": "two excerpt"}),
            ]
        );
    }

    #[test]
    fn sources_within_the_bound_are_not_reported_as_truncated() {
        let response = response_with(json!([{
            "type": "web_search_tool_result",
            "content": [{"type": "web_search_result", "url": "https://example.com/one"}],
        }]));
        let (sources, truncated) = sources_from_response(&response, 8).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(!truncated);
    }

    #[test]
    fn the_searched_queries_come_from_the_server_tool_use_blocks() {
        let response = response_with(json!([
            {"type": "server_tool_use", "name": "web_search", "input": {"query": "rust 1.93"}},
            {"type": "server_tool_use", "name": "web_search", "input": {"query": "rust 1.93"}},
            {"type": "server_tool_use", "name": "web_search", "input": {"query": " "}},
            {"type": "text", "text": "prose"},
        ]));
        assert_eq!(queries_from_response(&response), ["rust 1.93"]);
    }

    #[test]
    fn a_response_without_search_results_fails_loudly() {
        let response = json!({
            "content": [{"type": "text", "text": "I already know the answer."}],
            "stop_reason": "end_turn",
            "usage": {"server_tool_use": {"web_search_requests": 0}},
        });
        let error = sources_from_response(&response, 8).unwrap_err();
        assert_eq!(error.reason(), Some("web_search_data_missing"));
        assert_eq!(error.status(), axum::http::StatusCode::BAD_GATEWAY);
        assert!(error.message().contains("after 0 server-side search(es)"));
    }

    #[test]
    fn usage_sums_the_disjoint_anthropic_counts() {
        let usage = usage_from_response(&response_with(json!([])));
        assert_eq!(usage.input_tokens, 15);
        assert_eq!(usage.output_tokens, 30);
        assert_eq!(usage.cached_tokens, 4);
    }
}
