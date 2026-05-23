use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use crate::db::schema::Schema;
use crate::error::AppError;
use crate::config;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ToolCall {
    id: String,
    r#type: String,
    function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ChatResponse {
    message: ChatMessage,
}

fn define_tools() -> Vec<serde_json::Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_tables",
                "description": "List all table names in a specific database",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "database": {
                            "type": "string",
                            "description": "The database name to list tables from"
                        }
                    },
                    "required": ["database"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "get_table_schema",
                "description": "Get column definitions for a specific table. Returns column names, types, nullability, and keys.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "database": {
                            "type": "string",
                            "description": "The database name"
                        },
                        "table": {
                            "type": "string",
                            "description": "The table name"
                        }
                    },
                    "required": ["database", "table"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "get_sample_data",
                "description": "Get 3 sample rows from a table to understand the data format and values",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "database": {
                            "type": "string",
                            "description": "The database name"
                        },
                        "table": {
                            "type": "string",
                            "description": "The table name"
                        }
                    },
                    "required": ["database", "table"]
                }
            }
        }),
    ]
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

pub async fn natural_language_to_sql_with_tools(
    natural_language: &str,
    all_schemas: &[Schema],
) -> Result<String, AppError> {
    let url = config::get_llm_url().await;
    let model = config::get_llm_model().await;

    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;

    let api_url = format!("{}/api/chat", url.trim_end_matches('/'));

    // Build system message with available databases only
    let db_list: Vec<&str> = all_schemas.iter().map(|s| s.database.as_str()).collect();
    let system_msg = format!(
        "You are a MySQL 5.6+ expert. Available databases: {}\n\n\
         The user will ask a question. Use the available tools to discover table schemas before writing SQL.\n\
         1. Use list_tables to find tables in a database\n\
         2. Use get_table_schema to understand column names and types\n\
         3. Use get_sample_data to see example values\n\
         4. When you have enough information, write the SQL query\n\n\
         Rules:\n\
         - Always use fully qualified table names: database.table\n\
         - Check for relationships between tables before joining\n\
         - Return ONLY the SQL query when done, no explanations",
        db_list.join(", ")
    );

    let mut messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_msg,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: natural_language.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    let tools = define_tools();
    let max_iterations = 10;

    for iteration in 0..max_iterations {
        eprintln!("[LLM] Iteration {}/{} — messages count: {}", iteration + 1, max_iterations, messages.len());

        let chat_request = json!({
            "model": model,
            "messages": messages,
            "tools": tools,
            "stream": false,
        });

        let response = client
            .post(&api_url)
            .json(&chat_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::QueryExecution(
                format!("Ollama chat returned {status} — {body}")
            ));
        }

        let chat_response: ChatResponse = response.json().await?;
        let assistant_msg = chat_response.message;

        eprintln!("[LLM] Assistant response: role={} tool_calls={} content={:?}",
            assistant_msg.role,
            assistant_msg.tool_calls.as_ref().map(|t| t.len()).unwrap_or(0),
            assistant_msg.content.chars().take(100).collect::<String>()
        );

        // Check if LLM made tool calls
        if let Some(tool_calls) = &assistant_msg.tool_calls {
            // Add assistant message to history
            messages.push(assistant_msg.clone());

            // Execute each tool call
            for tool_call in tool_calls {
                let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
                    .unwrap_or(json!({}));

                let result = match tool_call.function.name.as_str() {
                    "list_tables" => {
                        let database = args.get("database").and_then(|v| v.as_str()).unwrap_or("");
                        if let Some(schema) = all_schemas.iter().find(|s| s.database == database) {
                            let tables: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
                            format!("Tables in '{}': {}", database, tables.join(", "))
                        } else {
                            format!("Database '{}' not found in cache. Available: {}", database, db_list.join(", "))
                        }
                    }
                    "get_table_schema" => {
                        let database = args.get("database").and_then(|v| v.as_str()).unwrap_or("");
                        let table = args.get("table").and_then(|v| v.as_str()).unwrap_or("");
                        if let Some(schema) = all_schemas.iter().find(|s| s.database == database) {
                            if let Some(tbl) = schema.tables.iter().find(|t| t.name == table) {
                                let cols: Vec<String> = tbl.columns.iter().map(|c| {
                                    let key = if !c.column_key.is_empty() {
                                        format!(" [{}]", c.column_key)
                                    } else { "".to_string() };
                                    format!("  {} {}{}{}", c.name, c.column_type, key,
                                        if c.is_nullable { " NULL" } else { " NOT NULL" })
                                }).collect();
                                format!("Table {}.{} columns:\n{}", database, table, cols.join("\n"))
                            } else {
                                format!("Table '{}' not found in database '{}'", table, database)
                            }
                        } else {
                            format!("Database '{}' not found", database)
                        }
                    }
                    "get_sample_data" => {
                        let database = args.get("database").and_then(|v| v.as_str()).unwrap_or("");
                        let table = args.get("table").and_then(|v| v.as_str()).unwrap_or("");
                        format!("Sample data from {}.{}: (run the query to see actual data)", database, table)
                    }
                    _ => "Unknown tool".to_string(),
                };

                // Add tool result to messages
                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: result,
                    tool_calls: None,
                    tool_call_id: Some(tool_call.id.clone()),
                    name: Some(tool_call.function.name.clone()),
                });
            }
        } else {
            // No tool calls — LLM returned final answer
            let sql = assistant_msg.content.trim().to_string();
            let sql = sql
                .trim_start_matches("```sql")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
                .to_string();

            if sql.is_empty() {
                return Err(AppError::InvalidLlmResponse);
            }

            return Ok(sql);
        }
    }

    Err(AppError::InvalidLlmResponse)
}
