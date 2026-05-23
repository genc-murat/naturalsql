use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub name: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub connections: Vec<ConnectionProfile>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig {
                url: "http://localhost:11434".to_string(),
                model: "gemma4:e2b".to_string(),
            },
            connections: Vec::new(),
        }
    }
}

// Use a synchronous Mutex for in-memory state - file I/O is the slow part anyway
pub static CONFIG: Mutex<AppConfig> = Mutex::new(AppConfig {
    llm: LlmConfig {
        url: String::new(),
        model: String::new(),
    },
    connections: Vec::new(),
});

static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn ensure_initialized() {
    if !INITIALIZED.load(std::sync::atomic::Ordering::SeqCst) {
        let config = load_config().unwrap_or_default();
        let mut guard = CONFIG.lock().unwrap();
        *guard = config;
        INITIALIZED.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

fn get_config_path() -> PathBuf {
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
        let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.json");
        if dev_path.exists() {
            let content = fs::read_to_string(&dev_path).ok()?;
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    }
}

fn save_config_sync(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;

    // Update in-memory state
    let mut guard = CONFIG.lock().map_err(|e| e.to_string())?;
    *guard = config.clone();

    Ok(())
}

pub async fn get_config() -> AppConfig {
    ensure_initialized();
    let guard = CONFIG.lock().unwrap();
    guard.clone()
}

pub async fn get_llm_url() -> String {
    ensure_initialized();
    let guard = CONFIG.lock().unwrap();
    guard.llm.url.clone()
}

pub async fn get_llm_model() -> String {
    ensure_initialized();
    let guard = CONFIG.lock().unwrap();
    guard.llm.model.clone()
}

pub async fn get_connections() -> Vec<ConnectionProfile> {
    ensure_initialized();
    let guard = CONFIG.lock().unwrap();
    guard.connections.clone()
}

pub async fn save_connection(profile: ConnectionProfile) -> Result<(), String> {
    ensure_initialized();
    let mut config = {
        let guard = CONFIG.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };

    // Check if profile with same name exists
    if let Some(existing) = config.connections.iter_mut().find(|c| c.name == profile.name) {
        *existing = profile;
    } else {
        config.connections.push(profile);
    }

    save_config_sync(&config)
}

pub async fn delete_connection(name: String) -> Result<(), String> {
    ensure_initialized();
    let mut config = {
        let guard = CONFIG.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };

    config.connections.retain(|c| c.name != name);
    save_config_sync(&config)
}
