use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

pub async fn natural_language_to_sql(
    natural_language: &str,
    schema_context: &str,
    model: Option<&str>,
) -> Result<String, AppError> {
    let model = model.unwrap_or("gemma4:e2b");
    
    let prompt = format!(
        "You are a MySQL 5.6+ expert. Given the database schema below, convert the user's natural language question into a valid MySQL SQL query.\n\
         Only return the SQL query, no explanations, no markdown formatting, no backticks.\n\n\
         {}\n\
         Question: {}\n\n\
         SQL Query:",
        schema_context, natural_language
    );

    let request = OllamaRequest {
        model: model.to_string(),
        prompt,
        stream: false,
    };

    let client = Client::new();
    let response = client
        .post("http://localhost:11434/api/generate")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(AppError::QueryExecution(
            format!("Ollama returned status: {}", response.status())
        ));
    }

    let ollama_response: OllamaResponse = response.json().await?;
    
    // Clean up the response - remove markdown code blocks if present
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
