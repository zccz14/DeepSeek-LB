use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

pub const DEFAULT_LISTEN: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080);

/// Optional User-Agent overrides applied to requests sent for each provider identity.
#[derive(Clone, Debug, Default, Serialize)]
pub struct UpstreamUserAgents {
    pub codex_cli_rs: Option<String>,
    pub pi: Option<String>,
    pub opencode: Option<String>,
}

impl UpstreamUserAgents {
    pub fn for_originator(&self, originator: &str) -> Option<&str> {
        match originator {
            "codex_cli_rs" => self.codex_cli_rs.as_deref(),
            "pi" => self.pi.as_deref(),
            "opencode" => self.opencode.as_deref(),
            _ => None,
        }
    }

    pub fn set(&mut self, originator: &str, user_agent: Option<String>) {
        match originator {
            "codex_cli_rs" => self.codex_cli_rs = user_agent,
            "pi" => self.pi = user_agent,
            "opencode" => self.opencode = user_agent,
            _ => {}
        }
    }
}

pub const DEFAULT_AVAILABLE_MODEL_IDS: &[&str] = &[
    "gpt-6-astra",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
    "gpt-5.4",
    "gpt-5.3-codex",
    "gpt-5.4-mini",
    "gpt-4o-transcribe",
    "gpt-image-1",
    "gpt-image-1.5",
    "gpt-image-2",
];

pub fn default_available_model_ids() -> Vec<String> {
    DEFAULT_AVAILABLE_MODEL_IDS
        .iter()
        .map(|model| (*model).to_owned())
        .collect()
}

pub fn normalize_available_model_ids(
    model_ids: Vec<String>,
) -> std::result::Result<Vec<String>, String> {
    if model_ids.is_empty() || model_ids.len() > 64 {
        return Err("available_model_ids must contain 1-64 model IDs".to_owned());
    }
    let mut unique = std::collections::HashSet::new();
    let mut normalized = Vec::with_capacity(model_ids.len());
    for model_id in model_ids {
        let model_id = model_id.trim().to_owned();
        if model_id.is_empty()
            || model_id.len() > 128
            || !model_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            return Err(
                "model IDs must be 1-128 characters using letters, digits, -, _, ., or :"
                    .to_owned(),
            );
        }
        if !unique.insert(model_id.clone()) {
            return Err(format!("duplicate model ID: {model_id}"));
        }
        normalized.push(model_id);
    }
    Ok(normalized)
}

#[derive(Clone, Debug)]
pub struct BootstrapConfig {
    pub listen: SocketAddr,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
}

impl BootstrapConfig {
    pub fn load() -> Result<Self> {
        let home =
            dirs::home_dir().context("cannot determine the current user's home directory")?;
        Self::in_data_dir(home.join(".openai-lb"), DEFAULT_LISTEN)
    }

