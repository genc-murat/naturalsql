use mysql_async::{prelude::*, Row};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::AppError;
use crate::db::connection::get_pool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub column_type: String,
    pub is_nullable: bool,
    pub column_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub database: String,
    pub tables: Vec<TableInfo>,
}

fn get_cache_path() -> PathBuf {
    let dir = dirs_next::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("naturalsql");
    std::fs::create_dir_all(&dir).ok();
    dir.join("schema_cache.sqlite")
}

pub async fn introspect_schema(database: &str) -> Result<Schema, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    // Get tables
    let tables: Vec<String> = conn
        .query_map(
            format!(
                "SELECT TABLE_NAME FROM information_schema.TABLES 
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_TYPE = 'BASE TABLE'",
                database
            ),
            |row: Row| {
                row.get(0).unwrap_or_default()
            },
        )
        .await?;

    let mut table_infos = Vec::new();

    for table_name in tables {
        let columns: Vec<ColumnInfo> = conn
            .query_map(
                format!(
                    "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY 
                     FROM information_schema.COLUMNS 
                     WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'
                     ORDER BY ORDINAL_POSITION",
                    database, table_name
                ),
                |row: Row| {
                    ColumnInfo {
                        name: row.get(0).unwrap_or_default(),
                        column_type: row.get(1).unwrap_or_default(),
                        is_nullable: {
                            let val: String = row.get(2).unwrap_or("YES".to_string());
                            val == "YES"
                        },
                        column_key: row.get(3).unwrap_or_default(),
                    }
                },
            )
            .await?;

        table_infos.push(TableInfo {
            name: table_name,
            columns,
        });
    }

    Ok(Schema {
        database: database.to_string(),
        tables: table_infos,
    })
}

pub fn cache_schema(schema: &Schema) -> Result<(), AppError> {
    let path = get_cache_path();
    let mut conn = Connection::open(path)?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_cache (
            id INTEGER PRIMARY KEY,
            database TEXT NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            column_type TEXT NOT NULL,
            is_nullable INTEGER NOT NULL,
            column_key TEXT NOT NULL
        );"
    )?;

    // Delete existing cache for this database only
    conn.execute("DELETE FROM schema_cache WHERE database = ?1", [&schema.database])?;

    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO schema_cache (database, table_name, column_name, column_type, is_nullable, column_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        )?;

        for table in &schema.tables {
            for col in &table.columns {
                stmt.execute(rusqlite::params![
                    schema.database,
                    table.name,
                    col.name,
                    col.column_type,
                    col.is_nullable as i32,
                    col.column_key
                ])?;
            }
        }
    }
    tx.commit()?;

    Ok(())
}

pub fn list_cached_databases() -> Result<Vec<String>, AppError> {
    let path = get_cache_path();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let conn = Connection::open(path)?;
    let mut stmt = conn.prepare("SELECT DISTINCT database FROM schema_cache ORDER BY database")?;
    let databases = stmt.query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(databases)
}

pub fn load_cached_schema(database: &str) -> Result<Option<Schema>, AppError> {
    let path = get_cache_path();
    if !path.exists() {
        return Ok(None);
    }

    let conn = Connection::open(path)?;

    // Check if database exists in cache
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM schema_cache WHERE database = ?1",
        [database],
        |row| row.get(0),
    )?;

    if !exists {
        return Ok(None);
    }

    // Load tables
    let mut table_map: std::collections::HashMap<String, Vec<ColumnInfo>> = std::collections::HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT table_name, column_name, column_type, is_nullable, column_key
         FROM schema_cache WHERE database = ?1 ORDER BY table_name"
    )?;

    let rows = stmt.query_map([database], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get::<_, i32>(3)? != 0,
            row.get(4)?,
        ))
    })?;

    for row in rows {
        let (table_name, col_name, col_type, is_nullable, col_key): (String, String, String, bool, String) = row?;
        let columns = table_map.entry(table_name.clone()).or_default();
        columns.push(ColumnInfo {
            name: col_name,
            column_type: col_type,
            is_nullable,
            column_key: col_key,
        });
    }

    let mut tables: Vec<TableInfo> = table_map
        .into_iter()
        .map(|(name, columns)| TableInfo { name, columns })
        .collect();
    tables.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Some(Schema {
        database: database.to_string(),
        tables,
    }))
}

pub fn load_all_cached_schemas() -> Result<Vec<Schema>, AppError> {
    let path = get_cache_path();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let conn = Connection::open(path)?;

    // Get all database names
    let mut db_stmt = conn.prepare("SELECT DISTINCT database FROM schema_cache ORDER BY database")?;
    let db_names: Vec<String> = db_stmt.query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let mut schemas = Vec::new();
    for db_name in &db_names {
        let mut table_map: std::collections::HashMap<String, Vec<ColumnInfo>> = std::collections::HashMap::new();

        let mut stmt = conn.prepare(
            "SELECT table_name, column_name, column_type, is_nullable, column_key
             FROM schema_cache WHERE database = ?1 ORDER BY table_name"
        )?;

        let rows = stmt.query_map([db_name], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i32>(3)? != 0,
                row.get(4)?,
            ))
        })?;

        for row in rows {
            let (table_name, col_name, col_type, is_nullable, col_key): (String, String, String, bool, String) = row?;
            let columns = table_map.entry(table_name.clone()).or_default();
            columns.push(ColumnInfo {
                name: col_name,
                column_type: col_type,
                is_nullable,
                column_key: col_key,
            });
        }

        let mut tables: Vec<TableInfo> = table_map
            .into_iter()
            .map(|(name, columns)| TableInfo { name, columns })
            .collect();
        tables.sort_by(|a, b| a.name.cmp(&b.name));

        schemas.push(Schema {
            database: db_name.clone(),
            tables,
        });
    }

    Ok(schemas)
}

#[allow(dead_code)]
pub fn remove_cached_schema(database: &str) -> Result<(), AppError> {
    let path = get_cache_path();
    if !path.exists() {
        return Ok(());
    }

    let conn = Connection::open(path)?;
    conn.execute("DELETE FROM schema_cache WHERE database = ?1", [database])?;
    Ok(())
}

pub fn format_schema_for_prompt(schema: &Schema) -> String {
    let mut result = format!("Database: {}\n\n", schema.database);
    for table in &schema.tables {
        result.push_str(&format!("Table: {}\n", table.name));
        for col in &table.columns {
            let nullable = if col.is_nullable { "NULL" } else { "NOT NULL" };
            let key = if !col.column_key.is_empty() {
                format!(" [{}]", col.column_key)
            } else {
                String::new()
            };
            result.push_str(&format!(
                "  - {} {} {}{}\n",
                col.name, col.column_type, nullable, key
            ));
        }
        result.push('\n');
    }
    result
}

/// Format ALL cached schemas with database.table notation for cross-database queries
pub fn format_all_schemas_for_prompt(schemas: &[Schema]) -> String {
    let mut result = String::new();
    for schema in schemas {
        result.push_str(&format!("Database: {}\n\n", schema.database));
        for table in &schema.tables {
            // Reference tables with database prefix for cross-db JOINs
            result.push_str(&format!("Table: {}.{}\n", schema.database, table.name));
            for col in &table.columns {
                let nullable = if col.is_nullable { "NULL" } else { "NOT NULL" };
                let key = if !col.column_key.is_empty() {
                    format!(" [{}]", col.column_key)
                } else {
                    String::new()
                };
                result.push_str(&format!(
                    "  - {}.{} {} {}{}\n",
                    schema.database, col.name, col.column_type, nullable, key
                ));
            }
            result.push('\n');
        }
    }
    result
}
