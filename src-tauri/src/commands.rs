use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::db::{connection, schema};
use crate::llm;
use crate::query;

#[derive(Debug, Serialize, Deserialize)]
pub struct NlToSqlRequest {
    pub natural_language: String,
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteRequest {
    pub sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SqlResponse {
    pub sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub connected: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaResponse {
    pub schema: Option<schema::Schema>,
}

#[tauri::command]
pub async fn connect_db(connection_string: String) -> Result<ConnectionStatus, AppError> {
    connection::connect(&connection_string).await?;
    Ok(ConnectionStatus { connected: true })
}

#[tauri::command]
pub async fn disconnect_db() -> Result<ConnectionStatus, AppError> {
    connection::disconnect().await?;
    Ok(ConnectionStatus { connected: false })
}

#[tauri::command]
pub async fn get_connection_status() -> ConnectionStatus {
    let connected = connection::is_connected().await;
    ConnectionStatus { connected }
}

#[tauri::command]
pub async fn cache_schema(connection_string: String) -> Result<SchemaResponse, AppError> {
    let db_name = connection::get_database_name(&connection_string).await?;
    let schema = schema::introspect_schema(&db_name).await?;
    schema::cache_schema(&schema)?;
    Ok(SchemaResponse { schema: Some(schema) })
}

#[tauri::command]
pub async fn get_cached_schema() -> Result<SchemaResponse, AppError> {
    let cached = schema::load_cached_schema()?;
    Ok(SchemaResponse { schema: cached })
}

#[tauri::command]
pub async fn nl_to_sql(request: NlToSqlRequest) -> Result<SqlResponse, AppError> {
    let schema = schema::load_cached_schema()?.ok_or(AppError::SchemaNotCached)?;
    let schema_context = schema::format_schema_for_prompt(&schema);
    
    let sql = llm::natural_language_to_sql(
        &request.natural_language,
        &schema_context,
        request.model.as_deref(),
    ).await?;
    
    Ok(SqlResponse { sql })
}

#[tauri::command]
pub async fn execute_sql(request: ExecuteRequest) -> Result<query::QueryResult, AppError> {
    query::execute_query(&request.sql).await
}
