use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database connection failed: {0}")]
    Connection(#[from] mysql_async::UrlError),

    #[error("MySQL error: {0}")]
    MySql(#[from] mysql_async::Error),

    #[error("Ollama request failed: {0}")]
    Ollama(#[from] reqwest::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Not connected to any database")]
    NotConnected,

    #[error("No schema cached. Please cache at least one database schema first.")]
    SchemaNotCached,

    #[error("LLM returned an empty response. Try again or check your Ollama model.")]
    InvalidLlmResponse,

    #[error("Query execution failed: {0}")]
    QueryExecution(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Query cancelled by user")]
    QueryCancelled,

    #[error("Streaming error: {0}")]
    Streaming(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // For reqwest errors, provide more helpful messages
        if let AppError::Ollama(e) = self {
            let msg = if e.is_connect() {
                "Cannot connect to Ollama. Make sure Ollama is running on the configured URL (default: http://localhost:11434). Run 'ollama serve' to start it."
            } else if e.is_timeout() {
                "Ollama request timed out. The model may be loading or the server is unresponsive."
            } else if e.is_status() {
                &format!("Ollama returned an HTTP error: {}", e)
            } else {
                &format!("Ollama request failed: {}", e)
            };
            return serializer.serialize_str(msg);
        }

        serializer.serialize_str(&self.to_string())
    }
}
