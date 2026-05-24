use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
#[allow(dead_code)]
pub struct SqlResponse {
    pub sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCallStep {
    pub tool_name: String,
    pub parameters: HashMap<String, String>,
    pub result: String,
    pub iteration: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NlToSqlResponse {
    pub sql: String,
    pub tool_calls: Vec<ToolCallStep>,
    pub iterations: u32,
    pub used_fallback: bool,
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
pub async fn nl_to_sql(request: NlToSqlRequest) -> Result<NlToSqlResponse, AppError> {
    let all_schemas = schema::load_all_cached_schemas()?;
    if all_schemas.is_empty() {
        return Err(AppError::SchemaNotCached);
    }

    // Try tool-based approach first
    // Pass the (previously ignored) database param to scope prompts and bias toward selected DB
    let selected_db = if request.database.trim().is_empty() { None } else { Some(request.database.as_str()) };
    match llm::natural_language_to_sql_with_tools(
        &request.natural_language,
        &all_schemas,
        selected_db,
    ).await {
        Ok((sql, tool_steps, iterations)) => {
            Ok(NlToSqlResponse {
                sql,
                tool_calls: tool_steps.into_iter().map(|s| crate::commands::ToolCallStep {
                    tool_name: s.tool_name,
                    parameters: s.parameters,
                    result: s.result,
                    iteration: s.iteration,
                }).collect(),
                iterations,
                used_fallback: false,
            })
        }
        Err(e) => {
            // Fallback to single-prompt approach if tool calling fails
            eprintln!("Tool calling failed, falling back to single-prompt: {}", e);
            let schema_context = schema::format_all_schemas_for_prompt(&all_schemas);
            let sql = llm::natural_language_to_sql(
                &request.natural_language,
                &schema_context,
            ).await?;
            Ok(NlToSqlResponse {
                sql,
                tool_calls: Vec::new(),
                iterations: 0,
                used_fallback: true,
            })
        }
    }
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

// Smart Join Builder - Natural language to JOIN SQL

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildJoinRequest {
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildJoinResponse {
    pub sql: String,
}

#[tauri::command]
pub async fn build_join(request: BuildJoinRequest) -> Result<BuildJoinResponse, AppError> {
    if request.description.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    // Load all cached schemas for context
    let all_schemas = schema::load_all_cached_schemas()?;
    
    // Discover cross-database relationships - try actual FK first, then heuristic
    let mut cross_db_relations = Vec::new();
    for s in &all_schemas {
        if let Ok(rels) = schema::introspect_foreign_keys(&s.database).await {
            cross_db_relations.extend(rels);
        }
    }
    if cross_db_relations.is_empty() {
        cross_db_relations = schema::find_cross_database_relationships(&all_schemas);
    }
    
    // Use enhanced schema format with relationship hints for cross-database joins
    let schema_context = if all_schemas.is_empty() {
        "No schema information available. Use standard table names.".to_string()
    } else if cross_db_relations.is_empty() {
        schema::format_all_schemas_for_prompt(&all_schemas)
    } else {
        schema::format_schemas_with_relationships(&all_schemas, &cross_db_relations)
    };

    let prompt = format!(
        "You are a MySQL expert. Given the database schema below, generate a SQL JOIN query \
         based on the user's description. Return ONLY the SQL query, no explanations, no markdown.\n\
         Always use fully qualified table names: database.table\n\
         For cross-database JOINs, ensure both tables are on the same MySQL server.\n\n\
         {}\n\
         User request: {}\n\n\
         SQL:",
        schema_context, request.description
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
    let sql = body.response.trim()
        .trim_start_matches("```sql")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    if sql.is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    Ok(BuildJoinResponse { sql })
}

// Cross-database JOIN validation - checks if tables exist and suggests join conditions

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateCrossDbJoinRequest {
    pub left_table: String,
    pub right_table: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidateCrossDbJoinResponse {
    pub valid: bool,
    pub left_exists: bool,
    pub right_exists: bool,
    pub suggested_join_columns: Vec<(String, String)>,
    pub has_relationship: bool,
}

#[tauri::command]
pub async fn validate_cross_db_join(
    request: ValidateCrossDbJoinRequest,
) -> Result<ValidateCrossDbJoinResponse, AppError> {
    let all_schemas = schema::load_all_cached_schemas()?;
    
    // Parse table names (support both "table" and "database.table" formats)
    let (left_db, left_table) = if request.left_table.contains('.') {
        let parts: Vec<&str> = request.left_table.splitn(2, '.').collect();
        (parts[0].to_string(), parts[1].to_string())
    } else {
        // If no database specified, find in any schema
        ("*".to_string(), request.left_table.clone())
    };

    let (right_db, right_table) = if request.right_table.contains('.') {
        let parts: Vec<&str> = request.right_table.splitn(2, '.').collect();
        (parts[0].to_string(), parts[1].to_string())
    } else {
        ("*".to_string(), request.right_table.clone())
    };

    // Find tables in schemas
    let mut left_exists = false;
    let mut right_exists = false;
    let mut left_columns: Vec<schema::ColumnInfo> = Vec::new();
    let mut right_columns: Vec<schema::ColumnInfo> = Vec::new();

    for schema in &all_schemas {
        if left_db == "*" || schema.database == left_db {
            if let Some(table) = schema.tables.iter().find(|t| t.name == left_table) {
                left_exists = true;
                left_columns = table.columns.clone();
            }
        }
        if right_db == "*" || schema.database == right_db {
            if let Some(table) = schema.tables.iter().find(|t| t.name == right_table) {
                right_exists = true;
                right_columns = table.columns.clone();
            }
        }
    }

    // Find matching columns for potential join keys
    let mut suggested_join_columns: Vec<(String, String)> = Vec::new();
    let left_col_names: std::collections::HashSet<_> = left_columns.iter().map(|c| &c.name).collect();
    
    for right_col in &right_columns {
        if left_col_names.contains(&right_col.name) {
            // Matching column name - potential join key
            suggested_join_columns.push((right_col.name.clone(), right_col.name.clone()));
        }
    }

    // Check for FK relationships - try actual FK first, then heuristic
    let mut cross_db_relations = Vec::new();
    for s in &all_schemas {
        if let Ok(rels) = schema::introspect_foreign_keys(&s.database).await {
            cross_db_relations.extend(rels);
        }
    }
    if cross_db_relations.is_empty() {
        cross_db_relations = schema::find_cross_database_relationships(&all_schemas);
    }
    let has_relationship = !cross_db_relations.is_empty();

    Ok(ValidateCrossDbJoinResponse {
        valid: left_exists && right_exists,
        left_exists,
        right_exists,
        suggested_join_columns,
        has_relationship,
    })
}

// Data Analysis Chat - Natural language data analysis → SQL → Execute → Interpret

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeDataRequest {
    pub question: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DashboardWidget {
    pub id: String,
    pub r#type: String, // "stat", "bar", "line", "area", "pie", "table"
    pub title: String,
    pub sql: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Dashboard {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub widgets: Vec<DashboardWidget>,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeDataResponse {
    pub sql: String,
    pub answer: String,
    pub data: Option<query::QueryResult>,
    pub dashboard: Option<Dashboard>,
}

#[tauri::command]
pub async fn analyze_data(request: AnalyzeDataRequest) -> Result<AnalyzeDataResponse, AppError> {
    if request.question.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    // Load all cached schemas for context
    let all_schemas = schema::load_all_cached_schemas()?;
    let schema_context = if all_schemas.is_empty() {
        "No schema information available.".to_string()
    } else {
        schema::format_all_schemas_for_prompt(&all_schemas)
    };

    // Step 1: Generate SQL
    let sql_prompt = format!(
        "You are a MySQL data analyst. Given the database schema below, write a SQL query to answer the user's question.\n\
         Return ONLY the SQL query, no explanations, no markdown.\n\n\
         {}\n\
         Question: {}\n\n\
         SQL:",
        schema_context, request.question
    );

    let url = config::get_llm_url().await;
    let model = config::get_llm_model().await;

    let response = reqwest::Client::new()
        .post(&format!("{}/api/generate", url.trim_end_matches('/')))
        .json(&serde_json::json!({
            "model": model,
            "prompt": sql_prompt,
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
    struct OllamaResp { response: String }

    let body: OllamaResp = response.json().await?;
    let sql = body.response.trim()
        .trim_start_matches("```sql")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    if sql.is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    // Step 2: Execute the query
    let data = query::execute_query(&sql).await.ok();

    // Step 3: Ask LLM to interpret the results
    let data_summary = if let Some(ref d) = data {
        let mut summary = format!("Query returned {} rows.\n\n", d.row_count);
        summary.push_str(&format!("Columns: {}\n\n", d.columns.join(", ")));
        // Include first 5 rows as sample
        for (i, row) in d.rows.iter().enumerate().take(5) {
            let vals: Vec<String> = row.iter().map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => "NULL".to_string(),
                other => other.to_string(),
            }).collect();
            summary.push_str(&format!("Row {}: {}\n", i + 1, vals.join(", ")));
        }
        if d.row_count > 5 {
            summary.push_str(&format!("\n... and {} more rows\n", d.row_count - 5));
        }
        summary
    } else {
        "Query executed successfully but returned no data.".to_string()
    };

    let interpret_prompt = format!(
        "You are a data analyst. Given the user's question, the SQL query, and the query results,\n\
         provide a clear, concise answer to the user's question. Include key insights from the data.\n\n\
         Question: {}\n\n\
         SQL: {}\n\n\
         Results:\n{}\n\n\
         Answer:",
        request.question, sql, data_summary
    );

    let interp_response = reqwest::Client::new()
        .post(&format!("{}/api/generate", url.trim_end_matches('/')))
        .json(&serde_json::json!({
            "model": model,
            "prompt": interpret_prompt,
            "stream": false,
        }))
        .send()
        .await?;

    let answer = if interp_response.status().is_success() {
        match interp_response.json::<OllamaResp>().await {
            Ok(b) => b.response.trim().to_string(),
            Err(_) => "Query executed successfully.".to_string(),
        }
    } else {
        "Query executed successfully.".to_string()
    };

    // Step 4: Check if a dashboard was requested and generate it
    let mut dashboard = None;
    let q_lower = request.question.to_lowercase();
    if q_lower.contains("dashboard") || q_lower.contains("visualize") || q_lower.contains("chart") {
        let dashboard_prompt = format!(
            "You are a dashboard architect. Given the database schema and the user's request, \
             design a dashboard with 3-5 widgets. Each widget needs a title, a type, and a SQL query.\n\
             Types: stat, bar, line, area, pie, table.\n\
             Return ONLY a JSON object with this structure:\n\
             {{\"title\": \"...\", \"description\": \"...\", \"widgets\": [{{\"id\": \"1\", \"type\": \"...\", \"title\": \"...\", \"sql\": \"...\"}}]}}\n\n\
             {}\n\n\
             Request: {}\n\n\
             JSON:",
            schema_context, request.question
        );

        let dash_response = reqwest::Client::new()
            .post(&format!("{}/api/generate", url.trim_end_matches('/')))
            .json(&serde_json::json!({
                "model": model,
                "prompt": dashboard_prompt,
                "format": "json",
                "stream": false,
            }))
            .send()
            .await?;

        if dash_response.status().is_success() {
            if let Ok(b) = dash_response.json::<OllamaResp>().await {
                if let Ok(mut d) = serde_json::from_str::<Dashboard>(&b.response) {
                    d.id = uuid::Uuid::new_v4().to_string();
                    d.created_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    dashboard = Some(d);
                }
            }
        }
    }

    Ok(AnalyzeDataResponse {
        sql,
        answer,
        data,
        dashboard,
    })
}

// Result Set LLM Features

#[derive(Debug, Serialize, Deserialize)]
pub struct ResultActionRequest {
    pub question: String,
    pub columns: Vec<String>,
    pub sample_rows: Vec<Vec<serde_json::Value>>,
    pub total_rows: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResultActionResponse {
    pub response: String,
    pub suggested_sql: Option<String>,
}

#[tauri::command]
pub async fn result_set_action(request: ResultActionRequest) -> Result<ResultActionResponse, AppError> {
    if request.question.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    // Build a summary of the data
    let col_names = request.columns.join(", ");
    let sample: Vec<String> = request.sample_rows.iter().take(3).map(|row| {
        row.iter().map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => "NULL".to_string(),
            other => other.to_string(),
        }).collect::<Vec<_>>().join(", ")
    }).collect();

    let data_summary = format!(
        "Columns: {}\nTotal rows: {}\nSample data:\n{}",
        col_names, request.total_rows, sample.join("\n")
    );

    let prompt = format!(
        "You are a MySQL data analyst. Given the following query result data and the user's request,\n\
         provide your response.\n\n\
         If the user asks for aggregation, filtering, or transformation, also provide the SQL query\n\
         that would achieve this on the same table(s) the data came from.\n\n\
         {}\\n\n\
         User request: {}\n\n\
         Response:",
        data_summary, request.question
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
    struct OllamaResp { response: String }

    let body: OllamaResp = response.json().await?;
    let text = body.response.trim().to_string();

    // Try to extract SQL if present
    let mut suggested_sql = None;
    if let Some(idx) = text.find("```sql") {
        let after = &text[idx + 6..];
        if let Some(end) = after.find("```") {
            let sql = after[..end].trim().to_string();
            if !sql.is_empty() {
                suggested_sql = Some(sql);
            }
        }
    } else if let Some(idx) = text.find("SELECT") {
        let after = &text[idx..];
        let sql: String = after.lines().take_while(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#') && !t.starts_with("//")
        }).collect::<Vec<_>>().join(" ");
        if !sql.is_empty() {
            suggested_sql = Some(sql);
        }
    }

    // Clean response text - remove SQL blocks for readability
    let clean_response = text
        .replace("```sql\n", "\n")
        .replace("```", "")
        .trim()
        .to_string();

    Ok(ResultActionResponse {
        response: if suggested_sql.is_some() {
            clean_response
        } else {
            text
        },
        suggested_sql,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct TableStructureRequest {
    pub database: String,
    pub table: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TableStructureResponse {
    pub ddl: String,
    pub indexes: Vec<schema::IndexInfo>,
    pub constraints: Vec<schema::ConstraintInfo>,
    pub foreign_keys: Vec<schema::ForeignKeyRelation>,
    pub stats: schema::TableStats,
    pub status: schema::TableStatus,
}

#[tauri::command]
pub async fn get_table_structure(
    request: TableStructureRequest,
) -> Result<TableStructureResponse, AppError> {
    let ddl = schema::get_table_ddl(&request.database, &request.table).await?;
    let indexes = schema::get_table_indexes(&request.database, &request.table).await?;
    let constraints = schema::get_table_constraints(&request.database, &request.table).await?;
    let foreign_keys = schema::get_table_foreign_keys(&request.database, &request.table).await?;
    let stats = schema::get_table_statistics(&request.database, &request.table).await?;
    let status = schema::get_table_status(&request.database, &request.table).await?;

    Ok(TableStructureResponse {
        ddl,
        indexes,
        constraints,
        foreign_keys,
        stats,
        status,
    })
}

#[tauri::command]
pub async fn explain_sql_json(request: ExecuteRequest) -> Result<String, AppError> {
    query::explain_query_json(&request.sql).await
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaMigrationRequest {
    pub natural_language: String,
    pub database: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaMigrationResponse {
    pub sql: String,
    pub explanation: String,
    pub risk_level: String,
}

#[tauri::command]
pub async fn schema_migration(
    request: SchemaMigrationRequest,
) -> Result<SchemaMigrationResponse, AppError> {
    if request.natural_language.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    let all_schemas = schema::load_all_cached_schemas()?;
    let schema_context = if all_schemas.is_empty() {
        "No schema information available.".to_string()
    } else {
        schema::format_all_schemas_for_prompt(&all_schemas)
    };

    let prompt = format!(
        "You are a MySQL 5.6+ database administrator. Given the database schema below, generate DDL SQL \
         (ALTER TABLE, CREATE TABLE, DROP TABLE, CREATE INDEX, etc.) based on the user's request.\n\n\
         IMPORTANT RULES:\n\
         1. Only generate DDL statements (no SELECT/INSERT/UPDATE/DELETE).\n\
         2. Always use fully qualified table names: database.table\n\
         3. Return ONLY the SQL on the first line(s), then a blank line, then a brief explanation.\n\
         4. Do not include markdown code blocks or backticks.\n\
         5. Preserve existing data where possible (use ADD COLUMN, not DROP + CREATE).\n\n\
         {}\n\n\
         User request: {}\n\n\
         DDL SQL:",
        schema_context, request.natural_language
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

    let parts: Vec<&str> = text.splitn(2, "\n\n").collect();
    let sql = parts.first()
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

    if sql.is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    let sql_upper = sql.to_uppercase();
    let risk_level = if sql_upper.contains("DROP TABLE") || sql_upper.contains("DROP DATABASE") {
        "high".to_string()
    } else if sql_upper.contains("ALTER TABLE") && (sql_upper.contains("DROP COLUMN") || sql_upper.contains("MODIFY COLUMN")) {
        "high".to_string()
    } else if sql_upper.contains("CREATE TABLE") || sql_upper.contains("CREATE INDEX") {
        "medium".to_string()
    } else {
        "low".to_string()
    };

    Ok(SchemaMigrationResponse {
        sql,
        explanation,
        risk_level,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataEditRequest {
    pub natural_language: String,
    pub database: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataEditResponse {
    pub sql: String,
    pub preview_sql: String,
    pub explanation: String,
    pub undo_sql: String,
    pub affected_estimate: String,
}

#[tauri::command]
pub async fn nl_data_edit(
    request: DataEditRequest,
) -> Result<DataEditResponse, AppError> {
    if request.natural_language.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    let all_schemas = schema::load_all_cached_schemas()?;
    let schema_context = if all_schemas.is_empty() {
        "No schema information available.".to_string()
    } else {
        schema::format_all_schemas_for_prompt(&all_schemas)
    };

    let prompt = format!(
        "You are a MySQL 5.6+ data editor. Given the database schema below, generate DML SQL \
         (INSERT, UPDATE, DELETE) based on the user's request.\n\n\
         STRICT RULES:\n\
         1. Only generate DML statements (no SELECT, no DDL).\n\
         2. Always include a WHERE clause for UPDATE and DELETE.\n\
         3. Use fully qualified table names: database.table\n\
         4. Format your response as follows (4 sections separated by blank lines):\n\
            - The DML SQL statement\n\
            - A SELECT query to preview affected data (preview)\n\
            - A brief explanation\n\
            - An undo SQL statement (reverse operation)\n\
         5. Do not include markdown code blocks or backticks.\n\n\
         {}\n\n\
         User request: {}\n\n\
         DML SQL:",
        schema_context, request.natural_language
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

    let parts: Vec<&str> = text.split("\n\n").collect();

    let clean = |s: &str| -> String {
        s.trim()
            .trim_start_matches("```sql")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string()
    };

    let sql = clean(parts.first().unwrap_or(&""));
    let preview_sql = clean(parts.get(1).unwrap_or(&""));
    let explanation = parts.get(2).unwrap_or(&"").trim().to_string();
    let undo_sql = clean(parts.get(3).unwrap_or(&""));

    if sql.is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    let sql_upper = sql.to_uppercase();
    if (sql_upper.starts_with("UPDATE") || sql_upper.starts_with("DELETE")) && !sql_upper.contains("WHERE") {
        return Err(AppError::QueryExecution(
            "Safety check failed: Generated SQL has no WHERE clause. Refusing to execute.".to_string()
        ));
    }

    let affected_estimate = if sql_upper.starts_with("INSERT") {
        "1 row".to_string()
    } else {
        "unknown".to_string()
    };

    Ok(DataEditResponse {
        sql,
        preview_sql,
        explanation,
        undo_sql,
        affected_estimate,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamingRequest {
    pub sql: String,
    pub query_id: String,
}

#[tauri::command]
pub async fn execute_sql_streaming(
    app_handle: tauri::AppHandle,
    request: StreamingRequest,
) -> Result<(), AppError> {
    if request.sql.trim().is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    // Register for cancel
    let _cancel_flag = query::register_query(&request.query_id);

    let result = query::execute_query_streaming(
        &app_handle,
        &request.sql,
        &request.query_id,
    ).await;

    query::unregister_query(&request.query_id);

    if let Err(ref e) = result {
        use tauri::Emitter;
        let _ = app_handle.emit("sql-stream-error", query::StreamErrorPayload {
            query_id: request.query_id.clone(),
            error: e.to_string(),
        });
    }

    result
}

#[tauri::command]
pub async fn cancel_running_query(query_id: String) -> Result<bool, String> {
    Ok(query::cancel_query(&query_id))
}

// ========================
// ER Diagram Data
// ========================

#[derive(Debug, Serialize, Deserialize)]
pub struct ErColumnNode {
    pub name: String,
    pub column_type: String,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErTableNode {
    pub database: String,
    pub table: String,
    pub columns: Vec<ErColumnNode>,
    pub row_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErRelation {
    pub constraint_name: Option<String>,
    pub from_database: String,
    pub from_table: String,
    pub from_column: String,
    pub to_database: String,
    pub to_table: String,
    pub to_column: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErDiagramResponse {
    pub tables: Vec<ErTableNode>,
    pub relations: Vec<ErRelation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErDiagramRequest {
    pub database: String,
}

#[tauri::command]
pub async fn get_er_diagram_data(
    request: ErDiagramRequest,
) -> Result<ErDiagramResponse, AppError> {
    if request.database.trim().is_empty() {
        return Err(AppError::QueryExecution("Database name is required.".to_string()));
    }

    // Load cached schema for table/column info
    let cached = schema::load_cached_schema(&request.database)?;
    let schema = cached.ok_or_else(|| AppError::QueryExecution(
        format!("Database '{}' is not cached. Please cache it first.", request.database)
    ))?;

    // Get all foreign keys from the database
    let fk_relations = schema::introspect_foreign_keys(&request.database).await?;

    // Build a set of FK columns for quick lookup
    let mut fk_columns = std::collections::HashSet::new();
    for rel in &fk_relations {
        fk_columns.insert((rel.from_table.clone(), rel.from_column.clone()));
    }

    // Build table nodes with column info
    let mut tables = Vec::new();
    for table in &schema.tables {
        let columns: Vec<ErColumnNode> = table.columns.iter().map(|col| {
            ErColumnNode {
                name: col.name.clone(),
                column_type: col.column_type.clone(),
                is_primary_key: col.column_key == "PRI",
                is_foreign_key: fk_columns.contains(&(table.name.clone(), col.name.clone())),
            }
        }).collect();

        // Get approximate row count
        let row_count = schema::get_table_row_count(&request.database, &table.name).await.unwrap_or(0);

        tables.push(ErTableNode {
            database: request.database.clone(),
            table: table.name.clone(),
            columns,
            row_count,
        });
    }

    // Also include referenced tables from other databases (cross-db FK references)
    let mut seen_tables: std::collections::HashSet<String> = tables.iter().map(|t| t.table.clone()).collect();
    let mut extra_tables: Vec<ErTableNode> = Vec::new();

    for rel in &fk_relations {
        if rel.to_database != request.database && !seen_tables.contains(&rel.to_table) {
            if let Ok(Some(other_schema)) = schema::load_cached_schema(&rel.to_database) {
                if let Some(ref_table) = other_schema.tables.iter().find(|t| t.name == rel.to_table) {
                    let columns: Vec<ErColumnNode> = ref_table.columns.iter().map(|col| ErColumnNode {
                        name: col.name.clone(),
                        column_type: col.column_type.clone(),
                        is_primary_key: col.column_key == "PRI",
                        is_foreign_key: false,
                    }).collect();

                    let row_count = schema::get_table_row_count(&rel.to_database, &ref_table.name).await.unwrap_or(0);

                    extra_tables.push(ErTableNode {
                        database: rel.to_database.clone(),
                        table: ref_table.name.clone(),
                        columns,
                        row_count,
                    });
                    seen_tables.insert(ref_table.name.clone());
                }
            }
        }
    }

    tables.extend(extra_tables);

    // Convert FK relations
    let relations: Vec<ErRelation> = fk_relations.into_iter().map(|rel| {
        ErRelation {
            constraint_name: rel.constraint_name,
            from_database: rel.from_database,
            from_table: rel.from_table,
            from_column: rel.from_column,
            to_database: rel.to_database,
            to_table: rel.to_table,
            to_column: rel.to_column,
        }
    }).collect();

    Ok(ErDiagramResponse { tables, relations })
}
