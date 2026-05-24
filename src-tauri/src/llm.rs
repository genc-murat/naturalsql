use mysql_async::prelude::Queryable;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

use crate::db::schema::{self, Schema};
use crate::error::AppError;
use crate::config;

/// Structured record of a single tool call
#[derive(Debug, Clone)]
pub struct ToolCallStep {
    pub tool_name: String,
    pub parameters: HashMap<String, String>,
    pub result: String,
    pub iteration: u32,
}

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
/// Returns: (sql, tool_steps, iterations_used)
pub async fn natural_language_to_sql_with_tools(
    natural_language: &str,
    all_schemas: &[Schema],
) -> Result<(String, Vec<ToolCallStep>, u32), AppError> {
    let url = config::get_llm_url().await;
    let model = config::get_llm_model().await;

    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    let api_url = format!("{}/api/generate", url.trim_end_matches('/'));
    let max_iterations = 6;

    let db_names: Vec<&str> = all_schemas.iter().map(|s| s.database.as_str()).collect();
    let db_list = db_names.join(", ");

    // Discover cross-database relationships
    // First, try to get actual FK constraints from MySQL
    let mut cross_db_relations = Vec::new();
    for schema in all_schemas {
        match schema::introspect_foreign_keys(&schema.database).await {
            Ok(relations) => cross_db_relations.extend(relations),
            Err(e) => {
                eprintln!("[LLM] Failed to introspect FK for '{}': {}", schema.database, e);
            }
        }
    }

    // If no FK relations found, fall back to heuristic matching by column names
    if cross_db_relations.is_empty() {
        eprintln!("[LLM] No FK relations from DB, falling back to heuristic matching");
        cross_db_relations = schema::find_cross_database_relationships(all_schemas);
    }

    let has_cross_db_relations = !cross_db_relations.is_empty();

    let mut seen_tool_calls: Vec<String> = Vec::new();
    let mut conversation = String::new();
    let mut schemas_explored: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut tool_steps: Vec<ToolCallStep> = Vec::new();

    for iteration in 0..max_iterations {
        eprintln!("[LLM] Tool iteration {}/{}", iteration + 1, max_iterations);

        let exploration_guidance = if schemas_explored.len() >= 2 {
            let tables_list = schemas_explored.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
            format!("You already have schema for: {tables_list}. Write the SQL query NOW. Do NOT call any more schema tools.\n")
        } else if schemas_explored.len() == 1 {
            let table_name = schemas_explored.iter().next().unwrap();
            format!("You already have schema for {table_name}. Get the SECOND table's schema with get_table_schema, or use cross_db_join. Do NOT call get_indexes or get_foreign_keys — you don't need them to write SQL.\n")
        } else {
            "RULE: You MUST call get_table_schema for each table BEFORE writing SQL. NEVER guess column names. Always use get_table_schema first.\n".to_string()
        };

        let tools_description = format!(
            "You have access to tools. Use them by outputting a JSON object:\n\
             Core (for writing SQL):\n\
             - {{\"tool\": \"list_tables\", \"database\": \"<db_name>\"}}\n\
             - {{\"tool\": \"get_table_schema\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\
             - {{\"tool\": \"cross_db_join\", \"left_table\": \"db1.table1\", \"right_table\": \"db2.table2\", \"join_type\": \"INNER JOIN\"}}\n\
             Auxiliary (use only when needed for indexes/FK/stats):\n\
             - {{\"tool\": \"get_indexes\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\
             - {{\"tool\": \"get_foreign_keys\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\
             - {{\"tool\": \"get_constraints\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\
             - {{\"tool\": \"list_views\", \"database\": \"<db_name>\"}}\n\
             - {{\"tool\": \"list_procedures\", \"database\": \"<db_name>\"}}\n\
             - {{\"tool\": \"find_relationships\", \"from_database\": \"<db>\", \"to_database\": \"<db>\"}}\n\
             - {{\"tool\": \"find_similar_columns\", \"column_pattern\": \"user_id\"}}\n\
             - {{\"tool\": \"compare_tables\", \"left\": \"db1.table1\", \"right\": \"db2.table1\"}}\n\
             - {{\"tool\": \"explain_query\", \"sql\": \"SELECT ...\"}}\n\
             - {{\"tool\": \"security_check\", \"sql\": \"SELECT ...\"}}\n\
             - {{\"tool\": \"validate_sql\", \"sql\": \"SELECT ...\"}}\n\
             - {{\"tool\": \"get_server_info\"}}\n\
             {exploration_guidance}"
        );

        let prompt = format!(
            "You are a MySQL expert. Available databases: {db_list}\n\n\
             {tools_description}\n\n\
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
        let mut tool_continued = false;

        if let Ok(tool_call) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(raw_tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                // Normalize tool name for tracking (handles aliases/typos)
                let normalized_name = match raw_tool_name {
                    "list_indexes" | "get_index" | "show_indexes" => "get_indexes",
                    "list_foreign_keys" | "get_fk" => "get_foreign_keys",
                    "show_constraints" => "get_constraints",
                    "show_views" => "list_views",
                    "show_procedures" => "list_procedures",
                    "show_triggers" => "list_triggers",
                    "get_stats" => "get_table_stats",
                    "show_table_status" => "get_table_status",
                    "get_info" => "get_server_info",
                    "explain" => "explain_query",
                    "check_security" => "security_check",
                    "validate" => "validate_sql",
                    "find_similar" => "find_similar_columns",
                    "compare" => "compare_tables",
                    "join" => "cross_db_join",
                    _ => raw_tool_name,
                };

                // Build a normalized signature for repeat detection
                let mut normalized_call = tool_call.clone();
                if let Some(obj) = normalized_call.as_object_mut() {
                    obj.insert("tool".to_string(), serde_json::json!(normalized_name));
                }
                let call_sig = normalized_call.to_string();

                if seen_tool_calls.contains(&call_sig) {
                    // Repeated call → fallback with ALL schemas for complete context
                    eprintln!("[LLM] Repeated tool call ({normalized_name}), falling back with all schemas");
                    let schema_context = if has_cross_db_relations {
                        schema::format_schemas_with_relationships(all_schemas, &cross_db_relations)
                    } else {
                        schema::format_all_schemas_for_prompt(all_schemas)
                    };
                    let fallback_prompt = format!(
                        "You are a MySQL 5.6+ expert. Given the database schema below, \
                         write a SQL query to answer the user's question.\n\
                         Only return the SQL query, no explanations, no markdown.\n\n\
                         {schema_context}\n\n\
                         User's question: {natural_language}\n\n\
                         SQL Query:",
                    );
                    return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await.map(|sql| (sql, tool_steps, (iteration + 1) as u32));
                }
                seen_tool_calls.push(call_sig);

                let (maybe_sql, mut step) = process_tool_call(
                    normalized_name,
                    &tool_call,
                    all_schemas,
                    &db_list,
                    &cross_db_relations,
                    &mut conversation,
                    natural_language,
                    &url,
                    &model,
                    &client,
                    &api_url,
                ).await?;
                step.iteration = (iteration + 1) as u32;
                tool_steps.push(step);

                if let Some(sql) = maybe_sql {
                    eprintln!("[LLM] Tool returned SQL directly");
                    return Ok((sql, tool_steps, iteration as u32 + 1));
                }
                tool_continued = true;

                // Track which tables have been explored
                if normalized_name == "get_table_schema" {
                    if let Some(db) = tool_call.get("database").and_then(|v| v.as_str()) {
                        if let Some(table) = tool_call.get("table").and_then(|v| v.as_str()) {
                            schemas_explored.insert(format!("{db}.{table}"));
                            eprintln!("[LLM] Explored schemas: {:?}", schemas_explored);
                        }
                    }
                }
            }
        }

        // If not parsed as whole JSON, try extracting from mixed content
        if !tool_continued {
            if let Some(tool_call) = extract_first_json_object(&text) {
                if let Some(raw_tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                    let normalized_name = match raw_tool_name {
                        "list_indexes" | "get_index" | "show_indexes" => "get_indexes",
                        "list_foreign_keys" | "get_fk" => "get_foreign_keys",
                        "show_constraints" => "get_constraints",
                        "show_views" => "list_views",
                        "show_procedures" => "list_procedures",
                        "show_triggers" => "list_triggers",
                        "get_stats" => "get_table_stats",
                        "show_table_status" => "get_table_status",
                        "get_info" => "get_server_info",
                        "explain" => "explain_query",
                        "check_security" => "security_check",
                        "validate" => "validate_sql",
                        "find_similar" => "find_similar_columns",
                        "compare" => "compare_tables",
                        "join" => "cross_db_join",
                        _ => raw_tool_name,
                    };

                    let mut normalized_call = tool_call.clone();
                    if let Some(obj) = normalized_call.as_object_mut() {
                        obj.insert("tool".to_string(), serde_json::json!(normalized_name));
                    }
                    let call_sig = normalized_call.to_string();

                    if seen_tool_calls.contains(&call_sig) {
                        eprintln!("[LLM] Repeated tool call (extracted, {normalized_name}), falling back with all schemas");
                        let schema_context = if has_cross_db_relations {
                            schema::format_schemas_with_relationships(all_schemas, &cross_db_relations)
                        } else {
                            schema::format_all_schemas_for_prompt(all_schemas)
                        };
                        let fallback_prompt = format!(
                            "You are a MySQL 5.6+ expert. Given the database schema below, \
                             write a SQL query to answer the user's question.\n\
                             Only return the SQL query, no explanations, no markdown.\n\n\
                             {schema_context}\n\n\
                             User's question: {natural_language}\n\n\
                             SQL Query:",
                        );
                        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await.map(|sql| (sql, tool_steps.clone(), (iteration + 1) as u32));
                    }
                    seen_tool_calls.push(call_sig);

                    let (maybe_sql, mut step) = process_tool_call(
                        normalized_name,
                        &tool_call,
                        all_schemas,
                        &db_list,
                        &cross_db_relations,
                        &mut conversation,
                        natural_language,
                        &url,
                        &model,
                        &client,
                        &api_url,
                    ).await?;
                    step.iteration = (iteration + 1) as u32;
                    tool_steps.push(step);

                    if let Some(sql) = maybe_sql {
                        eprintln!("[LLM] Tool returned SQL directly");
                        return Ok((sql, tool_steps, iteration as u32 + 1));
                    }
                    tool_continued = true;

                    // Track which tables have been explored
                    if normalized_name == "get_table_schema" {
                        if let Some(db) = tool_call.get("database").and_then(|v| v.as_str()) {
                            if let Some(table) = tool_call.get("table").and_then(|v| v.as_str()) {
                                schemas_explored.insert(format!("{db}.{table}"));
                            }
                        }
                    }
                }
            }
        }

        if tool_continued {
            continue;
        }

        // Not a tool call — extract SQL
        let sql = extract_sql(&text);

        if !sql.is_empty() && !sql.starts_with('{') && !sql.contains("\"tool\":") {
            eprintln!("[LLM] Final SQL: {}", sql.chars().take(100).collect::<String>());

            // Validate SQL columns against cached schema if schemas weren't explored
            if schemas_explored.is_empty() && !all_schemas.is_empty() {
                match validate_sql_columns(&sql, all_schemas) {
                    ValidationResult::Valid => {
                        return Ok((sql, tool_steps.clone(), (iteration + 1) as u32));
                    }
                    ValidationResult::InvalidColumns { table, bad_columns } => {
                        eprintln!("[LLM] SQL validation failed: table '{}' has no columns: {:?}", table, bad_columns);
                        // Fall back with full schema context
                        let schema_context = if has_cross_db_relations {
                            schema::format_schemas_with_relationships(all_schemas, &cross_db_relations)
                        } else {
                            schema::format_all_schemas_for_prompt(all_schemas)
                        };
                        let fallback_prompt = format!(
                            "You are a MySQL 5.6+ expert. Your previous query used column names that don't exist.\n\
                             Given the database schema below, write a CORRECT SQL query.\n\
                             ONLY use column names shown in the schema below.\n\
                             Only return the SQL query, no explanations, no markdown.\n\n\
                             {schema_context}\n\n\
                             User's question: {natural_language}\n\n\
                             SQL Query:",
                        );
                        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await.map(|sql| (sql, tool_steps.clone(), (iteration + 1) as u32));
                    }
                    ValidationResult::TableNotFound => {
                        // Table not in cache, let it through (might be valid)
                        return Ok((sql, tool_steps.clone(), (iteration + 1) as u32));
                    }
                }
            }

            return Ok((sql, tool_steps.clone(), (iteration + 1) as u32));
        }

        // No valid SQL — fallback with ALL schemas
        eprintln!("[LLM] No valid SQL from tool iteration, falling back with all schemas");
        let schema_context = if has_cross_db_relations {
            schema::format_schemas_with_relationships(all_schemas, &cross_db_relations)
        } else {
            schema::format_all_schemas_for_prompt(all_schemas)
        };
        let fallback_prompt = format!(
            "You are a MySQL 5.6+ expert. Given the database schema below, \
             write a SQL query to answer the user's question.\n\
             Only return the SQL query, no explanations, no markdown.\n\n\
             {schema_context}\n\n\
             User's question: {natural_language}\n\n\
             SQL Query:",
        );
        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await.map(|sql| (sql, tool_steps.clone(), (iteration + 1) as u32));
    }

    // Max iterations → fallback with all schemas
    eprintln!("[LLM] Max iterations reached, using all schemas for fallback");
    let schema_context = if has_cross_db_relations {
        schema::format_schemas_with_relationships(all_schemas, &cross_db_relations)
    } else {
        schema::format_all_schemas_for_prompt(all_schemas)
    };
    let sql = natural_language_to_sql(natural_language, &schema_context).await?;
    Ok((sql, tool_steps, max_iterations as u32))
}

