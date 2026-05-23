use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub llm: LlmConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig {
                url: "http://localhost:11434".to_string(),
                model: "gemma4:e2b".to_string(),
            },
        }
    }
}

static CONFIG: Lazy<Arc<RwLock<AppConfig>>> = Lazy::new(|| {
    let config = load_config().unwrap_or_default();
    Arc::new(RwLock::new(config))
});

fn get_config_path() -> PathBuf {
    // Same directory as the binary / tauri app
    // During dev: src-tauri/config.json
    // During prod: we use a writable app data location
    let dir = dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("naturalsql");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("config.json")
}

fn load_config() -> Option<AppConfig> {
    let path = get_config_path();
    if path.exists() {
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        // Try to load from embedded config.json during dev
        let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.json");
        if dev_path.exists() {
            let content = fs::read_to_string(&dev_path).ok()?;
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;
    
    // Update in-memory config
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut guard = CONFIG.write().await;
        *guard = config.clone();
    });
    
    Ok(())
}

pub async fn get_config() -> AppConfig {
    CONFIG.read().await.clone()
}

pub async fn get_llm_url() -> String {
    CONFIG.read().await.llm.url.clone()
}

pub async fn get_llm_model() -> String {
    CONFIG.read().await.llm.model.clone()
}
