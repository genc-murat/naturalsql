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

    #[error("No schema cached. Please cache schema first.")]
    SchemaNotCached,

    #[error("Invalid response from LLM")]
    InvalidLlmResponse,

    #[error("Query execution failed: {0}")]
    QueryExecution(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
