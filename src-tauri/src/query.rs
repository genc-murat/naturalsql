use mysql_async::{prelude::*, Row, Value};
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::time::Instant;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

use crate::error::AppError;
use crate::db::connection::get_pool;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
    pub row_count: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// For INSERT/UPDATE/DELETE: number of affected rows
    pub affected_rows: Option<u64>,
}

fn mysql_value_to_json(value: Value) -> JsonValue {
    match value {
        Value::NULL => JsonValue::Null,
        Value::Bytes(b) => JsonValue::String(String::from_utf8_lossy(&b).to_string()),
        Value::Int(i) => JsonValue::Number(i.into()),
        Value::UInt(u) => JsonValue::Number(u.into()),
        Value::Float(f) => JsonValue::Number(
            serde_json::Number::from_f64(f as f64).unwrap_or(serde_json::Number::from(0))
        ),
        Value::Double(d) => JsonValue::Number(
            serde_json::Number::from_f64(d).unwrap_or(serde_json::Number::from(0))
        ),
        Value::Date(year, month, day, hour, minute, second, _microsecond) => {
            JsonValue::String(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, minute, second))
        }
        Value::Time(is_neg, days, hours, minutes, seconds, _microsecs) => {
            let sign = if is_neg { "-" } else { "" };
            JsonValue::String(format!("{}{}d {:02}:{:02}:{:02}", sign, days, hours, minutes, seconds))
        }
    }
}

pub async fn execute_query(sql: &str) -> Result<QueryResult, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let start = Instant::now();

    // Try as a SELECT query first
    let result: std::result::Result<Vec<Row>, _> = conn.query(sql).await;

    match result {
        Ok(rows) => {
            if rows.is_empty() {
                return Ok(QueryResult {
                    columns: vec![],
                    rows: vec![],
                    row_count: 0,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    affected_rows: None,
                });
            }

            // Get column definitions from the first row
            let columns_def = rows[0].columns();
            let columns: Vec<String> = columns_def
                .as_ref()
                .iter()
                .map(|col| col.name_str().to_string())
                .collect();

            // Convert rows to JSON
            let mut json_rows = Vec::new();
            for row in rows {
                let mut json_row = Vec::new();
                for i in 0..columns.len() {
                    let val = row.get_opt::<Value, usize>(i);
                    match val {
                        Some(Ok(v)) => json_row.push(mysql_value_to_json(v)),
                        _ => json_row.push(JsonValue::Null),
                    }
                }
                json_rows.push(json_row);
            }

            let row_count = json_rows.len();

            Ok(QueryResult {
                columns,
                rows: json_rows,
                row_count,
                execution_time_ms: start.elapsed().as_millis() as u64,
                affected_rows: None,
            })
        }
        Err(_) => {
            // If SELECT failed, try as a write query (INSERT/UPDATE/DELETE)
            // Reset connection state and try query_drop for affected rows
            let mut conn = pool.get_conn().await?;

            // Get affected rows by checking the query type
            let sql_trimmed = sql.trim().to_uppercase();
            let is_write = sql_trimmed.starts_with("INSERT")
                || sql_trimmed.starts_with("UPDATE")
                || sql_trimmed.starts_with("DELETE")
                || sql_trimmed.starts_with("REPLACE");

            if is_write {
                // Use query_drop and then get affected rows via ROW_COUNT()
                let _: Vec<Row> = conn.query(sql).await.map_err(|e| {
                    AppError::QueryExecution(format!("{} (SQL: {})", e, sql))
                })?;

                let affected: Option<u64> = conn
                    .query::<mysql_async::Row, &str>("SELECT ROW_COUNT()")
                    .await
                    .ok()
                    .and_then(|rows| rows.into_iter().next())
                    .and_then(|r: mysql_async::Row| r.get(0));

                Ok(QueryResult {
                    columns: vec![],
                    rows: vec![],
                    row_count: 0,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    affected_rows: affected,
                })
            } else {
                // Not a write query, return the original SELECT error
                let mut conn = pool.get_conn().await?;
                let _: Vec<Row> = conn.query(sql).await.map_err(|e| {
                    AppError::QueryExecution(format!("{} (SQL: {})", e, sql))
                })?;

                // Unreachable, but satisfy the compiler
                Ok(QueryResult {
                    columns: vec![],
                    rows: vec![],
                    row_count: 0,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    affected_rows: None,
                })
            }
        }
    }
}

/// Run EXPLAIN on a query
pub async fn explain_query(sql: &str) -> Result<QueryResult, AppError> {
    let explain_sql = format!("EXPLAIN {}", sql);
    execute_query(&explain_sql).await
}

/// Run EXPLAIN FORMAT=JSON on a query
pub async fn explain_query_json(sql: &str) -> Result<String, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let explain_sql = format!("EXPLAIN FORMAT=JSON {}", sql);
    let result: Option<String> = conn
        .query_first(explain_sql)
        .await
        .map_err(|e| AppError::QueryExecution(format!("EXPLAIN failed: {}", e)))?;

    match result {
        Some(json_str) => Ok(json_str),
        None => Err(AppError::QueryExecution("EXPLAIN returned no result".to_string())),
    }
}

