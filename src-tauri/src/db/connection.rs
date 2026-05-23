use mysql_async::{Pool, Opts, prelude::Queryable};
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::AppError;

#[allow(dead_code)]
struct ConnectionState {
    pool: Pool,
    database: Option<String>,
}

static DB_STATE: Lazy<Arc<RwLock<Option<ConnectionState>>>> = Lazy::new(|| {
    Arc::new(RwLock::new(None))
});

pub async fn connect(connection_string: &str) -> Result<(), AppError> {
    let opts = Opts::from_url(connection_string)?;
    let database = get_connection_database(connection_string);

    let pool = Pool::new(opts);

    // Test the connection
    let mut conn = pool.get_conn().await?;
    let _: Vec<mysql_async::Row> = conn.query("SELECT 1").await?;

    // If database is specified in connection string, USE it
    if let Some(ref db) = database {
        conn.query_drop(&format!("USE `{}`", db)).await.map_err(|e| {
            AppError::QueryExecution(format!("Failed to select database '{}': {}", db, e))
        })?;
    }

    // Store the state
    let mut guard = DB_STATE.write().await;
    *guard = Some(ConnectionState {
        pool,
        database,
    });

    Ok(())
}

pub async fn disconnect() -> Result<(), AppError> {
    let mut guard = DB_STATE.write().await;
    if let Some(state) = guard.take() {
        state.pool.disconnect().await?;
    }
    Ok(())
}

pub async fn is_connected() -> bool {
    DB_STATE.read().await.is_some()
}

pub async fn get_pool() -> Result<Pool, AppError> {
    let guard = DB_STATE.read().await;
    guard.as_ref().map(|s| s.pool.clone()).ok_or(AppError::NotConnected)
}

#[allow(dead_code)]
pub async fn get_current_database() -> Option<String> {
    let guard = DB_STATE.read().await;
    guard.as_ref().and_then(|s| s.database.clone())
}

pub async fn list_databases() -> Result<Vec<String>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let databases: Vec<String> = conn
        .query_map(
            "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA
             WHERE SCHEMA_NAME NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys')
             ORDER BY SCHEMA_NAME",
            |row: mysql_async::Row| {
                row.get(0).unwrap_or_default()
            },
        )
        .await?;

    Ok(databases)
}

pub fn get_connection_database(connection_string: &str) -> Option<String> {
    if let Some(at_pos) = connection_string.find('@') {
        let after_at = &connection_string[at_pos + 1..];
        if let Some(slash_pos) = after_at.find('/') {
            let db_part = &after_at[slash_pos + 1..];
            let db = db_part.split('?').next().unwrap_or("");
            if !db.is_empty() {
                return Some(db.to_string());
            }
        }
    }
    None
}
