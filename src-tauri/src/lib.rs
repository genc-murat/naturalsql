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
            list_databases,
            cache_schema,
            get_cached_schema,
            list_cached_databases,
            remove_cached_schema,
            nl_to_sql,
            execute_sql,
            explain_sql,
            explain_sql_natural,
            fix_sql,
            optimize_sql,
            build_join,
            validate_cross_db_join,
            analyze_data,
            result_set_action,
            get_llm_config,
            update_llm_config,
            list_connections,
            save_connection_profile,
            delete_connection_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