    pub fn in_data_dir(data_dir: PathBuf, listen: SocketAddr) -> Result<Self> {
        fs::create_dir_all(&data_dir)
            .with_context(|| format!("failed to create {}", data_dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            listen,
            database_path: data_dir.join("openai-lb.sqlite3"),
            data_dir,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: SocketAddr,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub setup_complete: bool,
    pub auth_issuer: Option<String>,
    pub auth_audience: Option<String>,
    pub upstream_base: String,
    pub upstream_openai_beta: Option<String>,
    /// Legacy single-UA setting retained for older callers; it is the CodeX fallback.
    pub upstream_user_agent: Option<String>,
    pub upstream_user_agents: UpstreamUserAgents,
    pub experimental_filter_codex_turn_state_312: bool,
    pub image_host_model: String,
    pub available_model_ids: Vec<String>,
    pub allow_all_users_debt: bool,
    pub oauth_authorize_url: String,
    pub oauth_token_url: String,
    pub oauth_redirect_uri: String,
    pub oauth_client_id: String,
    pub response_body_limit: usize,
    pub image_body_limit: usize,
    pub audio_body_limit: usize,
    pub affinity_ttl_seconds: i64,
    pub provider_concurrency_limit: usize,
    pub request_archive_retention_days: i64,
    pub model_price_multiplier_nanos: i64,
    pub midas_api_base: String,
    pub midas_fund_user_id: Option<String>,
    pub midas_fund_api_key: Option<String>,
}

impl Config {
    pub async fn load(bootstrap: BootstrapConfig, pool: &SqlitePool) -> Result<Self> {
        let rows = sqlx::query("SELECT key,value FROM app_meta")
            .fetch_all(pool)
            .await?;
        let values = rows
            .into_iter()
            .map(|row| (row.get::<String, _>(0), row.get::<String, _>(1)))
            .collect::<std::collections::HashMap<_, _>>();
        let value = |key: &str| -> Result<&str> {
            values
                .get(key)
                .map(String::as_str)
                .with_context(|| format!("app_meta is missing {key}"))
        };
        let optional_url = |key: &str| -> Result<Option<String>> {
            Ok(match value(key)?.trim() {
                "" => None,
                configured => Some(configured.trim_end_matches('/').to_owned()),
            })
        };
        let optional_value = |key: &str| -> Result<Option<String>> {
            Ok(match value(key)?.trim() {
                "" => None,
                configured => Some(configured.to_owned()),
            })
        };
        let legacy_user_agent = optional_value("upstream_user_agent")?;
        let upstream_user_agents = UpstreamUserAgents {
            codex_cli_rs: optional_value("upstream_user_agent_codex_cli_rs")?
                .or_else(|| legacy_user_agent.clone()),
            pi: optional_value("upstream_user_agent_pi")?,
            opencode: optional_value("upstream_user_agent_opencode")?,
        };
        Ok(Self {
            listen: bootstrap.listen,
            data_dir: bootstrap.data_dir,
            database_path: bootstrap.database_path,
            setup_complete: value("setup_complete")?.parse()?,
            auth_issuer: optional_url("auth_issuer")?,
            auth_audience: optional_value("auth_audience")?,
            upstream_base: value("upstream_base")?.trim_end_matches('/').to_owned(),
            upstream_openai_beta: match value("upstream_openai_beta")?.trim() {
                "" => None,
                value => Some(value.to_owned()),
            },
            upstream_user_agent: legacy_user_agent,
            upstream_user_agents,
            experimental_filter_codex_turn_state_312: value(
                "experimental_filter_codex_turn_state_312",
            )?
            .parse()?,
            image_host_model: value("image_host_model")?.to_owned(),
            available_model_ids: normalize_available_model_ids(serde_json::from_str(value(
                "available_model_ids",
            )?)?)
            .map_err(anyhow::Error::msg)?,
            allow_all_users_debt: value("allow_all_users_debt")?.parse()?,
            oauth_authorize_url: value("oauth_authorize_url")?.to_owned(),
            oauth_token_url: value("oauth_token_url")?.to_owned(),
            oauth_redirect_uri: value("oauth_redirect_uri")?.to_owned(),
            oauth_client_id: value("oauth_client_id")?.to_owned(),
            response_body_limit: value("response_body_limit")?.parse()?,
            image_body_limit: value("image_body_limit")?.parse()?,
            audio_body_limit: value("audio_body_limit")?.parse()?,
            affinity_ttl_seconds: value("affinity_ttl_seconds")?.parse()?,
            provider_concurrency_limit: value("provider_concurrency_limit")?
                .parse::<std::num::NonZeroUsize>()?
                .get(),
            request_archive_retention_days: value("request_archive_retention_days")?.parse()?,
            model_price_multiplier_nanos: value("model_price_multiplier_nanos")?.parse()?,
            midas_api_base: value("midas_api_base")?.trim_end_matches('/').to_owned(),
            midas_fund_user_id: optional_value("midas_fund_user_id")?,
            midas_fund_api_key: optional_value("midas_fund_api_key")?,
        })
    }

    /// Returns the configured override for a provider identity, with compatibility for tests and
    /// pre-migration callers that still populate the legacy CodeX-only field.
    pub fn upstream_user_agent_for(&self, originator: &str) -> Option<&str> {
        self.upstream_user_agents
            .for_originator(originator)
            .or_else(|| {
                (originator == "codex_cli_rs")
                    .then_some(self.upstream_user_agent.as_deref())
                    .flatten()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn global_debt_permission_defaults_to_disabled() {
        let pool = crate::db::connect_memory().await.unwrap();
        let config = Config::load(
            BootstrapConfig {
                listen: DEFAULT_LISTEN,
                data_dir: std::env::temp_dir(),
                database_path: std::path::PathBuf::from(":memory:"),
            },
            &pool,
        )
        .await
        .unwrap();

        assert!(!config.allow_all_users_debt);
        assert_eq!(config.provider_concurrency_limit, 3);
    }

    #[test]
    fn default_available_models_include_the_latest_generation() {
        let defaults = default_available_model_ids();
        assert!(defaults.contains(&"gpt-6-astra".to_owned()));
        assert!(defaults.contains(&"gpt-5.6-sol".to_owned()));
        assert!(defaults.contains(&"gpt-5.6-terra".to_owned()));
        assert!(defaults.contains(&"gpt-5.6-luna".to_owned()));
    }

    #[test]
    fn available_model_ids_are_trimmed_and_unique() {
        assert_eq!(
            normalize_available_model_ids(vec![" gpt-5.6-luna ".to_owned()]),
            Ok(vec!["gpt-5.6-luna".to_owned()])
        );
        assert!(normalize_available_model_ids(vec!["gpt-5.6-luna".to_owned(); 2]).is_err());
    }

    #[test]
    fn data_directory_is_private_without_creating_a_master_key() {
        let dir = std::env::temp_dir().join(format!("openai-lb-config-{}", uuid::Uuid::new_v4()));
        let first = BootstrapConfig::in_data_dir(dir.clone(), DEFAULT_LISTEN).unwrap();
        let second = BootstrapConfig::in_data_dir(dir.clone(), DEFAULT_LISTEN).unwrap();
        assert_eq!(first.database_path, dir.join("openai-lb.sqlite3"));
        assert_eq!(second.database_path, dir.join("openai-lb.sqlite3"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        assert!(!dir.join("master.key").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
