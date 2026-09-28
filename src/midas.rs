use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{AppError, AppState, config::Config};

#[derive(Deserialize)]
struct InboundTransferSenderSummary {
    sender_user_id: String,
    recipient_user_id: String,
    total_received_usd_nanos: i64,
}

#[derive(Deserialize)]
struct InboundTransferSummaryResponse {
    recipient_user_id: String,
    senders: Vec<InboundTransferSenderSummary>,
}

#[derive(Serialize)]
struct InboundTransferSummaryRequest<'a> {
    sender_user_ids: &'a [String],
}

#[derive(Deserialize)]
struct MidasError {
    error: String,
}

pub fn is_configured(config: &Config) -> bool {
    config.midas_fund_user_id.is_some() && config.midas_fund_api_key.is_some()
}

pub async fn inbound_transfer_totals(
    state: &AppState,
    sender_user_ids: &[String],
) -> Result<HashMap<String, i64>, AppError> {
    if sender_user_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let config = state.config.load_full();
    let (api_base, fund_user_id, api_key) = fund_credentials(&config)?;
    let mut totals = HashMap::new();
    for sender_user_ids in sender_user_ids.chunks(100) {
        let response = state
            .client
            .post(format!("{api_base}/internal-transfers/me/inbound/summary"))
            .header("x-api-key", api_key)
            .json(&InboundTransferSummaryRequest { sender_user_ids })
            .send()
            .await
            .map_err(|_| AppError::unavailable("Midas payment service is unavailable"))?;
        let response = successful_midas_response(response).await?;
        let response: InboundTransferSummaryResponse = response.json().await.map_err(|_| {
            AppError::upstream(502, "Midas returned invalid inbound-transfer summary JSON")
        })?;
        if response.recipient_user_id != fund_user_id {
            return Err(AppError::upstream(
                502,
                "Midas fund API key does not match the configured public wallet user ID",
            ));
        }
        let requested = sender_user_ids.iter().collect::<HashSet<_>>();
        let summaries = response
            .senders
            .into_iter()
            .map(|summary| {
                if summary.recipient_user_id != fund_user_id {
                    return Err(AppError::upstream(
                        502,
                        "Midas returned an inbound transfer for a different public wallet",
                    ));
                }
                if summary.total_received_usd_nanos < 0 {
                    return Err(AppError::upstream(
                        502,
                        "Midas returned a negative inbound-transfer total",
                    ));
                }
                Ok((summary.sender_user_id, summary.total_received_usd_nanos))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        if summaries.len() != requested.len()
            || requested
                .iter()
                .any(|user_id| !summaries.contains_key(*user_id))
        {
            return Err(AppError::upstream(
                502,
                "Midas inbound-transfer summary did not include every requested user",
            ));
        }
        totals.extend(summaries);
    }
    Ok(totals)
}

fn fund_credentials(config: &Config) -> Result<(&str, &str, &str), AppError> {
    let fund_user_id = config
        .midas_fund_user_id
        .as_deref()
        .ok_or_else(|| AppError::unavailable("Midas is not configured"))?;
    let api_key = config
        .midas_fund_api_key
        .as_deref()
        .ok_or_else(|| AppError::unavailable("Midas is not configured"))?;
    Ok((&config.midas_api_base, fund_user_id, api_key))
}

async fn successful_midas_response(
    response: reqwest::Response,
) -> Result<reqwest::Response, AppError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let error = response
        .json::<MidasError>()
        .await
        .ok()
        .map(|body| body.error)
        .unwrap_or_else(|| format!("Midas returned {status}"));
    if matches!(status.as_u16(), 401 | 403) {
        return Err(AppError::unavailable(
            "Midas fund credentials are not configured correctly",
        ));
    }
    if status.is_server_error() {
        return Err(AppError::unavailable(
            "Midas payment service is unavailable",
        ));
    }
    Err(AppError::upstream(status.as_u16(), error))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        Json, Router,
        body::Bytes,
        extract::{OriginalUri, State},
        http::HeaderMap,
        response::{IntoResponse, Response},
        routing::post,
    };
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Clone)]
    struct RecordedRequest {
        path: String,
        headers: HeaderMap,
        body: Bytes,
    }

    #[derive(Clone)]
    struct MockMidas {
        requests: Arc<Mutex<Vec<RecordedRequest>>>,
    }

    async fn mock_midas(
        State(mock): State<MockMidas>,
        OriginalUri(uri): OriginalUri,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        mock.requests.lock().await.push(RecordedRequest {
            path: uri.path().to_owned(),
            headers,
            body,
        });
        Json(serde_json::json!({
            "recipient_user_id":"fund",
            "senders":[{
                "sender_user_id":"payer",
                "recipient_user_id":"fund",
                "total_received_usd_nanos":7_000_000_000_i64,
                "total_received_usd":"7.000000000",
                "transfer_count":1,
                "last_received_at":"2026-09-10T00:00:00Z"
            }]
        }))
        .into_response()
    }

    async fn state_with_mock_midas() -> (AppState, Arc<Mutex<Vec<RecordedRequest>>>) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route(
                "/api/internal-transfers/me/inbound/summary",
                post(mock_midas),
            )
            .with_state(MockMidas {
                requests: requests.clone(),
            });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let state = crate::test_state("http://token.invalid").await;
        let mut config = (*state.config.load_full()).clone();
        config.midas_api_base = format!("http://{address}/api");
        config.midas_fund_user_id = Some("fund".to_owned());
        config.midas_fund_api_key = Some("midas_fund_test".to_owned());
        state.config.store(std::sync::Arc::new(config));
        (state, requests)
    }

    #[tokio::test]
    async fn inbound_transfer_totals_use_the_configured_fund_user() {
        let (state, requests) = state_with_mock_midas().await;
        let totals = inbound_transfer_totals(&state, &["payer".to_owned()])
            .await
            .unwrap();
        assert_eq!(totals["payer"], 7_000_000_000);

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].path,
            "/api/internal-transfers/me/inbound/summary"
        );
        assert_eq!(requests[0].headers["x-api-key"], "midas_fund_test");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap(),
            serde_json::json!({"sender_user_ids":["payer"]})
        );
    }
}
