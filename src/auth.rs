use std::{convert::Infallible, sync::Arc};

use auth_mini_axum::{AuthMiniError, AuthMiniLayer, AuthMiniPrincipal, JwksCachePolicy};
use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tokio::sync::RwLock;
use tower::{Layer, ServiceExt, service_fn};

use crate::{AppError, AppState, crypto::consumer_secret_hash};

#[derive(Clone, Debug, Serialize)]
pub struct UserIdentity {
    pub id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub role: String,
}

#[derive(Clone, Debug)]
pub struct ApiIdentity {
    pub consumer_id: String,
    pub user_id: String,
    pub request_archive: bool,
    pub is_admin: bool,
    pub allow_debt: bool,
}

#[derive(Clone, Debug)]
pub struct VerifiedIdentity {
    pub id: String,
}

#[derive(Clone)]
pub struct AuthManager {
    layer: Arc<RwLock<Option<AuthMiniLayer>>>,
}

impl AuthManager {
    pub async fn new(issuer: Option<String>, audience: Option<String>) -> anyhow::Result<Self> {
        let layer = match (issuer, audience) {
            (None, None) => None,
            (Some(issuer), Some(audience)) => Some(create_layer(&issuer, audience).await?),
            _ => anyhow::bail!("Auth Mini issuer and audience must be configured together"),
        };
        Ok(Self {
            layer: Arc::new(RwLock::new(layer)),
        })
    }

    #[cfg(test)]
    pub async fn configure(&self, issuer: String, audience: String) -> Result<(), AppError> {
        self.install(create_layer(&issuer, audience).await.map_err(auth_error)?)
            .await;
        Ok(())
    }

    pub async fn install(&self, layer: AuthMiniLayer) {
        *self.layer.write().await = Some(layer);
    }

    pub async fn verify_candidate(
        &self,
        issuer: String,
        audience: String,
        token: &str,
    ) -> Result<(VerifiedIdentity, AuthMiniLayer), AppError> {
        let layer = create_layer(&issuer, audience).await.map_err(auth_error)?;
        let principal = layer.verifier().verify(token).await.map_err(auth_error)?;
        Ok((
            VerifiedIdentity {
                id: principal.subject,
            },
            layer,
        ))
    }

    async fn layer(&self) -> Result<AuthMiniLayer, AppError> {
        self.layer
            .read()
            .await
            .clone()
            .ok_or_else(|| AppError::unavailable("DeepSeek-LB setup is not complete"))
    }
}

async fn create_layer(
    issuer: &str,
    audience: String,
) -> Result<AuthMiniLayer, auth_mini_axum::AuthMiniError> {
    AuthMiniLayer::from_issuer(issuer, audience, JwksCachePolicy::default()).await
}

fn auth_error(error: AuthMiniError) -> AppError {
    match error {
        AuthMiniError::JwksUnavailable => AppError::unavailable("Auth Mini JWKS is unavailable"),
        AuthMiniError::InvalidIssuer => AppError::bad_request("Auth Mini issuer is not valid"),
        AuthMiniError::InvalidToken => AppError::unauthorized("invalid or expired bearer token"),
    }
}

pub async fn authenticate(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let layer = match state.auth.layer().await {
        Ok(layer) => layer,
        Err(error) => return error.into_response(),
    };
    let service = layer.layer(service_fn(move |mut request: Request| {
        let state = state.clone();
        let next = next.clone();
        async move {
            let principal = request
                .extensions()
                .get::<AuthMiniPrincipal>()
                .cloned()
                .expect("Auth Mini layer inserts a verified principal");
            let user = match upsert_user(&state.db, &state.write_gate, &principal.subject).await {
                Ok(user) => user,
                Err(error) => return Ok::<_, Infallible>(error.into_response()),
            };
            request.extensions_mut().insert(user);
            Ok::<_, Infallible>(next.run(request).await)
        }
    }));
    match service.oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

async fn upsert_user(
    pool: &SqlitePool,
    write_gate: &crate::SqliteWriteGate,
    user_id: &str,
) -> Result<UserIdentity, AppError> {
    let now = chrono::Utc::now().timestamp();
    {
        let _write = write_gate.lock().await;
        sqlx::query(
            "INSERT INTO users(id,role,created_at) VALUES(?,'user',?) ON CONFLICT(id) DO NOTHING",
        )
        .bind(user_id)
        .bind(now)
        .execute(pool)
        .await?;
    }
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id=?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(UserIdentity {
        id: user_id.to_owned(),
        email: None,
        name: None,
        role,
    })
}

pub async fn api_identity(state: &AppState, headers: &HeaderMap) -> Result<ApiIdentity, AppError> {
    let secret = bearer(headers)?;
    if !secret.starts_with("sk-") {
        return Err(AppError::unauthorized("invalid consumer credential"));
    }
    let row = sqlx::query(
        "SELECT k.id,k.user_id,u.role,k.request_archive,u.allow_debt FROM consumers k JOIN users u ON u.id=k.user_id WHERE k.secret_hash=? AND k.is_deleted=0 AND k.is_disabled=0",
    )
    .bind(consumer_secret_hash(secret))
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::unauthorized("invalid consumer credential"))?;
    let role: String = row.get(2);
    Ok(ApiIdentity {
        consumer_id: row.get(0),
        user_id: row.get(1),
        request_archive: row.get::<i64, _>(3) != 0,
        is_admin: role != "user",
        allow_debt: row.get::<i64, _>(4) != 0,
    })
}

pub fn require_admin(identity: &UserIdentity) -> Result<(), AppError> {
    is_admin(identity)
        .then_some(())
        .ok_or_else(|| AppError::forbidden("administrator access required"))
}

pub fn is_admin(identity: &UserIdentity) -> bool {
    matches!(identity.role.as_str(), "root" | "admin")
}

