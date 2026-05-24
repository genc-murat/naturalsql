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

    let raw = ollama_response.response.trim().to_string();
    let sql = match sanitize_and_extract_sql(&raw, natural_language) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[LLM] sanitize_and_extract_sql rejected in simple path: {}", e);
            return Err(AppError::InvalidLlmResponse);
        }
    };

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
    selected_database: Option<&str>,
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

    let current_database = selected_database.filter(|s| !s.trim().is_empty()).unwrap_or("");

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

    let mut seen_tool_calls: std::collections::HashMap<String, (String, u32)> = std::collections::HashMap::new();
    let mut conversation = String::new();
    let mut schemas_explored: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut data_tools_used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut tool_steps: Vec<ToolCallStep> = Vec::new();

    for iteration in 0..max_iterations {
        eprintln!("[LLM] Tool iteration {}/{}", iteration + 1, max_iterations);

        // Build available tables list for prompt (bias selected DB first)
        let available_tables_str: String = {
            let mut lines: Vec<String> = Vec::new();
            if !current_database.is_empty() {
                if let Some(s) = all_schemas.iter().find(|s| s.database == current_database) {
                    let tables: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
                    lines.push(format!("- {} (SELECTED / PREFERRED): {}", s.database, tables.join(", ")));
                }
            }
            for s in all_schemas.iter().filter(|s| s.database != current_database) {
                let tables: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
                lines.push(format!("- {}: {}", s.database, tables.join(", ")));
            }
            if lines.is_empty() {
                all_schemas.iter()
                    .map(|s| {
                        let tables: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
                        format!("- {}: {}", s.database, tables.join(", "))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                lines.join("\n")
            }
        };

        let exploration_guidance = if schemas_explored.len() >= 2 {
            let tables_list = schemas_explored.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
            let data_note = if !data_tools_used.is_empty() {
                let data_list = data_tools_used.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                format!(" You have inspected data for: {data_list}. Write the FINAL SQL NOW using the tool results above — DO NOT repeat any get_recent_data/search_data/get_column_values/get_sample_data calls.")
            } else { String::new() };
            format!("You already have schema for: {tables_list}.{data_note} Write the SQL query NOW. Do NOT call any more schema or data tools.\n")
        } else if schemas_explored.len() == 1 {
            let table_name = schemas_explored.iter().next().unwrap();
            let data_note = if data_tools_used.iter().any(|d| d.starts_with(&format!("{}.", table_name.split('.').next().unwrap_or("")) )) { " You have recent/sample data. " } else { "" };
            format!("You already have schema for {table_name}.{data_note} Get the SECOND table's schema with get_table_schema, or use cross_db_join. Do NOT call get_indexes or get_foreign_keys — you don't need them to write SQL.\n")
        } else {
            let current_bias = if !current_database.is_empty() {
                format!("\n**CURRENT DATABASE BIAS: Prefer tables from '{}' (SELECTED). Only use other DBs if you explicitly call find_relationships or cross_db_join first.**\n", current_database)
            } else { String::new() };
            format!("RULE: You MUST call get_table_schema for each table BEFORE writing SQL. NEVER guess column names. Always use get_table_schema first.\n\n\
                      **AVAILABLE TABLES (use these exact names, NEVER placeholders):**\n\
                      {available_tables_str}\n{current_bias}\n\
                      After calling data tools (get_recent_data etc.) on a table, DO NOT call them again — immediately write the SQL using the 'Tool result' data shown above.")
        };

        let current_db_section = if !current_database.is_empty() {
            format!("**CURRENT/SELECTED DATABASE: {}** — strongly prefer this DB's tables for queries unless the question requires cross-DB joins (in which case use find_relationships or cross_db_join tool explicitly).\n\n", current_database)
        } else {
            String::new()
        };

        // IMPORTANT: No angle-bracket placeholders like <table_name> — LLMs copy them verbatim.
        // Use ALL_CAPS descriptive labels instead.
        let tools_description = format!(
            "You have access to tools. Use them by outputting a SINGLE JSON object. Do NOT add reasoning before or after the JSON.\n\
             Core (for writing SQL):\n\
             - list_tables(database) — list tables in a database\n\
             - get_table_schema(database, table) — get column definitions for a table\n\
             - get_sample_data(database, table) — get 3 sample rows from a table\n\
             - cross_db_join(left_table, right_table, join_type) — generate JOIN SQL for two tables\n\
             Data Discovery:\n\
             - get_table_row_count(database, table) — approximate row count\n\
             - get_column_values(database, table, column, limit) — distinct column values\n\
             - get_recent_data(database, table, limit) — last N rows ordered by date/id\n\
             - search_data(database, table, pattern) — LIKE search across string columns\n\
             Schema & Metadata:\n\
             - get_table_ddl(database, table) — SHOW CREATE TABLE DDL\n\
             - get_indexes(database, table) — index definitions\n\
             - get_foreign_keys(database, table) — foreign key definitions\n\
             - get_constraints(database, table) — all constraints (PK, FK, UNIQUE, CHECK)\n\
             - list_views(database) — list views\n\
             - list_procedures(database) — list stored procedures\n\
             - list_triggers(database) — list triggers\n\
             - get_table_stats(database, table) — row count, data size, index size\n\
             - get_table_status(database, table) — engine, row format, collation\n\
             - analyze_table_health(database, table) — fragmentation and health check\n\
             Key & Constraint Discovery:\n\
             - get_primary_key(database, table) — primary key columns\n\
             - get_unique_keys(database, table) — unique key columns\n\
             - get_enum_values(database, table, column) — ENUM/SET possible values\n\
             - get_auto_increment(database, table) — current auto_increment value\n\
             - get_referenced_by(database, table) — tables that reference this table (reverse FK)\n\
             Cross-Database Discovery:\n\
             - find_relationships(from_database, to_database) — FK relations between databases\n\
             - find_similar_columns(column_pattern) — find columns matching a name pattern\n\
             - compare_tables(left, right) — compare structure of two tables\n\
             Data Analysis:\n\
             - count_nulls(database, table) — NULL count per column\n\
             - get_column_stats(database, table) — min/max/avg/distinct stats per column\n\
             - get_database_size(database) — total database size in MB\n\
             - get_table_size_ranking(database) — tables ranked by size\n\
             - find_orphan_records(database, table, column, ref_database, ref_table, ref_column) — find orphan records\n\
             Performance & Monitoring:\n\
             - get_active_connections — current MySQL connections\n\
             - get_slow_queries(limit) — slowest queries from performance_schema\n\
             - get_table_partitions(database, table) — partition info\n\
             - suggest_indexes(database, table) — index suggestions based on column patterns\n\
             Advanced Schema:\n\
             - get_column_charset(database, table) — charset/collation per column\n\
             - get_data_type_summary(database) — data type distribution across database\n\
             - get_table_aliases — find similarly named tables across databases\n\
             - get_create_options(database, table) — table CREATE OPTIONS (engine, pack_keys, etc.)\n\
             Server Info & Variables:\n\
             - get_server_info — MySQL version, user, charset\n\
             - get_variable(name) — server variable value\n\
             - get_user_privileges(database) — user permissions\n\
             Query Validation:\n\
             - explain_query(sql) — EXPLAIN plan for a query\n\
             - validate_sql(sql) — syntax validation\n\
             - security_check(sql) — security analysis\n\
             {exploration_guidance}"
        );

        let prompt = format!(
            "You are a MySQL expert. Available databases: {db_list}\n\n\
              {current_db_section}\
              {tools_description}\n\n\
              **STRICT RULES - FOLLOW EXACTLY:**\n\
              1. NEVER invent table names, column names, or data values. ONLY use names returned by tools.\n\
              2. ALWAYS call get_table_schema BEFORE writing any SQL query. NEVER guess column names.\n\
              3. Use ONLY column names exactly as shown in schema results. Case-sensitive.\n\
              4. If unsure about a table or column, call a SINGLE tool. Do NOT emit any prose explaining your thought process.\n\
              5. When you have enough information, write ONLY the SQL query. No explanations, no markdown, no backticks.\n\
              6. Always use fully qualified table names: database.table\n\
              7. IMPORTANT: If you need a tool, output ONLY the JSON. If you're ready, output ONLY the SQL.\n\
              8. Do NOT repeat the same tool call.\n\
              9. If a tool returns an error, try a different approach.\n\n\
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
            // Support two JSON formats:
            // Format A: {"tool": "get_indexes", "database": "...", "table": "..."}
            // Format B: {"get_indexes": {"database": "...", "table": "..."}}  (nested)
            let tool_call = if tool_call.get("tool").and_then(|v| v.as_str()).is_some() {
                // Format A — already has "tool" key
                tool_call
            } else if let Some((tool_name, params)) = tool_call.as_object().and_then(|obj| {
                // Format B — find first key that matches a known tool name
                obj.iter().find_map(|(k, v)| {
                    if k == "tool" || k == "response" || k == "content" || k == "parameters" {
                        return None;
                    }
                    // Check if the value is an object (nested params)
                    if v.is_object() {
                        Some((k.clone(), v.clone()))
                    } else {
                        None
                    }
                })
            }) {
                // Reconstruct into Format A
                let mut normalized = serde_json::Map::new();
                normalized.insert("tool".to_string(), serde_json::json!(tool_name));
                if let Some(params_obj) = params.as_object() {
                    for (k, v) in params_obj {
                        normalized.insert(k.clone(), v.clone());
                    }
                }
                eprintln!("[LLM] Normalized nested JSON to Format A: tool={}", tool_name);
                serde_json::Value::Object(normalized)
            } else {
                // Not a tool call at all
                tool_call
            };

            if let Some(raw_tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                // Validate parameter values — reject placeholder-like values
                if let Some(obj) = tool_call.as_object() {
                    for (key, val) in obj {
                        if key == "tool" { continue; }
                        if let Some(s) = val.as_str() {
                            let trimmed = s.trim();
                            if trimmed.starts_with('<') && trimmed.ends_with('>') {
                                eprintln!("[LLM] Rejected placeholder param: {}={}", key, s);
                                // Build a list of available tables to guide the LLM
                                let available_tables: Vec<String> = all_schemas.iter()
                                    .map(|s| {
                                        let tables: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
                                        format!("{}.{{{}}}", s.database, tables.join(", "))
                                    })
                                    .collect();
                                let tables_hint = available_tables.join("; ");
                                conversation.push_str(&format!(
                                    "\n\nTool error: Parameter '{}' has a placeholder value '{}'. \
                                     You MUST use a REAL table name from these available tables: {}\n\
                                     Call list_tables(database) first if you need to see the list again.",
                                    key, s, tables_hint
                                ));
                                tool_continued = true;
                                break;
                            }
                        }
                    }
                }
                if tool_continued { continue; }

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
                    "get_row_count" | "count_rows" => "get_table_row_count",
                    "get_column_values" | "distinct_values" => "get_column_values",
                    "get_recent" | "recent_data" => "get_recent_data",
                    "search_data" | "search_table" => "search_data",
                    "get_ddl" | "show_create_table" => "get_table_ddl",
                    "get_variable" | "server_variable" => "get_variable",
                    "get_privileges" | "user_privileges" => "get_user_privileges",
                    "analyze_health" | "table_health" => "analyze_table_health",
                    "pk" | "primary_key" => "get_primary_key",
                    "unique" | "unique_keys" => "get_unique_keys",
                    "enums" | "enum_values" | "set_values" => "get_enum_values",
                    "auto_inc" => "get_auto_increment",
                    "referenced_by" | "who_references" => "get_referenced_by",
                    "null_count" => "count_nulls",
                    "column_stats" | "stats" => "get_column_stats",
                    "db_size" | "database_size" => "get_database_size",
                    "size_ranking" | "table_sizes" => "get_table_size_ranking",
                    "connections" | "processlist" | "active_connections" => "get_active_connections",
                    "slow_query_log" => "get_slow_queries",
                    "partitions" => "get_table_partitions",
                    "charset" | "collation_info" => "get_column_charset",
                    "data_types" | "type_summary" => "get_data_type_summary",
                    "orphans" | "orphan_records" => "find_orphan_records",
                    "similar_tables" | "aliases" | "table_aliases" => "get_table_aliases",
                    "suggest_index" | "index_suggestions" => "suggest_indexes",
                    "create_options" | "table_options" => "get_create_options",
                    _ => raw_tool_name,
                };

                // Build a normalized signature for repeat detection
                let call_sig = make_canonical_tool_sig(normalized_name, &tool_call);

                let mut call_count = 0;
                let mut prev_result = String::new();
                if let Some((res, count)) = seen_tool_calls.get(&call_sig) {
                    call_count = *count;
                    prev_result = res.clone();
                }

                if call_count > 0 {
                    let truncated_result = if prev_result.len() > 300 {
                        format!("{}... [truncated]", &prev_result[..300])
                    } else {
                        prev_result.clone()
                    };

                    if call_count >= 2 {
                        // Repeated call 2nd time → fallback with ALL schemas for complete context
                        eprintln!("[LLM] Repeated tool call ({normalized_name}) second time, falling back with all schemas");
                        let schema_context = if has_cross_db_relations {
                            schema::format_schemas_with_relationships(all_schemas, &cross_db_relations)
                        } else {
                            schema::format_all_schemas_for_prompt(all_schemas)
                        };
                        let fallback_prompt = format!(
                            "You are a MySQL 5.6+ expert. Your ONLY job is to write a SELECT query that ANSWERS the user's question.\n\
                             \n\
                             You already called the tool `{normalized_name}` and got:\n\
                             {truncated_result}\n\
                             Do NOT call this tool or any other tools again. You must write the final SQL query now.\n\
                             \n\
                             STRICTLY FORBIDDEN — NEVER write these:\n\
                             - NEVER query information_schema (no information_schema.TABLES, COLUMNS, STATISTICS, etc.)\n\
                             - NEVER use SHOW commands (SHOW INDEX, SHOW CREATE TABLE, SHOW GRANTS, etc.)\n\
                             - NEVER use EXPLAIN\n\
                             - NEVER query mysql.* system tables\n\
                             \n\
                             ALLOWED — ONLY write:\n\
                             - SELECT ... FROM actual_database.actual_table\n\
                             - Use ONLY column names shown in the schema below\n\
                             \n\
                             Database schema:\n\
                             {schema_context}\n\n\
                             User's question: {natural_language}\n\n\
                             Write a SELECT query that ANSWERS this question. ONLY return the SQL, no explanations:\n",
                        );
                        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt, natural_language).await.map(|sql| (sql, tool_steps, (iteration + 1) as u32));
                    } else {
                        // First repeat -> inject a strong warning note into conversation and continue
                        eprintln!("[LLM] Repeated tool call ({normalized_name}) first time, injecting strong guidance note");
                        let guidance = format!(
                            "\n\n**CRITICAL WARNING:** You already called the tool `{normalized_name}` with these exact parameters and received:\n\
                             ```\n\
                             {}\n\
                             ```\n\
                             Do NOT call this tool or any other tools again. You already have enough information. Your NEXT action MUST be writing the final SQL query now.",
                            truncated_result
                        );
                        conversation.push_str(&guidance);

                        // Increment count in seen_tool_calls
                        if let Some(entry) = seen_tool_calls.get_mut(&call_sig) {
                            entry.1 += 1;
                        }

                        continue;
                    }
                }

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
                tool_steps.push(step.clone());

                seen_tool_calls.insert(call_sig, (step.result.clone(), 1));

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
                // Track data tools to prevent repeat loops on get_recent_data etc.
                if matches!(normalized_name, "get_recent_data" | "search_data" | "get_column_values" | "get_sample_data" | "get_table_row_count" | "count_nulls" | "get_column_stats") {
                    if let (Some(db), Some(table)) = (
                        tool_call.get("database").and_then(|v| v.as_str()),
                        tool_call.get("table").and_then(|v| v.as_str())
                    ) {
                        data_tools_used.insert(format!("{db}.{table}"));
                        eprintln!("[LLM] Data tools used for tables: {:?}", data_tools_used);
                    }
                }
            }
        }

        // If not parsed as whole JSON, try extracting from mixed content
        if !tool_continued {
            if let Some(mut tool_call) = extract_first_json_object(&text) {
                // Support nested JSON format: {"get_indexes": {"database": "...", "table": "..."}}
                if tool_call.get("tool").is_none() {
                    if let Some((tool_name, params)) = tool_call.as_object().and_then(|obj| {
                        obj.iter().find_map(|(k, v)| {
                            if k == "tool" || k == "response" || k == "content" || k == "parameters" {
                                return None;
                            }
                            if v.is_object() { Some((k.clone(), v.clone())) } else { None }
                        })
                    }) {
                        let mut normalized = serde_json::Map::new();
                        normalized.insert("tool".to_string(), serde_json::json!(tool_name));
                        if let Some(params_obj) = params.as_object() {
                            for (k, v) in params_obj {
                                normalized.insert(k.clone(), v.clone());
                            }
                        }
                        eprintln!("[LLM] Normalized nested JSON (extracted) to Format A: tool={}", tool_name);
                        tool_call = serde_json::Value::Object(normalized);
                    }
                }

                if let Some(raw_tool_name) = tool_call.get("tool").and_then(|v| v.as_str()) {
                    // Validate parameter values — reject placeholder-like values
                    let mut has_placeholder = false;
                    if let Some(obj) = tool_call.as_object() {
                        for (key, val) in obj {
                            if key == "tool" { continue; }
                            if let Some(s) = val.as_str() {
                                let trimmed = s.trim();
                                if trimmed.starts_with('<') && trimmed.ends_with('>') {
                                    eprintln!("[LLM] Rejected placeholder param (extracted): {}={}", key, s);
                                    let available_tables: Vec<String> = all_schemas.iter()
                                        .map(|s| {
                                            let tables: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
                                            format!("{}.{{{}}}", s.database, tables.join(", "))
                                        })
                                        .collect();
                                    let tables_hint = available_tables.join("; ");
                                    conversation.push_str(&format!(
                                        "\n\nTool error: Parameter '{}' has a placeholder value '{}'. \
                                         You MUST use a REAL table name from: {}\n\
                                         Call list_tables(database) first.",
                                        key, s, tables_hint
                                    ));
                                    has_placeholder = true;
                                    break;
                                }
                            }
                        }
                    }
                    if has_placeholder {
                        tool_continued = true;
                    } else {
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
                            "get_row_count" | "count_rows" => "get_table_row_count",
                            "get_column_values" | "distinct_values" => "get_column_values",
                            "get_recent" | "recent_data" => "get_recent_data",
                            "search_data" | "search_table" => "search_data",
                            "get_ddl" | "show_create_table" => "get_table_ddl",
                            "get_variable" | "server_variable" => "get_variable",
                            "get_privileges" | "user_privileges" => "get_user_privileges",
                            "analyze_health" | "table_health" => "analyze_table_health",
                            "pk" | "primary_key" => "get_primary_key",
                            "unique" | "unique_keys" => "get_unique_keys",
                            "enums" | "enum_values" | "set_values" => "get_enum_values",
                            "auto_inc" => "get_auto_increment",
                            "referenced_by" | "who_references" => "get_referenced_by",
                            "null_count" => "count_nulls",
                            "column_stats" | "stats" => "get_column_stats",
                            "db_size" | "database_size" => "get_database_size",
                            "size_ranking" | "table_sizes" => "get_table_size_ranking",
                            "connections" | "processlist" | "active_connections" => "get_active_connections",
                            "slow_query_log" => "get_slow_queries",
                            "partitions" => "get_table_partitions",
                            "charset" | "collation_info" => "get_column_charset",
                            "data_types" | "type_summary" => "get_data_type_summary",
                            "orphans" | "orphan_records" => "find_orphan_records",
                            "similar_tables" | "aliases" | "table_aliases" => "get_table_aliases",
                            "suggest_index" | "index_suggestions" => "suggest_indexes",
                            "create_options" | "table_options" => "get_create_options",
                            _ => raw_tool_name,
                        };

                        let call_sig = make_canonical_tool_sig(normalized_name, &tool_call);

                        let mut call_count = 0;
                        let mut prev_result = String::new();
                        if let Some((res, count)) = seen_tool_calls.get(&call_sig) {
                            call_count = *count;
                            prev_result = res.clone();
                        }

                        if call_count > 0 {
                            let truncated_result = if prev_result.len() > 300 {
                                format!("{}... [truncated]", &prev_result[..300])
                            } else {
                                prev_result.clone()
                            };

                            if call_count >= 2 {
                                // Repeated call 2nd time → fallback with ALL schemas for complete context
                                eprintln!("[LLM] Repeated tool call (extracted, {normalized_name}) second time, falling back with all schemas");
                                let schema_context = if has_cross_db_relations {
                                    schema::format_schemas_with_relationships(all_schemas, &cross_db_relations)
                                } else {
                                    schema::format_all_schemas_for_prompt(all_schemas)
                                };
                                let fallback_prompt = format!(
                                    "You are a MySQL 5.6+ expert. Your ONLY job is to write a SELECT query that ANSWERS the user's question.\n\
                                     \n\
                                     You already called the tool `{normalized_name}` and got:\n\
                                     {truncated_result}\n\
                                     Do NOT call this tool or any other tools again. You must write the final SQL query now.\n\
                                     \n\
                                     STRICTLY FORBIDDEN — NEVER write these:\n\
                                     - NEVER query information_schema\n\
                                     - NEVER use SHOW commands (SHOW INDEX, SHOW CREATE TABLE, etc.)\n\
                                     - NEVER use EXPLAIN\n\
                                     - NEVER query mysql.* system tables\n\
                                     \n\
                                     ALLOWED — ONLY write: SELECT ... FROM actual_database.actual_table\n\n\
                                     Database schema:\n\
                                     {schema_context}\n\n\
                                     User's question: {natural_language}\n\n\
                                     Write a SELECT query that ANSWERS this question. ONLY return the SQL, no explanations:\n",
                                );
                                return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt, natural_language).await.map(|sql| (sql, tool_steps.clone(), (iteration + 1) as u32));
                            } else {
                                // First repeat -> inject a strong warning note into conversation and continue
                                eprintln!("[LLM] Repeated tool call (extracted, {normalized_name}) first time, injecting strong guidance note");
                                let guidance = format!(
                                    "\n\n**CRITICAL WARNING:** You already called the tool `{normalized_name}` with these exact parameters and received:\n\
                                     ```\n\
                                     {}\n\
                                     ```\n\
                                     Do NOT call this tool or any other tools again. You already have enough information. Your NEXT action MUST be writing the final SQL query now.",
                                    truncated_result
                                );
                                conversation.push_str(&guidance);

                                // Increment count in seen_tool_calls
                                if let Some(entry) = seen_tool_calls.get_mut(&call_sig) {
                                    entry.1 += 1;
                                }

                                continue;
                            }
                        }

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
                        tool_steps.push(step.clone());

                        seen_tool_calls.insert(call_sig, (step.result.clone(), 1));

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
                        // Track data tools to prevent repeat loops on get_recent_data etc. (extracted path)
                        if matches!(normalized_name, "get_recent_data" | "search_data" | "get_column_values" | "get_sample_data" | "get_table_row_count" | "count_nulls" | "get_column_stats") {
                            if let (Some(db), Some(table)) = (
                                tool_call.get("database").and_then(|v| v.as_str()),
                                tool_call.get("table").and_then(|v| v.as_str())
                            ) {
                                data_tools_used.insert(format!("{db}.{table}"));
                            }
                        }
                    }
                }
            }
        }

        if tool_continued {
            continue;
        }

        // Not a tool call — extract SQL (robust sanitize to block NL pollution)
        let sql = sanitize_and_extract_sql(&text, natural_language).unwrap_or_else(|e| {
            eprintln!("[LLM] sanitize rejected direct path: {}", e);
            String::new()
        });

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
                             \n\
                             STRICTLY FORBIDDEN: NEVER query information_schema, NEVER use SHOW/EXPLAIN, NEVER query mysql.* tables.\n\
                             ONLY write SELECT queries that produce application data answering the user's question.\n\n\
                             Database schema (ONLY use column names shown here):\n\
                             {schema_context}\n\n\
                             User's question: {natural_language}\n\n\
                             Write a SELECT query that ANSWERS this question. ONLY return the SQL:\n",
                        );
                        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt, natural_language).await.map(|sql| (sql, tool_steps.clone(), (iteration + 1) as u32));
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
            "You are a MySQL 5.6+ expert. Your ONLY job is to write a SELECT query that ANSWERS the user's question.\n\
             \n\
             STRICTLY FORBIDDEN: NEVER query information_schema, NEVER use SHOW/EXPLAIN, NEVER query mysql.* tables.\n\
             ONLY write: SELECT ... FROM actual_database.actual_table\n\n\
             Database schema:\n\
             {schema_context}\n\n\
             User's question: {natural_language}\n\n\
             Write a SELECT query that ANSWERS this question. ONLY return the SQL:\n",
        );
        return call_ollama_generate(&url, &model, &client, &api_url, &fallback_prompt, natural_language).await.map(|sql| (sql, tool_steps.clone(), (iteration + 1) as u32));
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
    natural_language: &str,
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
    let raw = body.response.trim().to_string();

    let sql = match sanitize_and_extract_sql(&raw, natural_language) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[LLM] sanitize_and_extract_sql rejected fallback output: {}", e);
            // Return error so caller can surface clean failure instead of polluted SQL
            return Err(AppError::InvalidLlmResponse);
        }
    };

    if sql.is_empty() {
        return Err(AppError::InvalidLlmResponse);
    }

    eprintln!("[LLM] Fallback SQL (sanitized): {}", sql.chars().take(100).collect::<String>());
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
        "get_row_count" | "count_rows" => "get_table_row_count",
        "get_column_values" | "distinct_values" => "get_column_values",
        "get_recent" | "recent_data" => "get_recent_data",
        "search_data" | "search_table" => "search_data",
        "get_ddl" | "show_create_table" => "get_table_ddl",
        "get_variable" | "server_variable" => "get_variable",
        "get_privileges" | "user_privileges" => "get_user_privileges",
        "analyze_health" | "table_health" => "analyze_table_health",
        "pk" | "primary_key" => "get_primary_key",
        "unique" | "unique_keys" => "get_unique_keys",
        "enums" | "enum_values" | "set_values" => "get_enum_values",
        "auto_inc" => "get_auto_increment",
        "referenced_by" | "who_references" => "get_referenced_by",
        "null_count" => "count_nulls",
        "column_stats" | "stats" => "get_column_stats",
        "db_size" | "database_size" => "get_database_size",
        "size_ranking" | "table_sizes" => "get_table_size_ranking",
        "connections" | "processlist" | "active_connections" => "get_active_connections",
        "slow_query_log" => "get_slow_queries",
        "partitions" => "get_table_partitions",
        "charset" | "collation_info" => "get_column_charset",
        "data_types" | "type_summary" => "get_data_type_summary",
        "orphans" | "orphan_records" => "find_orphan_records",
        "similar_tables" | "aliases" | "table_aliases" => "get_table_aliases",
        "suggest_index" | "index_suggestions" => "suggest_indexes",
        "create_options" | "table_options" => "get_create_options",
        _ => tool_name,
    };

    let (params_map, tool_name) = if tool_call.get("args").is_some() && tool_call.get("function_name").is_some() {
        // Nested format handler: {"function_name": "get_table_schema", "args": {"database": "...", "table": "..."}}
        let name = tool_call.get("function_name").and_then(|v| v.as_str()).unwrap_or(tool_name);
        let mut params = HashMap::new();
        if let Some(args) = tool_call.get("args").and_then(|v| v.as_object()) {
            for (k, v) in args {
                params.insert(k.clone(), match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Null => "NULL".to_string(),
                    other => other.to_string(),
                });
            }
        }
        (params, name)
    } else {
        let mut params = HashMap::new();
        if let Some(obj) = tool_call.as_object() {
            for (k, v) in obj {
                if k == "tool" { continue; }
                params.insert(k.clone(), match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Null => "NULL".to_string(),
                    other => other.to_string(),
                });
            }
        }
        (params, tool_name)
    };

    match tool_name {
        "list_tables" => {
            let database = params_map.get("database").map(|s| s.as_str()).unwrap_or("");
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
            let database = params_map.get("database").map(|s| s.as_str()).unwrap_or("");
            let table = params_map.get("table").map(|s| s.as_str()).unwrap_or("");
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
            let database = params_map.get("database").map(|s| s.as_str()).unwrap_or("");
            let table = params_map.get("table").map(|s| s.as_str()).unwrap_or("");

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
            let left_table = params_map.get("left_table").map(|s| s.as_str()).unwrap_or("");
            let right_table = params_map.get("right_table").map(|s| s.as_str()).unwrap_or("");
            let join_type = params_map.get("join_type").map(|s| s.as_str()).unwrap_or("INNER JOIN");

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

        "get_table_row_count" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_table_row_count(database, table).await {
                Ok(count) => {
                    let result = format!("Table {}.{} has approximately {} rows", database, table, count);
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_column_values" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            let column = tool_call.get("column").and_then(|v| v.as_str()).unwrap_or("");
            let limit = tool_call.get("limit").and_then(|v| v.as_str())
                .and_then(|v| v.parse::<usize>().ok()).unwrap_or(20);

            if column.is_empty() {
                conversation.push_str(&format!("\n\nTool error: column parameter is required"));
            } else {
                match schema::get_column_distinct_values(database, table, column, limit).await {
                    Ok(values) if values.is_empty() => {
                        conversation.push_str(&format!("\n\nTool result: No distinct values found for {}.{}.{}", database, table, column));
                    }
                    Ok(values) => {
                        let display: String = values.iter().take(10).map(|v| format!("  '{}'", v)).collect::<Vec<_>>().join("\n");
                        let more_note = if values.len() > 10 { format!("\n... and {} more (showing first 10 of {})", values.len() - 10, values.len()) } else { "".to_string() };
                        let result = format!("Distinct values for {}.{}.{} ({} total):\n{}\n{}", database, table, column, values.len(), display, more_note);
                        conversation.push_str(&format!("\n\nTool result:\n{result}"));
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "get_recent_data" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            let limit = tool_call.get("limit").and_then(|v| v.as_str())
                .and_then(|v| v.parse::<usize>().ok()).unwrap_or(5);

            match schema::get_recent_data(database, table, limit).await {
                Ok((_cols, rows)) if rows.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: Table {}.{} is empty", database, table));
                }
                Ok((cols, rows)) => {
                    let header = format!("Columns: {}", cols.join(", "));
                    let data_rows: Vec<String> = rows.iter().map(|row| {
                        format!("  ({})", row.iter().take(5).map(|v| {
                            if v.len() > 30 { format!("{}...", &v[..30]) } else { v.clone() }
                        }).collect::<Vec<_>>().join(", "))
                    }).collect();
                    let result = format!("Recent rows from {}.{} (showing {} of {}):\n{}\n{}", database, table, rows.len(), limit, header, data_rows.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "search_data" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            let pattern = tool_call.get("pattern").and_then(|v| v.as_str()).unwrap_or("");

            if pattern.is_empty() {
                conversation.push_str(&format!("\n\nTool error: pattern parameter is required"));
            } else {
                match schema::search_table_data(database, table, pattern, None, 10).await {
                    Ok((_cols, rows, _total)) if rows.is_empty() => {
                        conversation.push_str(&format!("\n\nTool result: No rows matching '{}' found in {}.{}", pattern, database, table));
                    }
                    Ok((cols, rows, total)) => {
                        let header = format!("Columns: {}", cols.join(", "));
                        let data_rows: Vec<String> = rows.iter().map(|row| {
                            format!("  ({})", row.iter().map(|v| {
                                if v.len() > 30 { format!("{}...", &v[..30]) } else { v.clone() }
                            }).collect::<Vec<_>>().join(", "))
                        }).collect();
                        let total_note = if total > 10 { format!("\nTotal matching rows: {} (showing first 10)", total) } else { "".to_string() };
                        let result = format!("Search results for '{}' in {}.{} ({} found):\n{}\n{}{}", pattern, database, table, total, header, data_rows.join("\n"), total_note);
                        conversation.push_str(&format!("\n\nTool result:\n{result}"));
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "get_table_ddl" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_table_ddl(database, table).await {
                Ok(ddl) => {
                    let result = format!("CREATE TABLE statement for {}.{}:\n{}", database, table, ddl);
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_variable" => {
            let name = tool_call.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                conversation.push_str(&format!("\n\nTool error: name parameter is required. Example: max_connections, innodb_buffer_pool_size"));
            } else {
                match schema::get_server_variable(name).await {
                    Ok(value) => {
                        let result = format!("MySQL variable '{}' = {}", name, value);
                        conversation.push_str(&format!("\n\nTool result: {result}"));
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "get_user_privileges" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let db = if database.is_empty() {
                // Use first available database
                all_schemas.first().map(|s| s.database.as_str()).unwrap_or("")
            } else {
                database
            };
            match schema::get_user_privileges(db).await {
                Ok(privs) if privs.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No specific table privileges found for current user on {}", db));
                }
                Ok(privs) => {
                    let display: String = privs.iter().take(10).map(|p| format!("  - {}", p)).collect::<Vec<_>>().join("\n");
                    let more_note = if privs.len() > 10 { format!("\n... and {} more", privs.len() - 10) } else { "".to_string() };
                    let result = format!("Privileges on {}:\n{}\n{}", db, display, more_note);
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "analyze_table_health" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::analyze_table_health(database, table).await {
                Ok(health) => {
                    let frag_status = if health.fragmentation_percent > 20.0 {
                        "HIGH FRAGMENTATION - consider OPTIMIZE TABLE"
                    } else if health.fragmentation_percent > 10.0 {
                        "MODERATE FRAGMENTATION"
                    } else {
                        "HEALTHY"
                    };
                    let result = format!(
                        "Health for {}.{}: Engine={} | Rows={} | Data={} bytes | Indexes={} bytes | Free={} bytes | Fragmentation={}%. Status: {}",
                        database, table, health.engine, health.row_count,
                        health.data_size_bytes, health.index_size_bytes,
                        health.free_space_bytes, health.fragmentation_percent, frag_status
                    );
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_primary_key" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_primary_key_columns(database, table).await {
                Ok(cols) if cols.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No primary key on {database}.{table}"));
                }
                Ok(cols) => {
                    let result = format!("Primary key on {database}.{table}: {}", cols.join(", "));
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_unique_keys" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_unique_key_columns(database, table).await {
                Ok(indexes) if indexes.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No unique keys on {database}.{table}"));
                }
                Ok(indexes) => {
                    let lines: Vec<String> = indexes.iter().map(|i| {
                        format!("  {} ({})", i.name, i.column)
                    }).collect();
                    let result = format!("Unique keys on {database}.{table}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_enum_values" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            let column = tool_call.get("column").and_then(|v| v.as_str()).unwrap_or("");
            if column.is_empty() {
                conversation.push_str(&format!("\n\nTool error: column parameter is required"));
            } else {
                match schema::get_enum_values(database, table, column).await {
                    Ok(values) if values.is_empty() => {
                        conversation.push_str(&format!("\n\nTool result: {database}.{table}.{column} is not an ENUM or SET type, or has no values"));
                    }
                    Ok(values) => {
                        let result = format!("ENUM/SET values for {database}.{table}.{column}: {}", values.iter().map(|v| format!("'{}'", v)).collect::<Vec<_>>().join(", "));
                        conversation.push_str(&format!("\n\nTool result: {result}"));
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "get_auto_increment" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_auto_increment_value(database, table).await {
                Ok(Some(value)) => {
                    let result = format!("Auto-increment for {database}.{table}: next value = {}", value);
                    conversation.push_str(&format!("\n\nTool result: {result}"));
                }
                Ok(None) => {
                    conversation.push_str(&format!("\n\nTool result: No auto-increment column on {database}.{table}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_referenced_by" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_referencing_tables(database, table).await {
                Ok(refs) if refs.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No tables reference {database}.{table}"));
                }
                Ok(refs) => {
                    let lines: Vec<String> = refs.iter().map(|r| {
                        format!("  {}.{} ({}) → references {}.{}", r.from_database, r.from_table, r.from_column, r.to_table, r.to_column)
                    }).collect();
                    let result = format!("Tables referencing {database}.{table}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "count_nulls" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::count_nulls_per_column(database, table).await {
                Ok(nulls) if nulls.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No columns found for {database}.{table}"));
                }
                Ok(nulls) => {
                    let lines: Vec<String> = nulls.iter().filter(|n| n.null_count > 0).map(|n| {
                        let pct = if n.total_count > 0 { (n.null_count as f64 / n.total_count as f64 * 100.0).round() } else { 0.0 };
                        format!("  {}: {}/{} NULL ({}%)", n.column, n.null_count, n.total_count, pct)
                    }).collect();
                    if lines.is_empty() {
                        conversation.push_str(&format!("\n\nTool result: No NULL values found in {database}.{table}"));
                    } else {
                        let result = format!("NULL counts for {database}.{table}:\n{}", lines.join("\n"));
                        conversation.push_str(&format!("\n\nTool result:\n{result}"));
                    }
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_column_stats" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_column_statistics(database, table).await {
                Ok(stats) if stats.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No columns found for {database}.{table}"));
                }
                Ok(stats) => {
                    let lines: Vec<String> = stats.iter().map(|s| {
                        let mut parts = vec![format!("  {} [{}]: {} values, {} distinct", s.column, s.data_type, s.count, s.distinct_count)];
                        if let Some(ref min) = s.min_val {
                            parts.push(format!("min='{}'", min));
                        }
                        if let Some(ref max) = s.max_val {
                            parts.push(format!("max='{}'", max));
                        }
                        if let Some(avg) = s.avg_val {
                            parts.push(format!("avg={:.2}", avg));
                        }
                        parts.join(", ")
                    }).collect();
                    let result = format!("Column statistics for {database}.{table}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_database_size" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            if database.is_empty() {
                conversation.push_str(&format!("\n\nTool error: database parameter is required"));
            } else {
                match schema::get_database_size(database).await {
                    Ok(size) => {
                        let result = format!(
                            "Database '{}' size: {} MB total ({} MB data, {} MB indexes), {} tables, ~{} rows",
                            database, size.total_size_mb, size.data_size_mb, size.index_size_mb, size.table_count, size.total_rows
                        );
                        conversation.push_str(&format!("\n\nTool result: {result}"));
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "get_table_size_ranking" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            if database.is_empty() {
                conversation.push_str(&format!("\n\nTool error: database parameter is required"));
            } else {
                match schema::get_table_size_ranking(database).await {
                    Ok(entries) if entries.is_empty() => {
                        conversation.push_str(&format!("\n\nTool result: No tables found in {database}"));
                    }
                    Ok(entries) => {
                        let lines: Vec<String> = entries.iter().take(20).enumerate().map(|(i, e)| {
                            format!("  {}. {} — {} rows, {} MB total ({} MB data + {} MB indexes)", i + 1, e.table, e.rows, e.total_size_mb, e.data_size_mb, e.index_size_mb)
                        }).collect();
                        let more = if entries.len() > 20 { format!("\n... and {} more tables", entries.len() - 20) } else { "".to_string() };
                        let result = format!("Table size ranking for '{}':\n{}{}", database, lines.join("\n"), more);
                        conversation.push_str(&format!("\n\nTool result:\n{result}"));
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "get_active_connections" => {
            match schema::get_active_connections().await {
                Ok(conns) if conns.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No active connections found"));
                }
                Ok(conns) => {
                    let lines: Vec<String> = conns.iter().take(20).map(|c| {
                        let db = c.database.as_deref().unwrap_or("NULL");
                        let state = c.state.as_deref().unwrap_or("");
                        let info_preview = c.info.as_deref().map(|i| if i.len() > 50 { format!("{}...", &i[..50]) } else { i.to_string() }).unwrap_or_default();
                        format!("  #{} {}@{} [{}] db={} time={}s{}{}", c.id, c.user, c.host, c.command, db, c.time,
                            if !state.is_empty() { format!(" state={}", state) } else { "".to_string() },
                            if !info_preview.is_empty() { format!(" query={}", info_preview) } else { "".to_string() }
                        )
                    }).collect();
                    let total = conns.len();
                    let more = if total > 20 { format!("\n... and {} more", total - 20) } else { "".to_string() };
                    let result = format!("Active connections ({} total):\n{}{}", total, lines.join("\n"), more);
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_slow_queries" => {
            let limit = tool_call.get("limit").and_then(|v| v.as_str())
                .and_then(|v| v.parse::<usize>().ok()).unwrap_or(10);
            match schema::get_slow_queries(limit).await {
                Ok(queries) if queries.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No slow queries found (performance_schema may be disabled or no queries recorded)"));
                }
                Ok(queries) => {
                    let lines: Vec<String> = queries.iter().enumerate().map(|(i, q)| {
                        let query_preview = if q.query.len() > 100 { format!("{}...", &q.query[..100]) } else { q.query.clone() };
                        format!("  {}. [{}x] avg {:.2}ms, total {:.2}ms, examined {} rows: {}", i + 1, q.exec_count, q.avg_timer_ms, q.total_timer_ms, q.rows_examined, query_preview)
                    }).collect();
                    let result = format!("Top {} slow queries:\n{}", queries.len(), lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_table_partitions" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_table_partitions(database, table).await {
                Ok(parts) if parts.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: {database}.{table} is not partitioned"));
                }
                Ok(parts) => {
                    let lines: Vec<String> = parts.iter().map(|p| {
                        let method = if p.method != "NONE" { format!(" [{}]", p.method) } else { "".to_string() };
                        let desc = p.description.as_deref().unwrap_or("");
                        format!("  {}{}: {} rows, {} bytes{}", p.name, method, p.rows, p.data_length, if !desc.is_empty() { format!(" ({})", desc) } else { "".to_string() })
                    }).collect();
                    let result = format!("Partitions for {database}.{table}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_column_charset" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_column_charset_info(database, table).await {
                Ok(infos) if infos.is_empty() => {
                    conversation.push_str(&format!("\n\nTool result: No columns found for {database}.{table}"));
                }
                Ok(infos) => {
                    let lines: Vec<String> = infos.iter().map(|i| {
                        let cs = i.character_set.as_deref().unwrap_or("-");
                        let coll = i.collation.as_deref().unwrap_or("-");
                        format!("  {} [{}]: charset={}, collation={}", i.column, i.data_type, cs, coll)
                    }).collect();
                    let result = format!("Charset info for {database}.{table}:\n{}", lines.join("\n"));
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        "get_data_type_summary" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            if database.is_empty() {
                conversation.push_str(&format!("\n\nTool error: database parameter is required"));
            } else {
                match schema::get_data_type_summary(database).await {
                    Ok(summary) if summary.is_empty() => {
                        conversation.push_str(&format!("\n\nTool result: No data found for {database}"));
                    }
                    Ok(summary) => {
                        let lines: Vec<String> = summary.iter().map(|s| {
                            let tables_preview = if s.tables.len() > 5 { format!("{}... ({} tables)", s.tables[..5].join(", "), s.tables.len()) } else { s.tables.join(", ") };
                            format!("  {}: {} columns in {}", s.data_type, s.column_count, tables_preview)
                        }).collect();
                        let result = format!("Data type summary for '{}':\n{}", database, lines.join("\n"));
                        conversation.push_str(&format!("\n\nTool result:\n{result}"));
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "find_orphan_records" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            let column = tool_call.get("column").and_then(|v| v.as_str()).unwrap_or("");
            let ref_database = tool_call.get("ref_database").and_then(|v| v.as_str()).unwrap_or(database);
            let ref_table = tool_call.get("ref_table").and_then(|v| v.as_str()).unwrap_or("");
            let ref_column = tool_call.get("ref_column").and_then(|v| v.as_str()).unwrap_or("");

            if database.is_empty() || table.is_empty() || column.is_empty() || ref_table.is_empty() || ref_column.is_empty() {
                conversation.push_str(&format!("\n\nTool error: Required parameters: database, table, column, ref_table, ref_column. Example: find_orphan_records(database='db1', table='orders', column='customer_id', ref_database='db1', ref_table='customers', ref_column='id')"));
            } else {
                match schema::find_orphan_records(database, table, column, ref_database, ref_table, ref_column).await {
                    Ok(count) => {
                        if count == 0 {
                            let result = format!("No orphan records in {database}.{table}.{column} → {ref_database}.{ref_table}.{ref_column}");
                            conversation.push_str(&format!("\n\nTool result: {result}"));
                        } else {
                            let result = format!("Found {} orphan records in {database}.{table}.{column} where no matching {ref_database}.{ref_table}.{ref_column} exists", count);
                            conversation.push_str(&format!("\n\nTool result: {result}"));
                        }
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "get_table_aliases" => {
            let similar = schema::get_similar_tables(all_schemas);
            if similar.is_empty() {
                conversation.push_str(&format!("\n\nTool result: No similarly named tables found across databases"));
            } else {
                let lines: Vec<String> = similar.iter().take(20).map(|(a, b, score)| {
                    format!("  {} ↔ {} (similarity: {:.0}%)", a, b, score * 100.0)
                }).collect();
                let result = format!("Similarly named tables:\n{}", lines.join("\n"));
                conversation.push_str(&format!("\n\nTool result:\n{result}"));
            }
        }

        "suggest_indexes" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            if database.is_empty() || table.is_empty() {
                conversation.push_str(&format!("\n\nTool error: database and table parameters are required"));
            } else {
                match schema::suggest_indexes(database, table).await {
                    Ok(suggestions) if suggestions.is_empty() => {
                        conversation.push_str(&format!("\n\nTool result: No index suggestions for {database}.{table} — all columns appear to be properly indexed"));
                    }
                    Ok(suggestions) => {
                        let lines: Vec<String> = suggestions.iter().map(|s| {
                            format!("  [{}] {}.{} — {}", s.priority, s.table, s.column, s.reason)
                        }).collect();
                        let result = format!("Index suggestions for {database}.{table}:\n{}", lines.join("\n"));
                        conversation.push_str(&format!("\n\nTool result:\n{result}"));
                    }
                    Err(e) => {
                        conversation.push_str(&format!("\n\nTool error: {e}"));
                    }
                }
            }
        }

        "get_create_options" => {
            let database = tool_call.get("database").and_then(|v| v.as_str()).unwrap_or("");
            let table = tool_call.get("table").and_then(|v| v.as_str()).unwrap_or("");
            match schema::get_create_options(database, table).await {
                Ok(opts) => {
                    let mut parts = vec![
                        format!("Table {database}.{table} options:"),
                        format!("  Engine: {}", opts.engine),
                        format!("  Row format: {}", opts.row_format),
                        format!("  Collation: {}", opts.table_collation),
                    ];
                    if !opts.create_options.is_empty() && opts.create_options != "none" {
                        parts.push(format!("  Create options: {}", opts.create_options));
                    }
                    if let Some(ai) = opts.auto_increment {
                        parts.push(format!("  Auto increment: {}", ai));
                    }
                    if let Some(ref pk) = opts.pack_keys {
                        parts.push(format!("  Pack keys: {}", pk));
                    }
                    if let Some(ck) = opts.checksum {
                        parts.push(format!("  Checksum: {}", ck));
                    }
                    if let Some(ref dkw) = opts.delay_key_write {
                        parts.push(format!("  Delay key write: {}", dkw));
                    }
                    let result = parts.join("\n");
                    conversation.push_str(&format!("\n\nTool result:\n{result}"));
                }
                Err(e) => {
                    conversation.push_str(&format!("\n\nTool error: {e}"));
                }
            }
        }

        _ => {
            conversation.push_str(&format!(
                "\n\nTool error: Unknown tool '{tool_name}'.\n\
                 Available tools: list_tables, get_table_schema, get_sample_data, cross_db_join, \
                 get_table_row_count, get_column_values, get_recent_data, search_data, \
                 get_table_ddl, get_indexes, get_foreign_keys, get_constraints, \
                 list_views, list_procedures, list_triggers, get_table_stats, get_table_status, \
                 analyze_table_health, find_relationships, find_similar_columns, compare_tables, \
                 get_server_info, get_variable, get_user_privileges, \
                 explain_query, validate_sql, security_check, \
                 get_primary_key, get_unique_keys, get_enum_values, get_auto_increment, \
                 get_referenced_by, count_nulls, get_column_stats, get_database_size, \
                 get_table_size_ranking, get_active_connections, get_slow_queries, \
                 get_table_partitions, get_column_charset, get_data_type_summary, \
                 find_orphan_records, get_table_aliases, suggest_indexes, get_create_options"
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

/// Robust sanitizer + extractor used for ALL SQL-returning paths (including all fallbacks).
/// Prevents NL text, Turkish fragments, "your_table", reasoning phrases from leaking into SQL.
fn sanitize_and_extract_sql(raw: &str, original_nl: &str) -> Result<String, String> {
    let mut sql = extract_sql(raw);

    // Extra strip of leading reasoning if extract didn't catch
    let mut cleaned_lines: Vec<&str> = Vec::new();
    let mut in_sql = false;
    for line in sql.lines() {
        let t = line.trim();
        if !in_sql {
            let u = t.to_uppercase();
            if u.starts_with("SELECT") || u.starts_with("INSERT") || u.starts_with("UPDATE")
                || u.starts_with("DELETE") || u.starts_with("CREATE") || u.starts_with("DROP")
                || u.starts_with("ALTER") || u.starts_with("WITH") || u.starts_with("EXPLAIN") {
                in_sql = true;
                cleaned_lines.push(line);
            } else if t.is_empty() || t.starts_with("--") || t.contains("I need to check") || t.contains("your_table") || t.contains("```") {
                continue;
            } else {
                continue; // drop leading prose
            }
        } else {
            if t.starts_with('{') || t.starts_with("```") { break; }
            cleaned_lines.push(line);
        }
    }
    sql = cleaned_lines.join("\n").trim().to_string();

    if sql.is_empty() {
        return Err("No SQL statement found after stripping reasoning".to_string());
    }

    let sql_lower = sql.to_lowercase();
    // Reject literal leaked phrases from rules or NL
    let forbidden = [
        "your_table", "your db", "<table_name>", "i need to check the schema first",
        "say 'i need", "placeholder", "if other _claim", "bu tarz bir sorguda"
    ];
    for phrase in &forbidden {
        if sql_lower.contains(phrase) {
            return Err(format!("Contains forbidden/leaked text: '{}'", phrase));
        }
    }

    // Cheap word-overlap rejection (defensive against mangled fallback)
    let nl_tokens: Vec<String> = original_nl
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 4 && !["select","insert","update","delete","create","drop","alter","from","where","table","database","query","data","recent","claim","limit","order","group","join","inner","left","right","the","and","for","with","that","this","have","been","used"].contains(&w.as_str()))
        .collect();
    let n = nl_tokens.len();
    if n >= 3 {
        let hits = nl_tokens.iter().filter(|w| sql_lower.contains(w.as_str())).count();
        let ratio = hits as f32 / n as f32;
        if ratio > 0.30 {
            return Err(format!("High NL word overlap ({:.0}% of {} distinctive words) — output rejected to prevent mangled SQL", ratio * 100.0, n));
        }
    }

    // Final keyword start guard
    let u = sql.to_uppercase();
    if !(u.starts_with("SELECT") || u.starts_with("INSERT") || u.starts_with("UPDATE")
        || u.starts_with("DELETE") || u.starts_with("CREATE") || u.starts_with("DROP")
        || u.starts_with("ALTER") || u.starts_with("WITH")) {
        return Err("Does not start with SELECT/INSERT/UPDATE/DELETE/CREATE/DROP/ALTER/WITH after sanitization".to_string());
    }

    Ok(sql)
}

/// Stable signature for repeat detection: "tool|sorted_key=val|..." (ignores JSON key order and Format A/B noise)
fn make_canonical_tool_sig(tool: &str, val: &serde_json::Value) -> String {
    let mut parts = vec![tool.trim().to_lowercase()];
    if let Some(obj) = val.as_object() {
        let mut keys: Vec<String> = obj.keys()
            .filter(|k| *k != "tool")
            .cloned()
            .collect();
        keys.sort();
        for k in keys {
            if let Some(v) = obj.get(&k) {
                let s = v.as_str()
                    .map(|ss| ss.to_string())
                    .unwrap_or_else(|| v.to_string().trim_matches('"').to_string());
                parts.push(format!("{}={}", k.trim().to_lowercase(), s.trim().to_lowercase()));
            }
        }
    }
    parts.join("|")
}
