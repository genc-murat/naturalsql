mod config;
mod db;
mod error;
mod llm;
mod query;
mod commands;

use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            connect_db,
            disconnect_db,
            get_connection_status,
            cache_schema,
            get_cached_schema,
            nl_to_sql,
            execute_sql,
            get_llm_config,
            update_llm_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