async fn call_ollama_generate(
    _url: &str,
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

async fn process_tool_call(
    tool_name: &str,
    tool_call: &serde_json::Value,
    all_schemas: &[Schema],
    db_list: &str,
    cross_db_relations: &[schema::ForeignKeyRelation],
    conversation: &mut String,
    _natural_language: &str,
    _url: &str,
    _model: &str,
    _client: &reqwest::Client,
    _api_url: &str,
) -> Result<(Option<String>, ToolCallStep), AppError> {
    eprintln!("[LLM] Tool call: {tool_name}");

    // Handle common tool name aliases/typos from LLM
    let tool_name = match tool_name {
        "list_indexes" => "get_indexes",
        "get_index" => "get_indexes",
        "list_foreign_keys" => "get_foreign_keys",
        "get_fk" => "get_foreign_keys",
        "show_indexes" => "get_indexes",
        "show_constraints" => "get_constraints",
        "show_views" => "list_views",
        "show_procedures" => "list_procedures",
        "show_triggers" => "list_triggers",
        "get_stats" => "get_table_stats",
        "show_table_status" => "get_table_status",
        "get_info" => "get_server_info",
        "explain" => "explain_query",
        "check_security" => "security_check",
        "validate" => "validate_sql",
        "find_similar" => "find_similar_columns",
        "compare" => "compare_tables",
        "join" => "cross_db_join",
        _ => tool_name,
    };

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

            // Actually query the database for sample data
            match crate::db::connection::get_pool().await {
                Ok(pool) => {
                    match pool.get_conn().await {
                        Ok(mut conn) => {
                            let query = format!("SELECT * FROM `{database}`.`{table}` LIMIT 3");
                            let query_result: std::result::Result<Vec<mysql_async::Row>, _> = conn.query(&query).await;
                            match query_result {
                                Ok(rows) => {
                                    if rows.is_empty() {
                                        let result = format!("Table {database}.{table} exists but has no data.");
                                        conversation.push_str(&format!("\n\nTool result: {result}"));
                                    } else {
                                        let cols = rows[0].columns();
                                        let col_names: Vec<String> = cols.as_ref().iter().map(|c| c.name_str().to_string()).collect();
                                        let sample_rows: Vec<String> = rows.iter().take(3).map(|row| {
                                            let vals: Vec<String> = col_names.iter().enumerate().map(|(i, _)| {
                                                match row.get_opt::<mysql_async::Value, usize>(i) {
                                                    Some(Ok(v)) => match v {
                                                        mysql_async::Value::NULL => "NULL".to_string(),
                                                        mysql_async::Value::Bytes(b) => String::from_utf8_lossy(&b).chars().take(30).collect::<String>(),
                                                        mysql_async::Value::Int(v) => v.to_string(),
                                                        mysql_async::Value::UInt(v) => v.to_string(),
                                                        mysql_async::Value::Float(v) => v.to_string(),
                                                        mysql_async::Value::Double(v) => v.to_string(),
                                                        _ => "?".to_string(),
                                                    },
                                                    _ => "NULL".to_string(),
                                                }
                                            }).collect();
                                            format!("  ({})", vals.join(", "))
                                        }).collect();
                                        let result = format!(
                                            "Sample from {database}.{table} (columns: {}):\n{}\n{} rows shown.",
                                            col_names.join(", "), sample_rows.join("\n"), rows.len()
                                        );
                                        conversation.push_str(&format!("\n\nTool result:\n{result}"));
                                        eprintln!("[LLM] Sample: {} rows from {}.{}", rows.len(), database, table);
                                    }
                                }
                                Err(e) => {
                                    conversation.push_str(&format!("\n\nTool result: Query error on {database}.{table}: {e}"));
                                }
                            }
                        }
                        Err(e) => {
                            conversation.push_str(&format!("\n\nTool result: Connection error: {e}"));
                        }
                    }
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool result: Not connected: {e}"));
                }
            }
        }
        "cross_db_join" => {
            let left_table = tool_call.get("left_table").and_then(|v| v.as_str()).unwrap_or("");
            let right_table = tool_call.get("right_table").and_then(|v| v.as_str()).unwrap_or("");
            let join_type = tool_call.get("join_type").and_then(|v| v.as_str()).unwrap_or("INNER JOIN");

            // Parse database.table format
            let left_parts: Vec<&str> = left_table.splitn(2, '.').collect();
            let right_parts: Vec<&str> = right_table.splitn(2, '.').collect();

            if left_parts.len() != 2 || right_parts.len() != 2 {
                let result = format!(
                    "Error: Tables must be in database.table format. Got: '{}' and '{}'",
                    left_table, right_table
                );
                conversation.push_str(&format!("\n\nTool result: {result}"));
                eprintln!("[LLM] {result}");
            } else {
                let (left_db, left_tbl) = (left_parts[0], left_parts[1]);
                let (right_db, right_tbl) = (right_parts[0], right_parts[1]);

                // Validate tables exist in schemas
                let left_schema = all_schemas.iter().find(|s| s.database == left_db);
                let right_schema = all_schemas.iter().find(|s| s.database == right_db);

                match (left_schema, right_schema) {
                    (Some(ls), Some(rs)) => {
                        let left_exists = ls.tables.iter().any(|t| t.name == left_tbl);
                        let right_exists = rs.tables.iter().any(|t| t.name == right_tbl);

                        if !left_exists || !right_exists {
                            let mut missing = Vec::new();
                            if !left_exists { missing.push(format!("{}.{}", left_db, left_tbl)); }
                            if !right_exists { missing.push(format!("{}.{}", right_db, right_tbl)); }
                            let result = format!("Table(s) not found: {}", missing.join(", "));
                            conversation.push_str(&format!("\n\nTool result: {result}"));
                            eprintln!("[LLM] {result}");
                        } else {
                            // Find potential join columns
                            let left_table_info = ls.tables.iter().find(|t| t.name == left_tbl).unwrap();
                            let right_table_info = rs.tables.iter().find(|t| t.name == right_tbl).unwrap();

                            // Look for matching column names (potential join keys)
                            let left_cols: Vec<_> = left_table_info.columns.iter().map(|c| &c.name).collect();
                            let right_cols: Vec<_> = right_table_info.columns.iter().map(|c| &c.name).collect();
                            let matching_cols: Vec<_> = left_cols.iter()
                                .filter(|c| right_cols.contains(c))
                                .map(|c| c.as_str())
                                .collect();

                            // Look for FK relations between these tables
                            let fk_relations: Vec<_> = cross_db_relations
                                .iter()
                                .filter(|r| {
                                    (r.from_database == left_db && r.from_table == left_tbl && r.to_database == right_db && r.to_table == right_tbl)
                                        || (r.from_database == right_db && r.from_table == right_tbl && r.to_database == left_db && r.to_table == left_tbl)
                                })
                                .collect();

                            let join_suggestions = if !fk_relations.is_empty() {
                                // Use FK relations for join conditions
                                let conditions: Vec<String> = fk_relations.iter().map(|r| {
                                    if r.from_database == left_db {
                                        format!("{}.{} = {}.{}", left_table, r.from_column, right_table, r.to_column)
                                    } else {
                                        format!("{}.{} = {}.{}", left_table, r.to_column, right_table, r.from_column)
                                    }
                                }).collect();
                                conditions.join(" AND ")
                            } else if !matching_cols.is_empty() {
                                // Use matching column names
                                matching_cols.iter().map(|c| {
                                    format!("{}.{} = {}.{}", left_table, c, right_table, c)
                                }).collect::<Vec<_>>().join(" AND ")
                            } else {
                                format!("-- No obvious join condition found. Specify join columns manually.\n-- Example: {}.id = {}.foreign_id", left_table, right_table)
                            };

                            let sql = format!(
                                "SELECT *\nFROM {}\n{} {}\n  ON {};",
                                left_table, join_type, right_table, join_suggestions
                            );

                            let result = format!(
                                "Generated JOIN SQL:\n{}",
                                sql
                            );
                            conversation.push_str(&format!("\n\nTool result:\n{result}"));
                            eprintln!("[LLM] {}", result.chars().take(200).collect::<String>());

                            // Return SQL directly if it's a clean join (no comment about missing columns)
                            if !join_suggestions.starts_with("--") {
                                let mut params = HashMap::new();
                                params.insert("left_table".to_string(), left_table.to_string());
                                params.insert("right_table".to_string(), right_table.to_string());
                                params.insert("join_type".to_string(), join_type.to_string());
                                return Ok((Some(sql), ToolCallStep {
                                    tool_name: tool_name.to_string(),
                                    parameters: params,
                                    result: result.chars().take(500).collect(),
                                    iteration: 0,
                                }));
                            }
                        }
                    }
                    _ => {
                        let missing_dbs = match (left_schema, right_schema) {
                            (None, Some(_)) => left_db.to_string(),
                            (Some(_), None) => right_db.to_string(),
                            (None, None) => format!("{} and {}", left_db, right_db),
                            _ => unreachable!(),
                        };
                        let result = format!("Database(s) not cached: {}. Available: {}", missing_dbs, db_list);
                        conversation.push_str(&format!("\n\nTool result: {result}"));
                        eprintln!("[LLM] {result}");
                    }
                }
            }
        }

        "get_indexes" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_table_indexes(database, table).await {
                Ok(indexes) if indexes.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No indexes on {database}.{table}"));
                }
                Ok(indexes) => {
                    let lines: Vec<String> = indexes.iter().map(|i| {
                        format!("  {} ({}): {} [{}]", i.name, if i.non_unique { "NON-UNIQUE" } else { "UNIQUE" }, i.column, i.index_type)
                    }).collect();
                    let result = format!("Indexes on {database}.{table}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_foreign_keys" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_table_foreign_keys(database, table).await {
                Ok(fks) if fks.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No foreign keys on {database}.{table}"));
                }
                Ok(fks) => {
                    let lines: Vec<String> = fks.iter().map(|f| {
                        format!("  {}.{} → {}.{} [{}]", f.to_database, f.to_table, f.to_column, f.from_column, f.constraint_name.as_deref().unwrap_or("unnamed"))
                    }).collect();
                    let result = format!("Foreign keys on {database}.{table}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_constraints" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_table_constraints(database, table).await {
                Ok(constraints) if constraints.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No constraints on {database}.{table}"));
                }
                Ok(constraints) => {
                    let lines: Vec<String> = constraints.iter().map(|c| {
                        format!("  {} ({}) on {}", c.constraint_type, c.name, c.column)
                    }).collect();
                    let result = format!("Constraints on {database}.{table}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "list_views" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_database_views(database).await {
                Ok(views) if views.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No views in {database}"));
                }
                Ok(views) => {
                    let names: Vec<&str> = views.iter().map(|v| v.name.as_str()).collect();
                    let result = format!("Views in {database}: {}", names.join(", "));
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "list_procedures" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_database_procedures(database).await {
                Ok(procs) if procs.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No procedures in {database}"));
                }
                Ok(procs) => {
                    let names: Vec<&str> = procs.iter().map(|p| p.name.as_str()).collect();
                    let result = format!("Procedures in {database}: {}", names.join(", "));
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "list_triggers" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_database_triggers(database).await {
                Ok(triggers) if triggers.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No triggers in {database}"));
                }
                Ok(triggers) => {
                    let lines: Vec<String> = triggers.iter().map(|t| {
                        format!("  {} on {database}.{} [{} {}]", t.name, t.table, t.timing, t.event)
                    }).collect();
                    let result = format!("Triggers in {database}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_table_stats" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_table_statistics(database, table).await {
                Ok(stats) => {
                    let result = format!(
                        "Stats for {database}.{table}: {} rows, {} MB data, {} MB indexes, {} avg row bytes",
                        stats.row_count, stats.data_size_mb, stats.index_size_mb, stats.avg_row_length
                    );
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_table_status" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_table_status(database, table).await {
                Ok(status) => {
                    let result = format!(
                        "Status for {database}.{table}: engine={}, row_format={}, collation={}, auto_increment={}",
                        status.engine, status.row_format, status.collation,
                        status.auto_increment.map(|v| v.to_string()).unwrap_or("none".to_string())
                    );
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "find_relationships" => {
            let from_db = tool_call.get("from_database").and_then(|v| v.as_str()).unwrap_or("");
            let to_db = tool_call.get("to_database").and_then(|v| v.as_str()).unwrap_or("");

            // Try actual FK first
            let mut fk_relations = Vec::new();
            if !from_db.is_empty() {
                if let Ok(rels) = schema::introspect_foreign_keys(from_db).await {
                    fk_relations.extend(rels);
                }
            }
            if !to_db.is_empty() && to_db != from_db {
                if let Ok(rels) = schema::introspect_foreign_keys(to_db).await {
                    fk_relations.extend(rels);
                }
            }

            // Filter and format
            let relevant: Vec<_> = fk_relations.iter()
                .filter(|r| {
                    (from_db.is_empty() || r.from_database == from_db || r.to_database == from_db)
                        && (to_db.is_empty() || r.from_database == to_db || r.to_database == to_db)
                })
                .collect();

            if relevant.is_empty() {
                conversation.push_str(&format!("\n\nTool result: No relationships found between '{}' and '{}'. Available: {}", from_db, to_db, db_list));
            } else {
                let lines: Vec<String> = relevant.iter().map(|r| {
                    format!("{}.{}.{} → {}.{}.{}", r.from_database, r.from_table, r.from_column, r.to_database, r.to_table, r.to_column)
                }).collect();
                conversation.push_str(&format!("\n\nTool result:\n{}", lines.join("\n")));
            }
        }

        "find_similar_columns" => {
            let pattern = tool_call.get("column_pattern").and_then(|v| v.as_str()).unwrap_or("");
            match schema::find_similar_columns(pattern).await {
                Ok(locations) if locations.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No columns matching '{}'", pattern));
                }
                Ok(locations) => {
                    let lines: Vec<String> = locations.iter().take(50).map(|l| {
                        let key = if !l.column_key.is_empty() { format!(" [{}]", l.column_key) } else { "".to_string() };
                        format!("  {}.{}.{} {}{}", l.database, l.table, l.column, l.column_type, key)
                    }).collect();
                    let total_note = if locations.len() > 50 {
                        format!("\n... and {} more", locations.len() - 50)
                    } else { "".to_string() };
                    let result = format!("Columns matching '{}':\n{}{}", pattern, lines.join("\n"), total_note);
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "compare_tables" => {
            let left = tool_call.get("left").and_then(|v| v.as_str()).unwrap_or("");
            let right = tool_call.get("right").and_then(|v| v.as_str()).unwrap_or("");

            let left_parts: Vec<&str> = left.splitn(2, '.').collect();
            let right_parts: Vec<&str> = right.splitn(2, '.').collect();

            if left_parts.len() != 2 || right_parts.len() != 2 {
                conversation.push_str(&format!("\n\nTool error: Tables must be in database.table format. Got: '{left}' and '{right}'"));
            } else {
                let left_schema = all_schemas.iter().find(|s| s.database == left_parts[0]).and_then(|s| s.tables.iter().find(|t| t.name == left_parts[1]));
                let right_schema = all_schemas.iter().find(|s| s.database == right_parts[0]).and_then(|s| s.tables.iter().find(|t| t.name == right_parts[1]));

                let comp = schema::compare_tables(left_schema, right_schema);

                let lines = vec![
                    format!("Common columns ({}): {}", comp.common.len(), comp.common.join(", ")),
                    format!("Only in {left} ({}): {}", comp.left_only.len(), comp.left_only.join(", ")),
                    format!("Only in {right} ({}): {}", comp.right_only.len(), comp.right_only.join(", ")),
                ];

                let type_mismatch = if !comp.type_mismatches.is_empty() {
                    let mismatch_lines: Vec<String> = comp.type_mismatches.iter().map(|(c, l, r)| {
                        format!("  {}: {} vs {}", c, l, r)
                    }).collect();
                    format!("Type mismatches:\n{}", mismatch_lines.join("\n"))
                } else { "".to_string() };

                let result = format!("Compare {left} vs {right}:\n{}\n{}", lines.join("\n"), type_mismatch);
                conversation.push_str(&format!("\n\nTool result:\n{result}"));
            }
        }

        "explain_query" => {
            let sql = tool_call.get("sql").and_then(|v| v.as_str()).unwrap_or("");
            if sql.trim().is_empty() {
                conversation.push_str(&format!("\n\nTool error: sql parameter is required"));
            } else {
                match crate::db::connection::get_pool().await {
                    Ok(pool) => {
                        match pool.get_conn().await {
                            Ok(mut conn) => {
                                let explain_sql = format!("EXPLAIN {}", sql);
                                match conn.query::<mysql_async::Row, _>(&explain_sql).await {
                                    Ok(rows) if !rows.is_empty() => {
                                        let cols = rows[0].columns();
                                        let col_names: Vec<String> = cols.iter().map(|c| c.name_str().to_string()).collect();
                                        let formatted: Vec<String> = rows.iter().take(5).map(|row| {
                                            let vals: Vec<String> = col_names.iter().enumerate().map(|(i, _)| {
                                                row.get_opt::<mysql_async::Value, usize>(i)
                                                    .map(|v| match v {
                                                        Ok(mysql_async::Value::NULL) => "NULL".to_string(),
                                                        Ok(mysql_async::Value::Bytes(b)) => String::from_utf8_lossy(&b).to_string(),
                                                        Ok(mysql_async::Value::Int(v)) => v.to_string(),
                                                        Ok(mysql_async::Value::UInt(v)) => v.to_string(),
                                                        _ => "NULL".to_string(),
                                                    })
                                                    .unwrap_or("NULL".to_string())
                                            }).collect();
                                            vals.join(" | ")
                                        }).collect();
                                        let result = format!(
                                            "EXPLAIN for query (columns: {}):\n{}",
                                            col_names.join(", "),
                                            formatted.join("\n")
                                        );
                                        conversation.push_str(&format!("\n\nTool result:\n{result}"));
                                    }
                                    Ok(_) => {
                                        conversation.push_str(&format!("\n\nTool result: EXPLAIN returned no rows (query may be invalid)"));
                                    }
                                    Err(e) => {
                                        conversation.push_str(&format!("\n\nTool result: EXPLAIN error: {}", e));
                                    }
                                }
                            }
                            Err(e) => {
                                conversation.push_str(&format!("\n\nTool result: Connection error: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool result: Not connected: {e}"));
                    }
                }
            }
        }

        "security_check" => {
            let sql = tool_call.get("sql").and_then(|v| v.as_str()).unwrap_or("");
            if sql.trim().is_empty() {
                conversation.push_str(&format!("\n\nTool error: sql parameter is required"));
            } else {
                let upper = sql.to_uppercase();
                let mut issues: Vec<String> = Vec::new();

                // Check for SELECT *
                if upper.contains("SELECT *") || upper.contains("SELECT  *") {
                    issues.push("WARNING: SELECT * - specify columns explicitly for better performance and security".to_string());
                }

                // Check for UPDATE/DELETE without WHERE
                if (upper.starts_with("UPDATE") || upper.contains(" UPDATE ")) && !upper.contains("WHERE") {
                    issues.push("CRITICAL: UPDATE without WHERE clause - will modify ALL rows".to_string());
                }
                if (upper.starts_with("DELETE") || upper.contains(" DELETE ")) && !upper.contains("WHERE") && !upper.contains("TRUNCATE") {
                    issues.push("CRITICAL: DELETE without WHERE clause - will delete ALL rows".to_string());
                }

                // Check for UNION
                if upper.contains("UNION") {
                    issues.push("INFO: UNION detected - verify this is intentional, not an injection vector".to_string());
                }

                // Check for string concatenation patterns (potential injection)
                if sql.contains("CONCAT(") || sql.contains("||") {
                    issues.push("INFO: String concatenation detected - ensure input sanitization".to_string());
                }

                // Check for DROP/TRUNCATE
                if upper.contains("DROP ") || upper.contains("TRUNCATE ") {
                    issues.push("CRITICAL: Destructive operation (DROP/TRUNCATE) detected - irreversible data loss".to_string());
                }

                // Check for implicit cross join
                if upper.contains("FROM") && upper.contains("WHERE") && !upper.contains("JOIN") {
                    let from_idx = upper.find("FROM").unwrap();
                    let where_idx = upper.find("WHERE").unwrap();
                    let between = &upper[from_idx..where_idx];
                    if between.contains(',') {
                        issues.push("WARNING: Implicit cross join (comma-separated tables) - use explicit JOIN syntax".to_string());
                    }
                }

                if issues.is_empty() {
                    conversation.push_str(&format!("\n\nTool result: SECURITY OK - no issues found"));
                } else {
                    conversation.push_str(&format!("\n\nTool result:\n{}", issues.join("\n")));
                }
            }
        }

        "validate_sql" => {
            let sql = tool_call.get("sql").and_then(|v| v.as_str()).unwrap_or("");
            if sql.trim().is_empty() {
                conversation.push_str(&format!("\n\nTool error: sql parameter is required"));
            } else {
                match crate::db::connection::get_pool().await {
                    Ok(pool) => {
                        match pool.get_conn().await {
                            Ok(mut conn) => {
                                let explain_sql = format!("EXPLAIN {}", sql);
                                match conn.query::<mysql_async::Row, _>(&explain_sql).await {
                                    Ok(_) => {
                                        conversation.push_str(&format!("\n\nTool result: SQL is valid - syntax OK"));
                                    }
                                    Err(e) => {
                                        let error_msg = e.to_string();
                                        conversation.push_str(&format!("\n\nTool result: SQL INVALID - {}", error_msg.chars().take(200).collect::<String>()));
                                    }
                                }
                            }
                            Err(e) => {
                                conversation.push_str(&format!("\n\nTool result: Connection error: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool result: Not connected: {e}"));
                    }
                }
            }
        }

        "get_server_info" => {
            match schema::get_server_info().await {
                Ok(info) => {
                    let result = format!(
                        "Server: MySQL {} | User: {} | DB: {} | Charset: {} ({}) | TZ: {} | Max conns: {}",
                        info.version, info.current_user, info.current_database,
                        info.character_set, info.collation, info.timezone, info.max_connections
                    );
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        _ => {
            conversation.push_str(&format!(
                "\n\nTool error: Unknown tool '{tool_name}'.\n\
                 Available tools: list_tables, get_table_schema, get_sample_data, get_indexes, \
                 get_foreign_keys, get_constraints, list_views, list_procedures, list_triggers, \
                 get_table_stats, get_table_status, find_relationships, find_similar_columns, \
                 compare_tables, cross_db_join, explain_query, security_check, validate_sql, \
                 get_server_info"
            ));
        }
    }

    // Extract the result from the conversation (last appended part)
    let result_text = conversation
        .rsplit_once("\n\nTool result:")
        .or_else(|| conversation.rsplit_once("\n\nTool error:"))
        .map(|(_, rest)| rest.trim().chars().take(500).collect())
        .unwrap_or_else(|| "No result".to_string());

    // Build parameters map
    let mut params = HashMap::new();
    if let Some(obj) = tool_call.as_object() {
        for (k, v) in obj {
            if k != "tool" {
                params.insert(k.clone(), v.as_str().unwrap_or("").to_string());
            }
        }
    }

    Ok((None, ToolCallStep {
        tool_name: tool_name.to_string(),
        parameters: params,
        result: result_text,
        iteration: 0, // Will be set by caller
    }))
}

/// Result of SQL column validation against cached schema
enum ValidationResult {
    Valid,
    InvalidColumns { table: String, bad_columns: Vec<String> },
    #[allow(dead_code)]
    TableNotFound,
}

/// Validates that columns referenced in a SQL query exist in the cached schema.
/// Only checks simple patterns: table.column and standalone column names in SELECT.
fn validate_sql_columns(sql: &str, all_schemas: &[Schema]) -> ValidationResult {
    // Build a lookup: database.table -> set of column names
    let mut table_columns: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();

    for schema in all_schemas {
        for table in &schema.tables {
            let key = format!("{}.{}", schema.database, table.name);
            let cols: std::collections::HashSet<String> = table.columns.iter()
                .map(|c| c.name.clone())
                .collect();
            table_columns.insert(key, cols);
        }
    }

    // Extract table aliases: e.g., "claim AS c" -> {"c": "doga_claim.claim"}
    let mut alias_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Find FROM and JOIN clauses to identify table aliases
    let sql_upper = sql.to_uppercase();
    
    // Simple regex-like extraction for "FROM database.table AS alias" or "JOIN database.table AS alias"
    for (db, tbl) in extract_table_references(sql) {
        let full_name = format!("{}.{}", db, tbl);
        if table_columns.contains_key(&full_name) {
            // Check for aliases: AS alias
            let patterns = [
                format!("FROM {} AS", full_name),
                format!("JOIN {} AS", full_name),
                format!("FROM {} AS", tbl),
                format!("JOIN {} AS", tbl),
            ];
            for pattern in &patterns {
                if let Some(idx) = sql_upper.find(&pattern.to_uppercase()) {
                    let after = &sql[idx + pattern.len()..];
                    if let Some(alias_end) = after.find(|c: char| !c.is_alphabetic() && c != '_') {
                        let alias = after[..alias_end].trim().to_lowercase();
                        if !alias.is_empty() {
                            alias_map.insert(alias, full_name.clone());
                        }
                    }
                }
            }
            // Also register the short table name as an alias
            alias_map.entry(tbl.to_lowercase()).or_insert_with(|| full_name.clone());
        }
    }

    // Extract column references from SELECT clause
    let select_start = match sql_upper.find("SELECT") {
        Some(i) => i,
        None => return ValidationResult::Valid,
    };
    let from_start = match sql_upper.find("FROM") {
        Some(i) => i,
        None => return ValidationResult::Valid,
    };

    let select_clause = &sql[select_start + 6..from_start];

    // Parse individual column expressions (handle commas but skip subqueries)
    let mut bad_columns: Vec<String> = Vec::new();
    let mut checked_table = String::new();

    for col_expr in select_clause.split(',') {
        let col_expr = col_expr.trim();
        if col_expr == "*" || col_expr.starts_with('*') {
            continue;
        }

        // Extract column reference (before AS alias if any)
        let col_ref = if let Some(as_idx) = col_expr.to_uppercase().find(" AS ") {
            &col_expr[..as_idx]
        } else {
            col_expr
        }.trim();

        // Check if it's table.column format
        if let Some(dot_pos) = col_ref.find('.') {
            let table_part = col_ref[..dot_pos].trim().to_lowercase();
            let col_part = col_ref[dot_pos + 1..].trim();

            let full_table = alias_map.get(&table_part)
                .cloned()
                .or_else(|| table_columns.keys()
                    .find(|k| k.to_lowercase().ends_with(&format!(".{}", table_part)))
                    .cloned());

            if let Some(full_table_name) = full_table {
                checked_table = full_table_name.clone();
                if let Some(cols) = table_columns.get(&full_table_name) {
                    if !cols.contains(col_part) {
                        bad_columns.push(col_part.to_string());
                    }
                }
            }
        } else {
            // Standalone column name - could be ambiguous, skip validation
            continue;
        }
    }

    if !bad_columns.is_empty() {
        return ValidationResult::InvalidColumns {
            table: checked_table,
            bad_columns,
        };
    }

    ValidationResult::Valid
}

/// Extract table references (database, table) from a SQL query
fn extract_table_references(sql: &str) -> Vec<(String, String)> {
    let mut refs = Vec::new();

    for keyword in ["FROM", "JOIN"] {
        let mut current = sql;
        while let Some(idx) = current.to_uppercase().find(keyword) {
            let after = &current[idx + keyword.len()..];
            let trimmed = after.trim_start();
            let end = trimmed.find(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == '\n')
                .unwrap_or(trimmed.len());
            let table_ref = trimmed[..end].trim();

            let parts: Vec<&str> = table_ref.split('.').collect();
            if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
                let entry = (parts[0].to_string(), parts[1].to_string());
                if !refs.contains(&entry) {
                    refs.push(entry);
                }
            }
            current = &current[idx + keyword.len()..];
        }
    }

    refs
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
