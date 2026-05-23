use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

use crate::db::schema::{self, Schema};
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

    let db_names: Vec<&str> = all_schemas.iter().map(|s| s.database.as_str()).collect();
    let db_list = db_names.join(", ");

    let mut seen_tool_calls: Vec<String> = Vec::new();
    let mut conversation = String::new();

    for iteration in 0..max_iterations {
        eprintln!("[LLM] Tool iteration {}/{}", iteration + 1, max_iterations);

        let prompt = format!(
            "You are a MySQL expert. Available databases: {db_list}\n\n\
             You have access to tools. Use them by outputting a JSON object:\n\
             - {{\"tool\": \"list_tables\", \"database\": \"<db_name>\"}}\n\
             - {{\"tool\": \"get_table_schema\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\
             - {{\"tool\": \"get_sample_data\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\n\
             When you have enough information, write ONLY the SQL query. No explanations, no markdown, no backticks.\n\
             Always use fully qualified table names: database.table\n\
             IMPORTANT: If you need a tool, output ONLY the JSON. If you're ready, output ONLY the SQL.\n\
             Do NOT repeat the same tool call — use the results below to decide your next step.\n\n\
             {conversation}\n\n\
             User's question: {natural_language}",
        );

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
        struct OllamaResp { response: String }

        let body: OllamaResp = response.json().await?;
        let mut text = body.response.trim().to_string();

        eprintln!("[LLM] Raw response (first 300 chars): {}", text.chars().take(300).collect::<String>());

        // Strip markdown code blocks by filtering backtick lines
        text = text.lines()
            .filter(|line| !line.trim().starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        eprintln!("[LLM] After strip: {}", text.chars().take(300).collect::<String>());

        // Try to parse as JSON tool call
        let mut found_tool = false;

        if let Ok(tool_call) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                let call_sig = tool_call.to_string();
                if seen_tool_calls.contains(&call_sig) {
                    // Repeated call → fallback immediately with accumulated conversation
                    eprintln!("[LLM] Repeated tool call, using conversation context for fallback");
                    let fallback_prompt = format!(
                        "You are a MySQL 5.6+ expert. Based on the schema information gathered below, \
                         write a SQL query to answer the user's question.\n\
                         Only return the SQL query, no explanations, no markdown.\n\n\
                         {conversation}\n\n\
                         User's question: {natural_language}\n\n\
                         SQL Query:",
                    );
                    return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await;
                }
                seen_tool_calls.push(call_sig);

                found_tool = process_tool_call(tool_name, &tool_call, all_schemas, &db_list, &mut conversation)?;
            }
        }

        // If not parsed as whole JSON, try extracting from mixed content
        if !found_tool {
            if let Some(tool_call) = extract_first_json_object(&text) {
                if let Some(tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                    let call_sig = tool_call.to_string();
                    if seen_tool_calls.contains(&call_sig) {
                        eprintln!("[LLM] Repeated tool call (extracted), using conversation context for fallback");
                        let fallback_prompt = format!(
                            "You are a MySQL 5.6+ expert. Based on the schema information gathered below, \
                             write a SQL query to answer the user's question.\n\
                             Only return the SQL query, no explanations, no markdown.\n\n\
                             {conversation}\n\n\
                             User's question: {natural_language}\n\n\
                             SQL Query:",
                        );
                        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await;
                    }
                    seen_tool_calls.push(call_sig);

                    found_tool = process_tool_call(tool_name, &tool_call, all_schemas, &db_list, &mut conversation)?;
                }
            }
        }

        if found_tool {
            continue;
        }

        // Not a tool call — extract SQL
        let sql = extract_sql(&text);

        if !sql.is_empty() && !sql.starts_with('{') && !sql.contains("\"tool\":") {
            eprintln!("[LLM] Final SQL: {}", sql.chars().take(100).collect::<String>());
            return Ok(sql);
        }

        // No valid SQL — fallback with conversation context
        eprintln!("[LLM] No valid SQL from tool iteration, using conversation context for fallback");
        let fallback_prompt = format!(
            "You are a MySQL 5.6+ expert. Based on the schema information gathered below, \
             write a SQL query to answer the user's question.\n\
             Only return the SQL query, no explanations, no markdown.\n\n\
             {conversation}\n\n\
             User's question: {natural_language}\n\n\
             SQL Query:",
        );
        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await;
    }

    // Max iterations → fallback with all schemas
    eprintln!("[LLM] Max iterations reached, using all schemas for fallback");
    let schema_context = schema::format_all_schemas_for_prompt(all_schemas);
    natural_language_to_sql(natural_language, &schema_context).await
}