// ========================
// Streaming Query Support
// ========================

use std::sync::Mutex;
use std::collections::HashMap;
use once_cell::sync::Lazy;

static ACTIVE_QUERIES: Lazy<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize)]
pub struct StreamBatchPayload {
    pub query_id: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
    pub total_so_far: usize,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamErrorPayload {
    pub query_id: String,
    pub error: String,
}

pub async fn execute_query_streaming(
    app_handle: &tauri::AppHandle,
    sql: &str,
    query_id: &str,
) -> Result<(), AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;
    let _start = Instant::now();

    // Try SELECT first
    let mut query_result = match conn.query_iter(sql).await {
        Ok(qr) => qr,
        Err(e) => {
            // Fallback: try as write query
            let mut conn = pool.get_conn().await?;
            let sql_trimmed = sql.trim().to_uppercase();
            let is_write = sql_trimmed.starts_with("INSERT")
                || sql_trimmed.starts_with("UPDATE")
                || sql_trimmed.starts_with("DELETE")
                || sql_trimmed.starts_with("REPLACE");

            if is_write {
                let _: Vec<Row> = conn.query(sql).await.map_err(|e2| {
                    AppError::QueryExecution(format!("{} (SQL: {})", e2, sql))
                })?;
                let affected: Option<u64> = conn
                    .query::<mysql_async::Row, &str>("SELECT ROW_COUNT()")
                    .await
                    .ok()
                    .and_then(|rows| rows.into_iter().next())
                    .and_then(|r: mysql_async::Row| r.get(0));
                app_handle.emit("sql-stream-done", StreamBatchPayload {
                    query_id: query_id.to_string(),
                    columns: vec![],
                    rows: vec![],
                    total_so_far: affected.unwrap_or(0) as usize,
                    done: true,
                }).map_err(|e| AppError::Streaming(e.to_string()))?;
                return Ok(());
            }
            return Err(AppError::QueryExecution(format!("{} (SQL: {})", e, sql)));
        }
    };

    // Get cancel flag
    let cancel_flag = {
        let map = ACTIVE_QUERIES.lock().unwrap();
        map.get(query_id).cloned()
    };

    // Get columns
    let columns: Vec<String> = if let Some(cols) = query_result.columns() {
        cols.iter().map(|c| c.name_str().to_string()).collect()
    } else {
        Vec::new()
    };

    let mut all_rows: Vec<Vec<JsonValue>> = Vec::new();
    let mut batch_rows: Vec<Vec<JsonValue>> = Vec::new();
    let batch_size = 200;

    while let Ok(Some(row)) = query_result.next().await {
        // Check cancel
        if let Some(ref flag) = cancel_flag {
            if flag.load(Ordering::SeqCst) {
                return Err(AppError::QueryCancelled);
            }
        }

        let mut json_row = Vec::new();
        for i in 0..columns.len() {
            let val = row.get_opt::<Value, usize>(i);
            json_row.push(match val {
                Some(Ok(v)) => mysql_value_to_json(v),
                _ => JsonValue::Null,
            });
        }
        all_rows.push(json_row.clone());
        batch_rows.push(json_row);

        if batch_rows.len() >= batch_size {
            app_handle.emit("sql-stream-batch", StreamBatchPayload {
                query_id: query_id.to_string(),
                columns: columns.clone(),
                rows: batch_rows.drain(..).collect(),
                total_so_far: all_rows.len(),
                done: false,
            }).map_err(|e| AppError::Streaming(e.to_string()))?;
        }
    }

    // Send final batch
    if !batch_rows.is_empty() {
        app_handle.emit("sql-stream-batch", StreamBatchPayload {
            query_id: query_id.to_string(),
            columns: columns.clone(),
            rows: batch_rows,
            total_so_far: all_rows.len(),
            done: false,
        }).map_err(|e| AppError::Streaming(e.to_string()))?;
    }

    // Send done
    app_handle.emit("sql-stream-done", StreamBatchPayload {
        query_id: query_id.to_string(),
        columns,
        rows: vec![],
        total_so_far: all_rows.len(),
        done: true,
    }).map_err(|e| AppError::Streaming(e.to_string()))?;

    Ok(())
}

/// Register an active query for cancellation
pub fn register_query(query_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut map = ACTIVE_QUERIES.lock().unwrap();
    map.insert(query_id.to_string(), flag.clone());
    flag
}

/// Cancel an active query
pub fn cancel_query(query_id: &str) -> bool {
    let map = ACTIVE_QUERIES.lock().unwrap();
    if let Some(flag) = map.get(query_id) {
        flag.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// Clean up a finished/cancelled query
pub fn unregister_query(query_id: &str) {
    let mut map = ACTIVE_QUERIES.lock().unwrap();
    map.remove(query_id);
}
