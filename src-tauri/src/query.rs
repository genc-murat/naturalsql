use mysql_async::{prelude::*, Row, Value};
use serde_json::Value as JsonValue;

use crate::error::AppError;
use crate::db::connection::get_pool;

#[derive(Debug, serde::Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
    pub row_count: usize,
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

    let rows: Vec<Row> = conn.query(sql).await.map_err(|e| {
        AppError::QueryExecution(e.to_string())
    })?;

    if rows.is_empty() {
        return Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            row_count: 0,
        });
    }

    // Extract column names from first row
    let columns: Vec<String> = rows[0]
        .columns()
        .as_ref()
        .iter()
        .map(|col| col.name_str().to_string())
        .collect();

    // Convert rows to JSON
    let mut json_rows = Vec::new();
    for row in rows {
        let mut json_row = Vec::new();
        for (i, _) in columns.iter().enumerate() {
            if let Ok(Some(value)) = row.get_opt::<Value, usize>(i).transpose() {
                json_row.push(mysql_value_to_json(value));
            } else {
                json_row.push(JsonValue::Null);
            }
        }
        json_rows.push(json_row);
    }

    let row_count = json_rows.len();

    Ok(QueryResult {
        columns,
        rows: json_rows,
        row_count,
    })
}
