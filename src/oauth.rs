use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{AppError, AppState, OAuthFlow, balancer::Provider};

#[derive(Debug, Serialize)]
pub struct OAuthStart {
    pub authorize_url: String,
    pub state: String,
}

#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

pub fn callback_parameters(
    state: &AppState,
    callback_url: &str,
) -> Result<(String, String), AppError> {
    let callback = Url::parse(callback_url.trim())
        .map_err(|_| AppError::bad_request("invalid OAuth callback URL"))?;
    let redirect = Url::parse(&state.config.load().oauth_redirect_uri)
        .map_err(|_| AppError::bad_request("invalid configured OAuth redirect URI"))?;
    let mut callback_base = callback.clone();
    callback_base.set_query(None);
    callback_base.set_fragment(None);
    let mut redirect_base = redirect;
    redirect_base.set_query(None);
    redirect_base.set_fragment(None);
    if callback_base != redirect_base {
        return Err(AppError::bad_request(
            "OAuth callback URL does not match the configured redirect URI",
        ));
    }
    let mut code = None;
    let mut state_value = None;
    let mut error = None;
    for (key, value) in callback.query_pairs() {
        if !value.is_empty() {
            match key.as_ref() {
                "code" => code = Some(value.into_owned()),
                "state" => state_value = Some(value.into_owned()),
                "error" => error = Some(value.into_owned()),
                _ => {}
            }
        }
    }
    if let Some(error) = error {
        return Err(AppError::bad_request(format!(
            "OpenAI OAuth authorization failed: {error}"
        )));
    }
    match (state_value, code) {
        (Some(state_value), Some(code)) => Ok((state_value, code)),
        _ => Err(AppError::bad_request(
            "OAuth callback URL must include code and state",
        )),
    }
}

pub fn account_id_from_jwt(token: &str) -> Result<String> {
    let payload = token
        .split('.')
        .nth(1)
        .context("access token is not a JWT")?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .context("invalid JWT payload")?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded)?;
    claims
        .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
        .or_else(|| claims.pointer("/https:~1~1api.openai.com~1auth/account_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .context("JWT is missing the CodeX account id")
}

pub fn expires_at_from_jwt(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<serde_json::Value>(&decoded)
        .ok()?
        .get("exp")?
        .as_i64()
}

pub async fn start(
    state: &AppState,
    user_id: &str,
    originator: &str,
) -> Result<OAuthStart, AppError> {
    let config = state.config.load();
    let random = random_bytes::<32>();
    let verifier = URL_SAFE_NO_PAD.encode(random);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let raw_state = URL_SAFE_NO_PAD.encode(random_bytes::<24>());
    let state_hash = hex::encode(Sha256::digest(raw_state.as_bytes()));
    let now = chrono::Utc::now().timestamp();
    let expires_at = now + 600;
    state.oauth_flows.retain(|_, flow| flow.expires_at > now);
    state.oauth_flows.insert(
        state_hash,
        OAuthFlow {
            verifier,
            created_by: user_id.to_owned(),
            originator: originator.to_owned(),
            expires_at,
        },
    );
    let mut url = Url::parse(&config.oauth_authorize_url)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.oauth_client_id)
        .append_pair("redirect_uri", &config.oauth_redirect_uri)
        .append_pair("scope", "openid profile email offline_access")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &raw_state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", originator);
    Ok(OAuthStart {
        authorize_url: url.into(),
        state: raw_state,
    })
}

/// Tokens exchanged for an authorization code, together with the originator the operator picked
/// when the authorization URL was created. The provider inherits that identity, so the credentials
/// stay bound to the client family they were authorized for.
pub struct ExchangedTokens {
    pub token: TokenResponse,
    pub originator: String,
}

pub async fn exchange(
    state: &AppState,
    raw_state: &str,
    code: &str,
    user_id: &str,
) -> Result<ExchangedTokens, AppError> {
    let config = state.config.load();
    let state_hash = hex::encode(Sha256::digest(raw_state.as_bytes()));
    let now = chrono::Utc::now().timestamp();
    let (_, flow) = state
        .oauth_flows
        .remove_if(&state_hash, |_, flow| {
            flow.created_by == user_id && flow.expires_at > now
        })
        .ok_or_else(|| AppError::bad_request("invalid or expired OAuth state"))?;
    let token = token_request(
        state,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &config.oauth_client_id),
            ("code", code),
            ("code_verifier", &flow.verifier),
            ("redirect_uri", &config.oauth_redirect_uri),
        ],
    )
    .await?;
    Ok(ExchangedTokens {
        token,
        originator: flow.originator,
    })
}

pub async fn refresh(state: &AppState, refresh_token: &str) -> Result<TokenResponse, AppError> {
    refresh_with_client(state, None, refresh_token).await
}

pub async fn refresh_with_client(
    state: &AppState,
    provider: Option<&Provider>,
    refresh_token: &str,
) -> Result<TokenResponse, AppError> {
    let config = state.config.load();
    token_request_with_client(
        state,
        provider,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &config.oauth_client_id),
        ],
    )
    .await
}

