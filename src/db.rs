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
            std::env::temp_dir().join(format!("deepseek-lb-db-{}.sqlite3", uuid::Uuid::new_v4()));
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
    async fn migrations_create_the_deepseek_lb_schema_with_seeded_settings() {
        let pool = connect_memory().await.unwrap();
        for table in [
            "app_meta",
            "users",
            "consumers",
            "providers",
            "affinities",
            "api_calls",
            "request_archives",
            "admin_audit",
        ] {
            let count: i64 = sqlx::query_scalar(&format!(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
            ))
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(count, 1, "missing table {table}");
        }
        let upstream_base: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='upstream_base'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(upstream_base, "https://api.deepseek.com");
        let models: String =
            sqlx::query_scalar("SELECT value FROM app_meta WHERE key='available_model_ids'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(models, r#"["deepseek-flash","deepseek-v4-pro"]"#);
        // A provider is one API key and nothing else.
        let columns: Vec<String> = sqlx::query("SELECT name FROM pragma_table_info('providers')")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.get(0))
            .collect();
        assert!(columns.contains(&"api_key".to_owned()));
        for removed in ["access_token", "refresh_token", "account_id", "originator"] {
            assert!(
                !columns.contains(&removed.to_owned()),
                "providers still has {removed}"
            );
        }
    }

    #[tokio::test]
    async fn migrations_keep_a_single_root_user() {
        let pool = connect_memory().await.unwrap();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('root','root',1)")
            .execute(&pool)
            .await
            .unwrap();
        let second_root =
            sqlx::query("INSERT INTO users(id,role,created_at) VALUES('root-2','root',2)")
                .execute(&pool)
                .await;
        assert!(second_root.is_err());
    }

    #[tokio::test]
    async fn api_calls_record_the_peak_tariff() {
        let pool = connect_memory().await.unwrap();
        sqlx::query("INSERT INTO users(id,role,created_at) VALUES('user','user',0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO consumers(id,user_id,name,prefix,secret_hash,created_at) VALUES('consumer','user','consumer','sk-test','hash',0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO api_calls(id,request_id,consumer_id,user_id,method,path,status,latency_ms,peak,created_at) VALUES('call','request','consumer','user','POST','/v1/chat/completions',200,1,1,1)")
            .execute(&pool)
            .await
            .unwrap();
        let peak: i64 = sqlx::query_scalar("SELECT peak FROM api_calls WHERE id='call'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(peak, 1);
    }
}