async fn call_ollama_generate(
    url: &str,
    model: &str,
    client: &reqwest::Client,
    api_url: &str,
    prompt: &str,
) -> Result<String, AppError> {
    let response = client
        .post(api_url)
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

    eprintln!("[LLM] Fallback SQL: {}", sql.chars().take(100).collect::<String>());
    Ok(sql)
}

fn extract_first_json_object(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let from_start = &text[start..];

    // Count braces to find the matching closing brace
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in from_start.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match c {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&from_start[..=i]) {
                        return Some(val);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn process_tool_call(
    tool_name: &str,
    tool_call: &serde_json::Value,
    all_schemas: &[Schema],
    db_list: &str,
    conversation: &mut String,
) -> Result<bool, AppError> {
    eprintln!("[LLM] Tool call: {tool_name}");

    match tool_name {
        "list_tables" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(schema) = all_schemas.iter().find(|s| s.database == database) {
                let tables: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
                let result = format!("Tables in '{}': {}", database, tables.join(", "));
                conversation.push_str(&format!("\n\nTool result: {result}"));
                eprintln!("[LLM] {result}");
            } else {
                conversation.push_str(&format!("\n\nTool error: Database '{}' not found. Available: {}", database, db_list));
            }
        }
        "get_table_schema" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(schema) = all_schemas.iter().find(|s| s.database == database) {
                if let Some(tbl) = schema.tables.iter().find(|t| t.name == table) {
                    let cols: Vec<String> = tbl.columns.iter().map(|c| {
                        let key = if !c.column_key.is_empty() { format!(" [{}]", c.column_key) } else { "".to_string() };
                        format!("  {} {}{}{}", c.name, c.column_type, key, if c.is_nullable { " NULL" } else { " NOT NULL" })
                    }).collect();
                    let result = format!("Table {}.{} columns:\n{}", database, table, cols.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                    eprintln!("[LLM] {}", result.chars().take(100).collect::<String>());
                } else {
                    conversation.push_str(&format!("\n\nTool error: Table '{}' not found in '{}'", table, database));
                }
            } else {
                conversation.push_str(&format!("\n\nTool error: Database '{}' not found", database));
            }
        }
        "get_sample_data" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            let result = format!("Sample: SELECT * FROM {database}.{table} LIMIT 3");
            conversation.push_str(&format!("\n\nTool result: {result}"));
        }
        _ => {
            conversation.push_str(&format!("\n\nTool error: Unknown tool '{tool_name}'"));
        }
    }
    Ok(true)
}

fn extract_sql(text: &str) -> String {
    let text = text.trim();

    // If it's wrapped in ```sql blocks
    let mut result = text.to_string();
    while result.starts_with("```sql") || result.starts_with("```") {
        let prefix = if result.starts_with("```sql") { 6 } else { 3 };
        if let Some(end) = result[prefix..].find("```") {
            let inner = result[prefix..prefix+end].trim();
            let after = result[prefix+end+3..].trim();
            result = if after.is_empty() { inner.to_string() } else { break };
        } else {
            break;
        }
    }

    // Extract SQL keywords from mixed content
    let result = result.trim();

    // If it starts with SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, EXPLAIN, WITH
    let upper = result.to_uppercase();
    if upper.starts_with("SELECT") || upper.starts_with("INSERT") || upper.starts_with("UPDATE")
        || upper.starts_with("DELETE") || upper.starts_with("CREATE") || upper.starts_with("DROP")
        || upper.starts_with("ALTER") || upper.starts_with("EXPLAIN") || upper.starts_with("WITH") {
        // Take until we hit a newline followed by non-SQL content
        let sql: String = result.lines()
            .take_while(|line| {
                let t = line.trim();
                !t.starts_with('{') && !t.starts_with("```") && !t.is_empty()
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !sql.is_empty() {
            return sql.trim().to_string();
        }
    }

    result.to_string()
}
