//! DeepSeek Search API.
//!
//! `POST /v1/tools/search` is DeepSeek's own search endpoint: one JSON request
//! per query, answered by its own engine, billed per successful call (CNY 5 per
//! 1,000) instead of by token. No model runs, so one search is one round trip
//! and one audit row — there is no turn to prompt and no prose to discard.

use serde_json::{Value, json};

use crate::AppError;

/// Path appended to the configured upstream base URL.
pub const SEARCH_PATH: &str = "/v1/tools/search";

/// Value stored in the audit row's `model` column. A search runs no model, but
/// operators filter the audit page by model and the console links there, so the
/// endpoint names itself instead of leaving the column blank.
pub const AUDIT_MODEL: &str = "deepseek-search";

/// Results requested when the caller asks for none.
pub const DEFAULT_MAX_RESULTS: usize = 8;

/// Largest accepted `max_results`, which is the endpoint's `count` range.
pub const MAX_MAX_RESULTS: usize = 50;

/// Longest `query` DeepSeek accepts, domain operators included.
const MAX_QUERY_CHARS: usize = 128;

#[derive(Debug)]
/// One validated `/v1/web-search` request, ready to become a Search API body.
pub struct SearchRequest {
    query: String,
    max_results: usize,
    need_content: bool,
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
    let allowed = optional_domains(value, "allowed_domains")?;
    let blocked = optional_domains(value, "blocked_domains")?;
    if allowed.is_some() && blocked.is_some() {
        return Err(AppError::bad_request(
            "allowed_domains and blocked_domains cannot be combined",
        ));
    }
    let query = with_domain_operators(query, allowed.as_deref(), blocked.as_deref());
    // INVARIANT: the caller learns an over-long query from us, with our own
    // message, instead of from DeepSeek's 400 after the search was attempted.
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::bad_request(format!(
            "query with domain filters must stay within {MAX_QUERY_CHARS} characters"
        )));
    }
    Ok(SearchRequest {
        query,
        max_results: bounded_usize(value, "max_results", DEFAULT_MAX_RESULTS, MAX_MAX_RESULTS)?,
        need_content: optional_bool(value, "need_content")?,
    })
}

/// The Search API request body: the query carries the domain operators, and
/// `count` is the result bound DeepSeek applies itself.
pub fn request_body(request: &SearchRequest) -> Value {
    json!({
        "query": request.query,
        "count": request.max_results,
        "need_content": request.need_content,
    })
}

