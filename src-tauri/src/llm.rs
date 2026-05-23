use mysql_async::prelude::Queryable;
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

    for iteration in 0..max_iterations {
        eprintln!("[LLM] Tool iteration {}/{}", iteration + 1, max_iterations);

        let exploration_guidance = if schemas_explored.len() >= 2 {
            "You have schema for 2+ tables. Write the SQL query NOW. Do NOT call list_tables or more get_table_schema calls.\n"
        } else {
            "RULE: You MUST call get_table_schema for each table BEFORE writing SQL. NEVER guess column names. Always use get_table_schema first.\n"
        };

        let tools_description = if has_cross_db_relations {
            format!(
                "You have access to tools. Use them by outputting a JSON object:\n\
                 - {{\"tool\": \"list_tables\", \"database\": \"<db_name>\"}}\n\
                 - {{\"tool\": \"get_table_schema\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\
                 - {{\"tool\": \"cross_db_join\", \"left_table\": \"db1.table1\", \"right_table\": \"db2.table2\", \"join_type\": \"INNER JOIN\"}}\n\n\
                 For cross-database JOINs, use the format: database.table\n\
                 Known relationships exist between some tables in different databases.\n\
                 {exploration_guidance}"
            )
        } else {
            format!(
                "You have access to tools. Use them by outputting a JSON object:\n\
                 - {{\"tool\": \"list_tables\", \"database\": \"<db_name>\"}}\n\
                 - {{\"tool\": \"get_table_schema\", \"database\": \"<db_name>\", \"table\": \"<table_name>\"}}\n\n\
                 For cross-database JOINs, use the format: database.table\n\
                 {exploration_guidance}"
            )
        };

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
            if let Some(tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                let call_sig = tool_call.to_string();
                if seen_tool_calls.contains(&call_sig) {
                    // Repeated call → fallback with ALL schemas for complete context
                    eprintln!("[LLM] Repeated tool call, falling back with all schemas");
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
                    return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await;
                }
                seen_tool_calls.push(call_sig);

                if let Some(sql) = process_tool_call(
                    tool_name,
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
                ).await? {
                    eprintln!("[LLM] Tool returned SQL directly");
                    return Ok(sql);
                }
                tool_continued = true;

                // Track which tables have been explored
                if tool_name == "get_table_schema" {
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
                if let Some(tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                    let call_sig = tool_call.to_string();
                    if seen_tool_calls.contains(&call_sig) {
                        eprintln!("[LLM] Repeated tool call (extracted), falling back with all schemas");
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
                        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await;
                    }
                    seen_tool_calls.push(call_sig);

                    if let Some(sql) = process_tool_call(
                        tool_name,
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
                    ).await? {
                        eprintln!("[LLM] Tool returned SQL directly");
                        return Ok(sql);
                    }
                    tool_continued = true;

                    // Track which tables have been explored
                    if tool_name == "get_table_schema" {
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
                        return Ok(sql);
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
                        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await;
                    }
                    ValidationResult::TableNotFound => {
                        // Table not in cache, let it through (might be valid)
                        return Ok(sql);
                    }
                }
            }

            return Ok(sql);
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
        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt).await;
    }

    // Max iterations → fallback with all schemas
    eprintln!("[LLM] Max iterations reached, using all schemas for fallback");
    let schema_context = if has_cross_db_relations {
        schema::format_schemas_with_relationships(all_schemas, &cross_db_relations)
    } else {
        schema::format_all_schemas_for_prompt(all_schemas)
    };
    natural_language_to_sql(natural_language, &schema_context).await
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
) -> Result<Option<String>, AppError> {
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
        "find_relationships" => {
            let from_db = tool_call.get("from_database").and_then(|v| v.as_str()).unwrap_or("");
            let to_db = tool_call.get("to_database").and_then(|v| v.as_str()).unwrap_or("");

            // Filter relations for the requested databases
            let relevant_relations: Vec<_> = cross_db_relations
                .iter()
                .filter(|r| {
                    (from_db.is_empty() || r.from_database == from_db || r.to_database == from_db)
                        && (to_db.is_empty() || r.from_database == to_db || r.to_database == to_db)
                })
                .collect();

            if relevant_relations.is_empty() {
                // If no cross-db relations found, show intra-database FK relations hint
                let result = if from_db.is_empty() && to_db.is_empty() {
                    format!("No cross-database relationships found between any databases. Available databases: {}", db_list)
                } else {
                    format!("No relationships found between '{}' and '{}'. Available databases: {}", from_db, to_db, db_list)
                };
                conversation.push_str(&format!("\n\nTool result: {result}"));
                eprintln!("[LLM] {result}");
            } else {
                let relations_str: Vec<String> = relevant_relations
                    .iter()
                    .map(|r| {
                        format!(
                            "{}.{}.{} → {}.{}.{}",
                            r.from_database, r.from_table, r.from_column,
                            r.to_database, r.to_table, r.to_column
                        )
                    })
                    .collect();
                let result = format!(
                    "Relationships found:\n{}",
                    relations_str.join("\n")
                );
                conversation.push_str(&format!("\n\nTool result:\n{result}"));
                eprintln!("[LLM] {}", result.chars().take(200).collect::<String>());
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
                                return Ok(Some(sql));
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
        _ => {
            conversation.push_str(&format!("\n\nTool error: Unknown tool '{tool_name}'"));
        }
    }
    Ok(None)
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
