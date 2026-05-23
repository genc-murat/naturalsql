use serde::{Deserialize, Serialize};

use crate::config::{self, AppConfig, LlmConfig};
use crate::error::AppError;
use crate::db::{connection, schema};
use crate::llm;
use crate::query;

#[derive(Debug, Serialize, Deserialize)]
pub struct NlToSqlRequest {
    pub natural_language: String,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct LlmConfigResponse {
    pub url: String,
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateLlmConfigRequest {
    pub url: String,
    pub model: String,
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
    if connection_string.trim().is_empty() {
        return Err(AppError::QueryExecution(
            "Connection string is empty. Please connect first.".to_string()
        ));
    }
    let db_name = connection::get_database_name(&connection_string).await?;
    if db_name.is_empty() {
        return Err(AppError::QueryExecution(
            "Could not parse database name from connection string. Expected format: mysql://user:pass@host:port/database".to_string()
        ));
    }
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
    ).await?;

    Ok(SqlResponse { sql })
}

#[tauri::command]
pub async fn execute_sql(request: ExecuteRequest) -> Result<query::QueryResult, AppError> {
    query::execute_query(&request.sql).await
}

#[tauri::command]
pub async fn get_llm_config() -> Result<LlmConfigResponse, String> {
    let config = config::get_config().await;
    Ok(LlmConfigResponse {
        url: config.llm.url,
        model: config.llm.model,
    })
}

#[tauri::command]
pub async fn update_llm_config(request: UpdateLlmConfigRequest) -> Result<LlmConfigResponse, String> {
    if request.url.trim().is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    if request.model.trim().is_empty() {
        return Err("Model cannot be empty".to_string());
    }

    let new_config = AppConfig {
        llm: LlmConfig {
            url: request.url.clone(),
            model: request.model.clone(),
        },
    };

    config::save_config(&new_config)?;

    Ok(LlmConfigResponse {
        url: request.url,
        model: request.model,
    })
}
