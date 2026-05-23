use mysql_async::{Pool, Opts, prelude::Queryable};
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::AppError;

static DB_POOL: Lazy<Arc<RwLock<Option<Pool>>>> = Lazy::new(|| {
    Arc::new(RwLock::new(None))
});

pub async fn connect(connection_string: &str) -> Result<(), AppError> {
    let opts = Opts::from_url(connection_string)?;
    
    let pool = Pool::new(opts);
    
    // Test the connection with a simple query
    let mut conn = pool.get_conn().await?;
    let _: Vec<mysql_async::Row> = conn.query("SELECT 1").await?;
    
    // Store the pool
    let mut guard = DB_POOL.write().await;
    *guard = Some(pool);
    
    Ok(())
}

pub async fn disconnect() -> Result<(), AppError> {
    let mut guard = DB_POOL.write().await;
    if let Some(pool) = guard.take() {
        pool.disconnect().await?;
    }
    Ok(())
}

pub async fn is_connected() -> bool {
    DB_POOL.read().await.is_some()
}

pub async fn get_pool() -> Result<Pool, AppError> {
    let guard = DB_POOL.read().await;
    guard.clone().ok_or(AppError::NotConnected)
}

pub async fn get_database_name(connection_string: &str) -> Result<String, AppError> {
    // Parse database name from connection string URL
    // mysql://user:pass@host:port/database
    if let Some(at_pos) = connection_string.find('@') {
        let after_at = &connection_string[at_pos + 1..];
        if let Some(slash_pos) = after_at.find('/') {
            let db_part = &after_at[slash_pos + 1..];
            // Remove query params
            return Ok(db_part.split('?').next().unwrap_or("").to_string());
        }
    }
    Ok(String::new())
}
