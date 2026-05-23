use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

use crate::db::schema::Schema;
use crate::error::AppError;
use crate::config;

/// Fallback: single-prompt approach (use when schema is already small)
#[allow(dead_code)]
pub async fn natural_language_to_sql(
    natural_language: &str,
    schema_context: &str,
) -> Result<String, AppError> {
    let url = config::get_llm_url().await;
    let model = config::get_llm_model().await;

    let prompt = format!(
        "You are a MySQL 5.6+ expert. Given the database schema below, convert the user's natural language question into a valid MySQL SQL query.\n\
         Only return the SQL query, no explanations, no markdown formatting, no backticks.\n\n\
         {}\n\
         Question: {}\n\n\
         SQL Query:",
        schema_context, natural_language
    );

    let request = json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
    });

    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let api_url = format!("{}/api/generate", url.trim_end_matches('/'));
    let response = client
        .post(&api_url)
        .json(&request)
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

    let ollama_response: OllamaResp = response.json().await?;

    let sql = ollama_response.response.trim().to_string();
    let sql = sql
        .trim_start_matches("```sql")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    if sql.is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    Ok(sql)
}

/// Manual JSON-based tool calling for schema discovery
/// Works with models that don't support native function calling
pub async fn natural_language_to_sql_with_tools(
    natural_language: &str,
    all_schemas: &[Schema],
) -> Result<String, AppError> {
    let url = config::get_llm_url().await;
    let model = config::get_llm_model().await;

    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    let api_url = format!("{}/api/generate", url.trim_end_matches('/'));
    let max_iterations = 10;

    // Build the available databases list for context
    let db_names: Vec<&str> = all_schemas.iter().map(|s| s.database.as_str()).collect();
    let db_list = db_names.join(", ");

    for iteration in 0..max_iterations {
        eprintln!("[LLM] Tool iteration {}/{}", iteration + 1, max_iterations);

        // Build prompt with instruction to use tools via JSON
        let tools_instructions = format!(
            "You are a MySQL expert. Available databases: {db_list}\n\n\
             You have access to these tools. Use them by outputting a JSON object on a single line:\n\
             \n\
             Tool: list_tables\n\
             Use when: you need to see what tables exist in a database\n\
             JSON format: {{\"tool\": \"list_tables\", \"database\": \"<db_name>\"}}\n\
             \n\
             Tool: get_table_schema\n\
             Use when: you need column definitions for a specific table\n\
             JSON format: {{\"tool\": \"get_table_schema\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\
             \n\
             Tool: get_sample_data\n\
             Use when: you need to see example values from a table\n\
             JSON format: {{\"tool\": \"get_sample_data\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\
             \n\
             When you have enough information, write ONLY the SQL query. No explanations, no markdown, no backticks.\n\
             Always use fully qualified table names: database.table\n\
             \n\
             IMPORTANT: If you need a tool, output ONLY the JSON object. If you're ready to write SQL, output ONLY the SQL.\n\
             User's question: {natural_language}",
        );

        // Add tool results from previous iterations as context
        let mut prompt = tools_instructions.clone();

        let response = client
            .post(&api_url)
            .json(&json!({
                "model": model,
                "prompt": prompt,
                "stream": false,
                "temperature": 0.1,
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

        eprintln!("[LLM] Raw response (first 200 chars): {}", text.chars().take(200).collect::<String>());

        // Try to parse as JSON tool call
        if let Ok(tool_call) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                eprintln!("[LLM] Tool call detected: {tool_name}");

                match tool_name {
                    "list_tables" => {
                        let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
                        if let Some(schema) = all_schemas.iter().find(|s| s.database == database) {
                            let tables: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
                            let result = format!("Tables in '{}': {}", database, tables.join(", "));
                            prompt = format!("{}\n\nTool result: {result}\n\nNow, what's your next step? Output JSON for another tool call, or the final SQL query.", prompt);
                            eprintln!("[LLM] Tool result: {result}");
                        } else {
                            prompt = format!("{}\n\nTool error: Database '{}' not found. Available: {}", prompt, database, db_list);
                        }
                    }
                    "get_table_schema" => {
                        let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
                        let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
                        if let Some(schema) = all_schemas.iter().find(|s| s.database == database) {
                            if let Some(tbl) = schema.tables.iter().find(|t| t.name == table) {
                                let cols: Vec<String> = tbl.columns.iter().map(|c| {
                                    let key = if !c.column_key.is_empty() {
                                        format!(" [{}]", c.column_key)
                                    } else { "".to_string() };
                                    format!("  {} {}{}{}", c.name, c.column_type, key,
                                        if c.is_nullable { " NULL" } else { " NOT NULL" })
                                }).collect();
                                let result = format!("Table {}.{} columns:\n{}", database, table, cols.join("\n"));
                                prompt = format!("{}\n\nTool result: {result}\n\nNow, what's your next step? Output JSON for another tool call, or the final SQL query.", prompt);
                                eprintln!("[LLM] Tool result: {}", result.chars().take(100).collect::<String>());
                            } else {
                                prompt = format!("{}\n\nTool error: Table '{}' not found in '{}'", prompt, table, database);
                            }
                        } else {
                            prompt = format!("{}\n\nTool error: Database '{}' not found", prompt, database);
                        }
                    }
                    "get_sample_data" => {
                        let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
                        let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
                        let result = format!("Sample data query would run: SELECT * FROM {database}.{table} LIMIT 3");
                        prompt = format!("{}\n\nTool result: {result}\n\nNow, what's your next step? Output JSON for another tool call, or the final SQL query.", prompt);
                        eprintln!("[LLM] Tool result: {result}");
                    }
                    _ => {
                        prompt = format!("{}\n\nTool error: Unknown tool '{}'", prompt, tool_name);
                    }
                }
                continue; // Go to next iteration
            }
        }

        // Not a tool call — treat as final SQL
        let sql = text
            .trim_start_matches("```sql")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string();

        // Also handle markdown-wrapped SQL
        let sql = sql.trim().to_string();

        if sql.is_empty() {
            return Err(AppError::InvalidLlmResponse);
        }

        eprintln!("[LLM] Final SQL (first 100 chars): {}", sql.chars().take(100).collect::<String>());
        return Ok(sql);
    }

    Err(AppError::InvalidLlmResponse)
}