pub fn require_root(identity: &UserIdentity) -> Result<(), AppError> {
    (identity.role == "root")
        .then_some(())
        .ok_or_else(|| AppError::forbidden("root access required"))
}

pub(crate) fn bearer(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unauthorized("missing bearer token"))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};
    use base64::Engine;
    use ed25519_dalek::Signer;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn configured_issuer_requires_an_audience() {
        assert!(
            AuthManager::new(Some("https://auth.example.com".to_owned()), None)
                .await
                .is_err()
        );
    }

    fn browser_token(
        signing: &ed25519_dalek::SigningKey,
        issuer: &str,
        audiences: serde_json::Value,
    ) -> String {
        let encoder = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let header = encoder.encode(serde_json::json!({"alg":"EdDSA","kid":"test"}).to_string());
        let now = chrono::Utc::now().timestamp();
        let claims = encoder.encode(
            serde_json::json!({
                "sub":"user-1",
                "sid":"session-1",
                "iss":issuer,
                "aud":audiences,
                "typ":"access",
                "amr":["webauthn"],
                "iat":now,
                "exp":now + 900,
            })
            .to_string(),
        );
        let signing_input = format!("{header}.{claims}");
        let signature = encoder.encode(signing.sign(signing_input.as_bytes()).to_bytes());
        format!("{signing_input}.{signature}")
    }

    #[tokio::test]
    async fn configured_layer_accepts_dual_audience_token_only_when_own_audience_is_present() {
        let signing = ed25519_dalek::SigningKey::from_bytes(&[8_u8; 32]);
        let x = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(signing.verifying_key().to_bytes());
        let jwks = axum::Router::new().route(
            "/jwks",
            axum::routing::get(move || {
                let x = x.clone();
                async move { axum::Json(serde_json::json!({"keys":[{"kid":"test","kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","x":x}]})) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, jwks).await.unwrap() });
        let layer = create_layer(&issuer, "deepseek-lb.test".to_owned())
            .await
            .unwrap();
        assert!(
            layer
                .verifier()
                .verify(&browser_token(
                    &signing,
                    &issuer,
                    serde_json::json!(["deepseek-lb.test", "linkit.ntnl.io"])
                ))
                .await
                .is_ok()
        );
        assert!(
            layer
                .verifier()
                .verify(&browser_token(
                    &signing,
                    &issuer,
                    serde_json::json!(["linkit.ntnl.io"])
                ))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn configured_layer_rejects_missing_bearer_token() {
        let signing = ed25519_dalek::SigningKey::from_bytes(&[9_u8; 32]);
        let x = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(signing.verifying_key().to_bytes());
        let jwks = axum::Router::new().route(
            "/jwks",
            axum::routing::get(move || {
                let x = x.clone();
                async move {
                    axum::Json(serde_json::json!({
                        "keys":[{"kid":"test","kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","x":x}]
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, jwks).await.unwrap() });

        let state = crate::test_state("http://token.invalid").await;
        state
            .auth
            .configure(issuer, "deepseek-lb.test".to_owned())
            .await
            .unwrap();
        let response = crate::router(state)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/me")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn consumer_auth_accepts_active_hash_and_rejects_deleted_key() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('user-1','user',?)")
            .bind(now)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('key-1','user-1','test','sk-test',?,?)")
            .bind(consumer_secret_hash("sk-test-secret")).bind(now).execute(&state.db).await.unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-test-secret"),
        );
        let identity = api_identity(&state, &headers).await.unwrap();
        assert_eq!(
            (identity.consumer_id.as_str(), identity.user_id.as_str()),
            ("key-1", "user-1")
        );
        sqlx::query("UPDATE consumers SET is_disabled=1 WHERE id='key-1'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(api_identity(&state, &headers).await.is_err());
        sqlx::query("UPDATE consumers SET is_disabled=0 WHERE id='key-1'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(api_identity(&state, &headers).await.is_ok());
        sqlx::query("UPDATE consumers SET is_deleted=1 WHERE id='key-1'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(api_identity(&state, &headers).await.is_err());
    }

    #[tokio::test]
    async fn consumer_identity_carries_admin_and_debt_permissions() {
        let state = crate::test_state("http://token.invalid").await;
        let now = chrono::Utc::now().timestamp();
        for (id, role, secret) in [
            ("tenant", "user", "sk-tenant-secret"),
            ("admin", "admin", "sk-admin-secret"),
        ] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,?,?)")
                .bind(id)
                .bind(role)
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES(?,?,?,?,?,?)")
                .bind(format!("key-{id}"))
                .bind(id)
                .bind("test")
                .bind("sk-test")
                .bind(consumer_secret_hash(secret))
                .bind(now)
                .execute(&state.db)
                .await
                .unwrap();
        }
        let mut tenant_headers = HeaderMap::new();
        tenant_headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-tenant-secret"),
        );
        let tenant = api_identity(&state, &tenant_headers).await.unwrap();
        assert!(!tenant.is_admin);
        assert!(!tenant.allow_debt);
        sqlx::query("UPDATE users SET allow_debt=1 WHERE id='tenant'")
            .execute(&state.db)
            .await
            .unwrap();
        let tenant = api_identity(&state, &tenant_headers).await.unwrap();
        assert!(!tenant.is_admin);
        assert!(tenant.allow_debt);
        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-admin-secret"),
        );
        assert!(api_identity(&state, &admin_headers).await.unwrap().is_admin);
    }

    #[tokio::test]
    async fn ordinary_login_never_bootstraps_privilege() {
        let state = crate::test_state("http://token.invalid").await;
        let ordinary = upsert_user(&state.db, &state.write_gate, "ordinary")
            .await
            .unwrap();
        assert_eq!(ordinary.role, "user");
    }
}
