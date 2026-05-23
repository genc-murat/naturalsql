use mysql_async::{prelude::*, Row, Value};
use serde_json::Value as JsonValue;
use std::time::Instant;

use crate::error::AppError;
use crate::db::connection::get_pool;

#[derive(Debug, serde::Serialize)]
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