async fn token_request(state: &AppState, form: &[(&str, &str)]) -> Result<TokenResponse, AppError> {
    token_request_with_client(state, None, form).await
}

async fn token_request_with_client(
    state: &AppState,
    provider: Option<&Provider>,
    form: &[(&str, &str)],
) -> Result<TokenResponse, AppError> {
    let token_url = state.config.load().oauth_token_url.clone();
    let client = match provider {
        Some(provider) => state.provider_client(provider)?,
        None => state.client.clone(),
    };
    let response = client
        .post(token_url)
        .form(form)
        .send()
        .await
        .context("CodeX OAuth request failed")?;
    if !response.status().is_success() {
        return Err(AppError::upstream(
            response.status().as_u16(),
            "CodeX OAuth token exchange failed",
        ));
    }
    let token = response
        .json::<TokenResponse>()
        .await
        .context("invalid CodeX OAuth response")?;
    if token.access_token.is_empty() || token.refresh_token.is_empty() || token.expires_in <= 0 {
        return Err(AppError::upstream(
            502,
            "CodeX OAuth response is missing fields",
        ));
    }
    Ok(token)
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0_u8; N];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, routing::post};

    use super::*;

    #[test]
    fn extracts_nested_account_id() {
        let claims =
            serde_json::json!({"https://api.openai.com/auth":{"chatgpt_account_id":"acct_1"}});
        let token = format!(
            "x.{}.x",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        );
        assert_eq!(account_id_from_jwt(&token).unwrap(), "acct_1");
    }

    #[tokio::test]
    async fn reads_code_and_state_from_the_configured_callback_url() {
        let state = crate::test_state("http://token.invalid").await;
        assert_eq!(
            callback_parameters(
                &state,
                "http://localhost:1455/auth/callback?code=auth-code&state=oauth-state"
            )
            .unwrap(),
            ("oauth-state".to_owned(), "auth-code".to_owned())
        );
    }

    #[tokio::test]
    async fn rejects_a_callback_url_for_another_redirect_uri() {
        let state = crate::test_state("http://token.invalid").await;
        assert!(
            callback_parameters(
                &state,
                "https://example.com/auth/callback?code=auth-code&state=oauth-state"
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn refresh_posts_to_configured_token_endpoint() {
        let app = Router::new().route(
            "/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token":"access",
                    "refresh_token":"rotated",
                    "expires_in":3600
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let state = crate::test_state(&format!("http://{address}/token")).await;
        let token = refresh(&state, "refresh").await.unwrap();
        assert_eq!(
            (
                token.access_token.as_str(),
                token.refresh_token.as_str(),
                token.expires_in
            ),
            ("access", "rotated", 3600)
        );
    }

    #[tokio::test]
    async fn records_the_selected_originator_in_the_authorize_url() {
        let state = crate::test_state("http://token.invalid").await;
        let flow = start(&state, "admin", "pi").await.unwrap();
        assert!(
            flow.authorize_url.contains("originator=pi"),
            "{}",
            flow.authorize_url
        );
        assert!(
            flow.authorize_url
                .contains("codex_cli_simplified_flow=true"),
            "{}",
            flow.authorize_url
        );
        let stored: Vec<String> = state
            .oauth_flows
            .iter()
            .map(|entry| entry.originator.clone())
            .collect();
        assert_eq!(stored, vec!["pi".to_owned()]);
    }

    #[tokio::test]
    async fn exchange_returns_the_originator_the_flow_started_with() {
        let app = Router::new().route(
            "/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token":"access",
                    "refresh_token":"refresh",
                    "expires_in":3600
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let state = crate::test_state(&format!("http://{address}/token")).await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('admin','admin',?)")
            .bind(chrono::Utc::now().timestamp())
            .execute(&state.db)
            .await
            .unwrap();
        let flow = start(&state, "admin", "opencode").await.unwrap();
        let exchanged = exchange(&state, &flow.state, "code", "admin")
            .await
            .unwrap();
        assert_eq!(exchanged.originator, "opencode");
        assert_eq!(exchanged.token.access_token, "access");
    }

    #[tokio::test]
    async fn oauth_state_is_single_use() {
        let app = Router::new().route(
            "/token",
            post(|| async {
                Json(serde_json::json!({
                    "access_token":"access",
                    "refresh_token":"refresh",
                    "expires_in":3600
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let state = crate::test_state(&format!("http://{address}/token")).await;
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('admin','admin',?)")
            .bind(chrono::Utc::now().timestamp())
            .execute(&state.db)
            .await
            .unwrap();
        let flow_table: Option<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='oauth_flows'",
        )
        .fetch_optional(&state.db)
        .await
        .unwrap();
        assert!(flow_table.is_none());
        let flow = start(&state, "admin", crate::identity::CODEX_ORIGINATOR)
            .await
            .unwrap();
        assert_eq!(state.oauth_flows.len(), 1);
        assert!(exchange(&state, &flow.state, "code", "admin").await.is_ok());
        assert!(state.oauth_flows.is_empty());
        assert!(
            exchange(&state, &flow.state, "code", "admin")
                .await
                .is_err()
        );
    }
}
