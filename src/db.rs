use std::{path::Path, time::Duration};

use anyhow::{Result, ensure};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect(path: &Path) -> Result<SqlitePool> {
    let options = options(path, true)?;
    let pool = connect_with_options(options, 4).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(pool)
}

#[cfg(test)]
pub async fn connect_memory() -> Result<SqlitePool> {
    let options = "sqlite::memory:"
        .parse::<SqliteConnectOptions>()?
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    connect_with_options(options, 1).await
}

#[cfg(test)]
pub async fn connect_test_file(path: &Path) -> Result<SqlitePool> {
    let options = options(path, true)?;
    connect_with_options(options, 4).await
}

fn options(path: &Path, create: bool) -> Result<SqliteConnectOptions> {
    Ok(SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .synchronous(SqliteSynchronous::Normal))
}

async fn connect_with_options(
    options: SqliteConnectOptions,
    max_connections: u32,
) -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await?;
    let mut connection = pool.acquire().await?;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *connection)
        .await?;
    MIGRATOR.run(&mut *connection).await?;
    let foreign_key_violations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&mut *connection)
            .await?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *connection)
        .await?;
    ensure!(
        foreign_key_violations == 0,
        "database migration left {foreign_key_violations} foreign key violation(s)"
    );
    drop(connection);
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use sqlx::Row;

    use super::*;

    #[tokio::test]
    async fn every_pooled_connection_has_required_pragmas() {
        let path =
            std::env::temp_dir().join(format!("openai-lb-db-{}.sqlite3", uuid::Uuid::new_v4()));
        let pool = connect_test_file(&path).await.unwrap();
        let mut connections = Vec::new();
        for _ in 0..4 {
            connections.push(pool.acquire().await.unwrap());
        }
        for connection in &mut connections {
            let row = sqlx::query("SELECT (SELECT * FROM pragma_foreign_keys()), (SELECT timeout FROM pragma_busy_timeout()), (SELECT * FROM pragma_synchronous())")
                .fetch_one(&mut **connection)
                .await
                .unwrap();
            assert_eq!(row.get::<i64, _>(0), 1);
            assert_eq!(row.get::<i64, _>(1), 5000);
            assert_eq!(row.get::<i64, _>(2), 1);
        }
        drop(connections);
        pool.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[tokio::test]
    async fn versioned_migrations_upgrade_the_legacy_user_role_schema() {
        let path = std::env::temp_dir().join(format!(
            "openai-lb-upgrade-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let legacy = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        let schema = [
            "CREATE TABLE users (id TEXT PRIMARY KEY,email TEXT,display_name TEXT,role TEXT NOT NULL CHECK(role IN ('admin','user')),created_at INTEGER NOT NULL)",
            "CREATE TABLE api_keys (id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,name TEXT NOT NULL,prefix TEXT NOT NULL,secret_hash TEXT NOT NULL UNIQUE,created_at INTEGER NOT NULL,last_used_at INTEGER,revoked_at INTEGER)",
            "CREATE TABLE channels (id TEXT PRIMARY KEY,name TEXT NOT NULL,account_id TEXT NOT NULL,access_enc TEXT NOT NULL,refresh_enc TEXT NOT NULL,expires_at INTEGER,status TEXT NOT NULL DEFAULT 'active',manual_disabled INTEGER NOT NULL DEFAULT 0,cooldown_until INTEGER,rate_limit_json TEXT,last_error TEXT,last_used_at INTEGER,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL)",
            "CREATE TABLE oauth_flows (state_hash TEXT PRIMARY KEY,verifier_enc TEXT NOT NULL,created_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,expires_at INTEGER NOT NULL)",
            "CREATE TABLE affinities (affinity_hash TEXT PRIMARY KEY,channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,expires_at INTEGER NOT NULL,updated_at INTEGER NOT NULL)",
            "CREATE TABLE api_calls (id TEXT PRIMARY KEY,request_id TEXT NOT NULL,api_key_id TEXT NOT NULL REFERENCES api_keys(id),user_id TEXT NOT NULL REFERENCES users(id),channel_id TEXT REFERENCES channels(id),method TEXT NOT NULL,path TEXT NOT NULL,model TEXT,status INTEGER NOT NULL,latency_ms INTEGER NOT NULL,input_tokens INTEGER NOT NULL DEFAULT 0,output_tokens INTEGER NOT NULL DEFAULT 0,cached_tokens INTEGER NOT NULL DEFAULT 0,error TEXT,client_ip TEXT,created_at INTEGER NOT NULL)",
            "CREATE TABLE admin_audit (id TEXT PRIMARY KEY,admin_user_id TEXT NOT NULL REFERENCES users(id),action TEXT NOT NULL,target_id TEXT,client_ip TEXT,created_at INTEGER NOT NULL)",
        ];
        for statement in schema {
            sqlx::query(statement).execute(&legacy).await.unwrap();
        }
        let fixtures = [
            "INSERT INTO users(id,email,role,created_at) VALUES('legacy-admin','admin@example.com','admin',1)",
            "INSERT INTO api_keys(id,user_id,name,prefix,secret_hash,created_at,revoked_at) VALUES('key','legacy-admin','legacy','sk-old','hash',2,3)",
            "INSERT INTO oauth_flows(state_hash,verifier_enc,created_by,expires_at) VALUES('flow','verifier','legacy-admin',100)",
            "INSERT INTO admin_audit(id,admin_user_id,action,created_at) VALUES('audit','legacy-admin','legacy.action',6)",
        ];
        for statement in fixtures {
            sqlx::query(statement).execute(&legacy).await.unwrap();
        }
        sqlx::query("INSERT INTO channels(id,name,account_id,access_enc,refresh_enc,created_at,updated_at) VALUES('channel','legacy','account',?,?,3,3)")
            .bind("access")
            .bind("refresh")
            .execute(&legacy)
            .await
            .unwrap();
        for statement in [
            "INSERT INTO affinities(affinity_hash,channel_id,expires_at,updated_at) VALUES('affinity','channel',100,4)",
            "INSERT INTO api_calls(id,request_id,api_key_id,user_id,channel_id,method,path,status,latency_ms,created_at) VALUES('call','request','key','legacy-admin','channel','POST','/v1/responses',200,5,5)",
        ] {
            sqlx::query(statement).execute(&legacy).await.unwrap();
        }
        legacy.close().await;

        let pool = connect_test_file(&path).await.unwrap();
        let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id='legacy-admin'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(role, "admin");
        let legacy_consumer_deleted: i64 =
            sqlx::query_scalar("SELECT is_deleted FROM consumers WHERE id='key'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(legacy_consumer_deleted, 1);
        let allow_debt: i64 =
            sqlx::query_scalar("SELECT allow_debt FROM users WHERE id='legacy-admin'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(allow_debt, 1);
        let thread_id: Option<String> =
            sqlx::query_scalar("SELECT thread_id FROM api_calls WHERE id='call'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(thread_id, None);
        let session_id: Option<String> =
            sqlx::query_scalar("SELECT session_id FROM api_calls WHERE id='call'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(session_id, None);
        let first_byte_latency_ms: Option<i64> =
            sqlx::query_scalar("SELECT first_byte_latency_ms FROM api_calls WHERE id='call'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(first_byte_latency_ms, None);
        let consumed: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT u.official_consumed_usd_nanos,u.consumed_usd_nanos,c.official_consumed_usd_before_nanos,c.official_consumed_usd_after_nanos,c.actual_consumed_usd_after_nanos FROM users u JOIN api_calls c ON c.user_id=u.id WHERE u.id='legacy-admin'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(consumed, (0, 0, 0, 0, 0));
        for table in [
            "users",
            "consumers",
            "providers",
            "affinities",
            "api_calls",
            "admin_audit",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 1, "legacy row was lost from {table}");
        }
        let foreign_key_violations: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(foreign_key_violations, 0);
        let tokens: (String, String) =
            sqlx::query_as("SELECT access_token,refresh_token FROM providers WHERE id='channel'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tokens, ("access".to_owned(), "refresh".to_owned()));
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('root','root',2)")
            .execute(&pool)
            .await
            .unwrap();
        let second_root =
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES('root-2','root',3)")
                .execute(&pool)
                .await;
        assert!(second_root.is_err());
        pool.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[tokio::test]
    async fn model_price_multiplier_migration_rebuilds_historical_costs() {
        let pool = connect_memory().await.unwrap();
        for statement in [
            "ALTER TABLE api_calls DROP COLUMN official_consumed_usd_after_nanos",
            "ALTER TABLE api_calls DROP COLUMN official_consumed_usd_before_nanos",
            "ALTER TABLE api_calls DROP COLUMN price_multiplier_nanos",
            "ALTER TABLE api_calls DROP COLUMN actual_cost_usd_nanos",
            "ALTER TABLE api_calls DROP COLUMN official_cost_usd_nanos",
            "ALTER TABLE api_calls RENAME COLUMN actual_consumed_usd_before_nanos TO consumed_usd_before_nanos",
            "ALTER TABLE api_calls RENAME COLUMN actual_consumed_usd_after_nanos TO consumed_usd_after_nanos",
            "ALTER TABLE users DROP COLUMN official_consumed_usd_nanos",
            "ALTER TABLE api_calls ADD COLUMN cost_usd_nanos INTEGER",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        sqlx::query("DELETE FROM app_meta WHERE key='model_price_multiplier_nanos'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('user','user',0)")
            .execute(&pool)
            .await
            .unwrap();
        let legacy_usage_table = ["credit", "ledger", "entries"].join("_");
        sqlx::query(&format!(
            "CREATE TABLE {legacy_usage_table}(id TEXT PRIMARY KEY,user_id TEXT NOT NULL,entry_kind TEXT NOT NULL,amount_usd_nanos INTEGER NOT NULL,payment_order_id TEXT,api_call_id TEXT UNIQUE,created_at INTEGER NOT NULL)",
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('consumer','user','consumer','sk-test','hash',0)")
            .execute(&pool)
            .await
            .unwrap();
        for (id, cost_usd_nanos, created_at) in [("first", 11_i64, 1_i64), ("second", 19, 2)] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,cost_usd_nanos,created_at) VALUES(?,?, 'consumer','user','POST','/v1/responses',200,1,?,?)")
                .bind(id)
                .bind(id)
                .bind(cost_usd_nanos)
                .bind(created_at)
                .execute(&pool)
                .await
                .unwrap();
        }

        sqlx::raw_sql(include_str!(
            "../migrations/20260815000034_model_price_multiplier.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        type CostSnapshot = (String, i64, i64, i64, i64, i64, i64, i64);
        let calls: Vec<CostSnapshot> = sqlx::query_as(
            "SELECT id,official_cost_usd_nanos,actual_cost_usd_nanos,price_multiplier_nanos,official_consumed_usd_before_nanos,official_consumed_usd_after_nanos,actual_consumed_usd_before_nanos,actual_consumed_usd_after_nanos FROM api_calls ORDER BY created_at",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            calls,
            vec![
                ("first".to_owned(), 11, 1, 100_000_000, 0, 11, 0, 1),
                ("second".to_owned(), 19, 1, 100_000_000, 11, 30, 1, 2),
            ]
        );
        let user: (i64, i64) = sqlx::query_as(
            "SELECT official_consumed_usd_nanos,consumed_usd_nanos FROM users WHERE id='user'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(user, (30, 2));
        let multiplier: String = sqlx::query_scalar(
            "SELECT value FROM app_meta WHERE key='model_price_multiplier_nanos'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(multiplier, "100000000");
    }

    #[tokio::test]
    async fn gpt_6_astra_migration_extends_only_the_former_default_allowlist() {
        let pool = connect_memory().await.unwrap();
        let former_default = r#"["gpt-5.6-sol","gpt-5.6-terra","gpt-5.6-luna","gpt-5.4","gpt-5.3-codex","gpt-5.4-mini","gpt-4o-transcribe","gpt-image-1","gpt-image-1.5","gpt-image-2"]"#;
        let current_default = r#"["gpt-6-astra","gpt-5.6-sol","gpt-5.6-terra","gpt-5.6-luna","gpt-5.4","gpt-5.3-codex","gpt-5.4-mini","gpt-4o-transcribe","gpt-image-1","gpt-image-1.5","gpt-image-2"]"#;

        sqlx::query("UPDATE app_meta SET value=? WHERE key='available_model_ids'")
            .bind(former_default)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../migrations/20260905000039_gpt_6_astra_available_model.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let value: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='available_model_ids'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(value, current_default);

        sqlx::query(
            "UPDATE app_meta SET value='[\"gpt-5.6-luna\"]' WHERE key='available_model_ids'",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../migrations/20260905000039_gpt_6_astra_available_model.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let value: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='available_model_ids'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(value, r#"["gpt-5.6-luna"]"#);
    }

    #[tokio::test]
    async fn provider_value_migration_rebuilds_provider_and_owner_totals() {
        let pool = connect_memory().await.unwrap();
        for statement in [
            "ALTER TABLE api_calls DROP COLUMN actual_provided_usd_after_nanos",
            "ALTER TABLE api_calls DROP COLUMN actual_provided_usd_before_nanos",
            "ALTER TABLE api_calls DROP COLUMN official_provided_usd_after_nanos",
            "ALTER TABLE api_calls DROP COLUMN official_provided_usd_before_nanos",
            "ALTER TABLE providers DROP COLUMN actual_provided_usd_nanos",
            "ALTER TABLE providers DROP COLUMN official_provided_usd_nanos",
            "ALTER TABLE users DROP COLUMN provided_usd_nanos",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        for user_id in ["caller", "owner"] {
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES(?,'user',0)")
                .bind(user_id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('consumer','caller','consumer','sk-test','hash',0)")
            .execute(&pool)
            .await
            .unwrap();
        for provider_id in ["provider-a", "provider-b"] {
            sqlx::query("INSERT INTO providers(id,name,account_id,access_token,refresh_token,owner_id,created_at,updated_at) VALUES(?,?,'account','access','refresh','owner',0,0)")
                .bind(provider_id)
                .bind(provider_id)
                .execute(&pool)
                .await
                .unwrap();
        }
        for (id, provider_id, official_cost_usd_nanos, actual_cost_usd_nanos, created_at) in [
            ("a-first", "provider-a", 10_i64, 1_i64, 1_i64),
            ("a-second", "provider-a", 20, 2, 2),
            ("b-first", "provider-b", 50, 5, 3),
        ] {
            sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,provider_id,method,path,status,latency_ms,official_cost_usd_nanos,actual_cost_usd_nanos,created_at) VALUES(?,?, 'consumer','caller',?,'POST','/v1/responses',200,1,?,?,?)")
                .bind(id)
                .bind(id)
                .bind(provider_id)
                .bind(official_cost_usd_nanos)
                .bind(actual_cost_usd_nanos)
                .bind(created_at)
                .execute(&pool)
                .await
                .unwrap();
        }

        sqlx::raw_sql(include_str!(
            "../migrations/20260817000036_provider_provided_value.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let calls: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT id,official_provided_usd_before_nanos,official_provided_usd_after_nanos,actual_provided_usd_before_nanos,actual_provided_usd_after_nanos FROM api_calls ORDER BY created_at",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            calls,
            vec![
                ("a-first".to_owned(), 0, 10, 0, 1),
                ("a-second".to_owned(), 10, 30, 1, 3),
                ("b-first".to_owned(), 0, 50, 0, 5),
            ]
        );
        let providers: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT id,official_provided_usd_nanos,actual_provided_usd_nanos FROM providers ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            providers,
            vec![
                ("provider-a".to_owned(), 30, 3),
                ("provider-b".to_owned(), 50, 5),
            ]
        );
        let owners: Vec<(String, i64)> =
            sqlx::query_as("SELECT id,provided_usd_nanos FROM users ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            owners,
            vec![("caller".to_owned(), 0), ("owner".to_owned(), 8)]
        );
    }

    #[tokio::test]
    async fn latest_migration_removes_legacy_midas_topup_history() {
        let pool = connect_memory().await.unwrap();
        let legacy_table_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='midas_topup_baselines'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let legacy_setting_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM app_meta WHERE key IN ('midas_agreement_id','midas_agreement_api_key')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(legacy_table_count, 0);
        assert_eq!(legacy_setting_count, 0);
    }
    #[tokio::test]
    async fn originator_fallback_migration_preserves_existing_rows_and_defaults_off() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE providers(id TEXT PRIMARY KEY,originator TEXT); INSERT INTO providers VALUES('old','pi'); CREATE TABLE api_calls(id TEXT PRIMARY KEY); INSERT INTO api_calls VALUES('old-call');")
            .execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!(
            "../migrations/20260919000056_allow_other_originator.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let row: (String, bool) = sqlx::query_as(
            "SELECT originator,allow_other_originator FROM providers WHERE id='old'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row, ("pi".to_owned(), false));
        let reason: Option<String> = sqlx::query_scalar(
            "SELECT originator_fallback_reason FROM api_calls WHERE id='old-call'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(reason, None);
        assert!(
            sqlx::query("UPDATE providers SET allow_other_originator=2")
                .execute(&pool)
                .await
                .is_err()
        );
    }
}
