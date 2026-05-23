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

// SQL → Natural Language explanation via LLM

// SQL Error → AI Fix

#[derive(Debug, Serialize, Deserialize)]
pub struct FixSqlRequest {
    pub sql: String,
    pub error: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FixSqlResponse {
    pub fixed_sql: String,
    pub explanation: String,
}

#[tauri::command]
pub async fn fix_sql(request: FixSqlRequest) -> Result<FixSqlResponse, AppError> {
    if request.sql.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    let prompt = format!(
        "This MySQL query produced the following error. Fix the query and explain what was wrong.\n\
         Return ONLY the fixed SQL query on the first line, then a blank line, then a brief explanation.\n\
         Do not include markdown code blocks or backticks.\n\n\
         Query: {}\n\
         Error: {}\n\n\
         Fixed SQL:\n",
        request.sql, request.error
    );

    let url = config::get_llm_url().await;
    let model = config::get_llm_model().await;

    let response = reqwest::Client::new()
        .post(&format!("{}/api/generate", url.trim_end_matches('/')))
        .json(&serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(AppError::QueryExecution(
            format!("Ollama returned status: {}", response.status())
        ));
    }

    #[derive(Deserialize)]
    struct OllamaResp {
        response: String,
    }

    let body: OllamaResp = response.json().await?;
    let text = body.response.trim().to_string();

    // Split into SQL (first line(s)) and explanation (after blank line)
    let parts: Vec<&str> = text.split("\n\n").collect();
    let fixed_sql = parts.first()
        .unwrap_or(&"")
        .trim()
        .trim_start_matches("```sql")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();
    let explanation = parts.get(1)
        .unwrap_or(&"")
        .trim()
        .to_string();

    if fixed_sql.is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    Ok(FixSqlResponse {
        fixed_sql,
        explanation,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExplainNaturalRequest {
    pub sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExplainNaturalResponse {
    pub explanation: String,
}

#[tauri::command]
pub async fn explain_sql_natural(request: ExplainNaturalRequest) -> Result<ExplainNaturalResponse, AppError> {
    if request.sql.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    let prompt = format!(
        "Given this MySQL query, explain in plain English what it does. \
         Be concise but complete. Include what tables are used, what filtering is applied, \
         and what the result represents.\n\n\
         Query: {}",
        request.sql
    );

    let url = config::get_llm_url().await;
    let model = config::get_llm_model().await;

    let ollama_request = reqwest::Client::new()
        .post(&format!("{}/api/generate", url.trim_end_matches('/')))
        .json(&serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
        }))
        .send()
        .await?;

    if !ollama_request.status().is_success() {
        return Err(AppError::QueryExecution(
            format!("Ollama returned status: {}", ollama_request.status())
        ));
    }

    #[derive(Deserialize)]
    struct OllamaResp {
        response: String,
    }

    let response: OllamaResp = ollama_request.json().await?;
    let explanation = response.response.trim().to_string();

    if explanation.is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    Ok(ExplainNaturalResponse { explanation })
}

// SQL Optimization via EXPLAIN + LLM analysis

#[derive(Debug, Serialize, Deserialize)]
pub struct OptimizeSqlRequest {
    pub sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OptimizeSqlResponse {
    pub original_explain: String,
    pub suggestions: String,
    pub optimized_sql: Option<String>,
}

#[tauri::command]
pub async fn optimize_sql(request: OptimizeSqlRequest) -> Result<OptimizeSqlResponse, AppError> {
    if request.sql.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    // Step 1: Run EXPLAIN
    let explain_result = query::explain_query(&request.sql).await?;

    // Convert explain results to text
    let mut explain_text = String::new();
    if !explain_result.columns.is_empty() {
        explain_text.push_str(&explain_result.columns.join("\t"));
        explain_text.push('\n');
        for row in &explain_result.rows {
            let vals: Vec<String> = row.iter().map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => "NULL".to_string(),
                other => other.to_string(),
            }).collect();
            explain_text.push_str(&vals.join("\t"));
            explain_text.push('\n');
        }
    }

    // Step 2: Ask LLM to analyze and suggest
    let prompt = format!(
        "You are a MySQL query optimization expert. Analyze the following EXPLAIN output and SQL query.\n\n\
         Original SQL:\n{}\n\n\
         EXPLAIN output:\n{}\n\n\
         Provide:\n\
         1. A brief analysis of what the EXPLAIN shows (table scans, index usage, etc.)\n\
         2. Specific optimization suggestions if any\n\
         3. An optimized version of the query if improvements are possible\n\n\
         Format your response as:\n\
         Analysis: <brief analysis>\n\
         Suggestions: <numbered list of suggestions>\n\
         Optimized SQL: <the optimized query, or 'No optimization needed' if none>",
        request.sql, explain_text
    );

    let url = config::get_llm_url().await;
    let model = config::get_llm_model().await;

    let response = reqwest::Client::new()
        .post(&format!("{}/api/generate", url.trim_end_matches('/')))
        .json(&serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(AppError::QueryExecution(
            format!("Ollama returned status: {}", response.status())
        ));
    }

    #[derive(Deserialize)]
    struct OllamaResp {
        response: String,
    }

    let body: OllamaResp = response.json().await?;
    let text = body.response.trim().to_string();

    // Parse the response - extract optimized SQL if present
    let mut optimized_sql = None;
    if let Some(idx) = text.find("Optimized SQL:") {
        let after = &text[idx + "Optimized SQL:".len()..];
        let sql = after.trim()
            .trim_start_matches("```sql")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string();
        if !sql.is_empty() && sql != "No optimization needed" {
            optimized_sql = Some(sql);
        }
    }

    Ok(OptimizeSqlResponse {
        original_explain: explain_text.trim().to_string(),
        suggestions: text,
        optimized_sql,
    })
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

    // Update in-memory config and save to disk
    {
        let path = dirs_next::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("naturalsql")
            .join("config.json");
        let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())?;
        // Update in-memory
        let mut guard = crate::config::CONFIG.lock().map_err(|e| e.to_string())?;
        *guard = config.clone();
    }

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