/// The results of one Search API response as this proxy's source objects.
///
/// A 200 answer with no results is not an empty success: the caller asked for
/// pages and got none, so it fails like any other search that found nothing.
pub fn sources_from_response(response: &Value) -> Result<Vec<Value>, AppError> {
    let Some(results) = response.get("results").and_then(Value::as_array) else {
        return Err(AppError::upstream_with_reason(
            502,
            "web_search_data_missing",
            "Web search failed: the DeepSeek response carried no results array.",
        ));
    };
    let sources = results.iter().filter_map(source_json).collect::<Vec<_>>();
    if sources.is_empty() {
        return Err(AppError::upstream_with_reason(
            502,
            "web_search_data_missing",
            "Web search failed: DeepSeek returned no results.",
        ));
    }
    Ok(sources)
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

fn optional_bool(value: &Value, field: &str) -> Result<bool, AppError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(false),
        Some(flag) => flag
            .as_bool()
            .ok_or_else(|| AppError::bad_request(format!("{field} must be a boolean"))),
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

/// Fold a domain filter into the query with the Search API's own operators:
/// several allowed domains are OR-ed, blocked ones are all excluded.
fn with_domain_operators(
    query: &str,
    allowed: Option<&[String]>,
    blocked: Option<&[String]>,
) -> String {
    if let Some(domains) = allowed {
        let terms = domains
            .iter()
            .map(|domain| format!("site:{domain}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        return format!("{query} {terms}");
    }
    if let Some(domains) = blocked {
        let terms = domains
            .iter()
            .map(|domain| format!("-site:{domain}"))
            .collect::<Vec<_>>()
            .join(" ");
        return format!("{query} {terms}");
    }
    query.to_owned()
}

/// One `results[]` entry as a source object: the url is what a caller cites, so
/// an entry without one is dropped rather than surfaced as a dead link.
fn source_json(item: &Value) -> Option<Value> {
    let url = item
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())?;
    let mut source = json!({ "url": url });
    for (field, output) in [
        ("title", "title"),
        ("snippet", "snippet"),
        ("summary", "summary"),
        ("content", "content"),
        ("date", "published_at"),
        ("site_name", "site_name"),
        ("authority", "authority"),
        ("rerank_score", "rerank_score"),
    ] {
        let Some(value) = item.get(field) else {
            continue;
        };
        if value.is_null() || value.as_str().is_some_and(|text| text.trim().is_empty()) {
            continue;
        }
        source[output] = value.clone();
    }
    Some(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(body: Value) -> SearchRequest {
        search_request_from_json(&body).unwrap()
    }

    #[test]
    fn a_bare_query_becomes_one_counted_search() {
        let body = request_body(&request(json!({"query":" rust 1.93 "})));
        assert_eq!(
            body,
            json!({"query":"rust 1.93","count":8,"need_content":false})
        );
    }

    #[test]
    fn domain_filters_become_search_operators() {
        assert_eq!(
            request_body(&request(
                json!({"query":"rust","allowed_domains":["a.com","b.com"]})
            )),
            json!({"query":"rust site:a.com OR site:b.com","count":8,"need_content":false})
        );
        assert_eq!(
            request_body(&request(
                json!({"query":"rust","blocked_domains":["a.com","b.com"]})
            )),
            json!({"query":"rust -site:a.com -site:b.com","count":8,"need_content":false})
        );
    }

    #[test]
    fn a_result_bound_and_content_request_reach_the_body() {
        let body = request_body(&request(
            json!({"query":"rust","max_results":50,"need_content":true}),
        ));
        assert_eq!(body["count"], 50);
        assert_eq!(body["need_content"], true);
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
                json!({"query":"rust","need_content":"yes"}),
                "need_content must be a boolean",
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
                json!({"query":"rust","blocked_domains":["a.com","b.com","c.com","d.com","e.com","f.com","g.com","h.com","i.com","j.com","k.com","l.com","m.com","n.com","o.com","p.com","q.com","r.com","s.com","t.com"]}),
                "query with domain filters must stay within 128 characters",
            ),
        ] {
            let error = search_request_from_json(&body).unwrap_err();
            assert_eq!(error.message(), expected, "{body}");
        }
    }

    #[test]
    fn results_become_sources_and_an_empty_answer_fails_loudly() {
        let response = json!({"results":[
            {
                "url": "https://example.com/one",
                "title": "One",
                "snippet": "short",
                "summary": "longer",
                "content": "# Page",
                "site_name": "Example",
                "date": "2026-10-01T00:00:00+00:00",
                "authority": 3,
                "rerank_score": 2.4,
            },
            {"url": "https://example.com/two", "title": ""},
            {"title": "no url"},
        ]});
        assert_eq!(
            sources_from_response(&response).unwrap(),
            vec![
                json!({
                    "url": "https://example.com/one",
                    "title": "One",
                    "snippet": "short",
                    "summary": "longer",
                    "content": "# Page",
                    "published_at": "2026-10-01T00:00:00+00:00",
                    "site_name": "Example",
                    "authority": 3,
                    "rerank_score": 2.4,
                }),
                json!({"url": "https://example.com/two"}),
            ]
        );

        for empty in [json!({"results":[]}), json!({})] {
            let error = sources_from_response(&empty).unwrap_err();
            assert_eq!(error.reason(), Some("web_search_data_missing"));
            assert_eq!(error.status(), axum::http::StatusCode::BAD_GATEWAY);
        }
    }
}
