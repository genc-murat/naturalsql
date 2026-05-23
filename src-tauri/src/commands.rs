use serde::{Deserialize, Serialize};

use crate::config::{self, ConnectionProfile, LlmConfig};
use crate::error::AppError;
use crate::db::{connection, schema};
use crate::llm;
use crate::query;

#[derive(Debug, Serialize, Deserialize)]
pub struct NlToSqlRequest {
    pub natural_language: String,
    pub database: String,
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
    let default_db = connection::get_connection_database(&connection_string);
    connection::connect(&connection_string).await?;
    // Return the default database name from the connection string (if any)
    // The frontend can use this to auto-select
    let _ = default_db; // stored for now, frontend will parse
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
pub async fn list_databases() -> Result<Vec<String>, AppError> {
    connection::list_databases().await
}

#[tauri::command]
pub async fn cache_schema(database: String) -> Result<SchemaResponse, AppError> {
    if database.trim().is_empty() {
        return Err(AppError::QueryExecution(
            "Database name is required.".to_string()
        ));
    }
    let schema = schema::introspect_schema(&database).await?;
    schema::cache_schema(&schema)?;
    Ok(SchemaResponse { schema: Some(schema) })
}

#[tauri::command]
pub async fn get_cached_schema(database: String) -> Result<SchemaResponse, AppError> {
    let cached = schema::load_cached_schema(&database)?;
    Ok(SchemaResponse { schema: cached })
}

#[tauri::command]
pub async fn list_cached_databases() -> Result<Vec<String>, AppError> {
    schema::list_cached_databases()
}

#[tauri::command]
pub async fn remove_cached_schema(database: String) -> Result<(), AppError> {
    schema::remove_cached_schema(&database)
}

#[tauri::command]
pub async fn nl_to_sql(request: NlToSqlRequest) -> Result<SqlResponse, AppError> {
    // Load ALL cached schemas for cross-database query support
    let all_schemas = schema::load_all_cached_schemas()?;
    if all_schemas.is_empty() {
        return Err(AppError::SchemaNotCached);
    }

    // Format all schemas with database.table notation
    let schema_context = schema::format_all_schemas_for_prompt(&all_schemas);

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
pub async fn explain_sql(request: ExecuteRequest) -> Result<query::QueryResult, AppError> {
    query::explain_query(&request.sql).await
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

    let mut config = config::get_config().await;
    config.llm = LlmConfig {
        url: request.url.clone(),
        model: request.model.clone(),
    };

    config::save_config(&config)?;

    Ok(LlmConfigResponse {
        url: request.url,
        model: request.model,
    })
}

// Connection Profile Commands

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionProfileResponse {
    pub name: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl From<&ConnectionProfile> for ConnectionProfileResponse {
    fn from(p: &ConnectionProfile) -> Self {
        Self {
            name: p.name.clone(),
            host: p.host.clone(),
            port: p.port.clone(),
            user: p.user.clone(),
            password: p.password.clone(),
            database: p.database.clone(),
        }
    }
}

#[tauri::command]
pub async fn list_connections() -> Vec<ConnectionProfileResponse> {
    let profiles = config::get_connections().await;
    profiles.iter().map(|p| p.into()).collect()
}

#[tauri::command]
pub async fn save_connection_profile(profile: ConnectionProfileResponse) -> Result<(), String> {
    if profile.name.trim().is_empty() {
        return Err("Profile name cannot be empty".to_string());
    }
    if profile.host.trim().is_empty() {
        return Err("Host cannot be empty".to_string());
    }

    config::save_connection(ConnectionProfile {
        name: profile.name,
        host: profile.host,
        port: profile.port,
        user: profile.user,
        password: profile.password,
        database: profile.database,
    }).await
}

#[tauri::command]
pub async fn delete_connection_profile(name: String) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Profile name cannot be empty".to_string());
    }
    config::delete_connection(name).await
}
