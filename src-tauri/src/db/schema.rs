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

/// Represents a foreign key relationship between tables (potentially cross-database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyRelation {
    pub from_database: String,
    pub from_table: String,
    pub from_column: String,
    pub to_database: String,
    pub to_table: String,
    pub to_column: String,
    pub constraint_name: Option<String>,
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

/// Format a single schema for prompt (used for single-database queries)
#[allow(dead_code)]
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

/// Format schemas with relationship hints for cross-database JOINs
pub fn format_schemas_with_relationships(
    schemas: &[Schema],
    relations: &[ForeignKeyRelation],
) -> String {
    let mut result = String::new();

    // First, show available databases and tables
    result.push_str("## Available Databases and Tables\n\n");
    for schema in schemas {
        result.push_str(&format!("Database: {}\n", schema.database));
        let table_names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        if !table_names.is_empty() {
            result.push_str(&format!("  Tables: {}\n\n", table_names.join(", ")));
        }
    }

    // Then show relationship hints
    if !relations.is_empty() {
        result.push_str("## Known Relationships (for JOIN conditions)\n\n");
        for rel in relations {
            result.push_str(&format!(
                "{}.{}.{} → {}.{}.{}\n",
                rel.from_database, rel.from_table, rel.from_column,
                rel.to_database, rel.to_table, rel.to_column
            ));
        }
        result.push('\n');
    }

    // Detailed column info
    result.push_str("## Detailed Schema\n\n");
    for schema in schemas {
        for table in &schema.tables {
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

/// Introspect foreign key relationships across all cached databases
pub async fn introspect_foreign_keys(database: &str) -> Result<Vec<ForeignKeyRelation>, AppError> {
    use crate::db::connection::get_pool;
    use mysql_async::Row;

    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let relations: Vec<ForeignKeyRelation> = conn
        .query_map(
            format!(
                "SELECT
                    kcu.TABLE_SCHEMA AS from_db,
                    kcu.TABLE_NAME AS from_table,
                    kcu.COLUMN_NAME AS from_column,
                    kcu.REFERENCED_TABLE_SCHEMA AS to_db,
                    kcu.REFERENCED_TABLE_NAME AS to_table,
                    kcu.REFERENCED_COLUMN_NAME AS to_column,
                    kcu.CONSTRAINT_NAME AS constraint_name
                 FROM information_schema.KEY_COLUMN_USAGE kcu
                 JOIN information_schema.TABLE_CONSTRAINTS tc
                     ON kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME
                     AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA
                     AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY'
                 WHERE kcu.TABLE_SCHEMA = '{}'
                     AND kcu.REFERENCED_TABLE_NAME IS NOT NULL",
                database
            ),
            |row: Row| {
                ForeignKeyRelation {
                    from_database: row.get(0).unwrap_or_default(),
                    from_table: row.get(1).unwrap_or_default(),
                    from_column: row.get(2).unwrap_or_default(),
                    to_database: row.get(3).unwrap_or_default(),
                    to_table: row.get(4).unwrap_or_default(),
                    to_column: row.get(5).unwrap_or_default(),
                    constraint_name: row.get(6),
                }
            },
        )
        .await?;

    Ok(relations)
}

/// Find potential cross-database JOIN relationships by column name matching
pub fn find_cross_database_relationships(
    schemas: &[Schema],
) -> Vec<ForeignKeyRelation> {
    let mut relations = Vec::new();

    // Build a map of column names to their locations
    let mut column_locations: std::collections::HashMap<String, Vec<(&str, &str, &str)>> =
        std::collections::HashMap::new();

    for schema in schemas {
        for table in &schema.tables {
            for col in &table.columns {
                // Look for common FK patterns: _id, id, reference columns
                if col.column_key == "PRI"
                    || col.name.ends_with("_id")
                    || col.name == "id"
                {
                    column_locations
                        .entry(col.name.clone())
                        .or_default()
                        .push((&schema.database, &table.name, &col.name));
                }
            }
        }
    }

    // Find columns with the same name across different databases
    for (col_name, locations) in &column_locations {
        if locations.len() < 2 {
            continue;
        }

        // Create relations between different database locations
        for i in 0..locations.len() {
            for j in (i + 1)..locations.len() {
                let (db1, table1, col1) = locations[i];
                let (db2, table2, col2) = locations[j];

                // Only create cross-database relations
                if db1 != db2 {
                    relations.push(ForeignKeyRelation {
                        from_database: db1.to_string(),
                        from_table: table1.to_string(),
                        from_column: col1.to_string(),
                        to_database: db2.to_string(),
                        to_table: table2.to_string(),
                        to_column: col2.to_string(),
                        constraint_name: Some(format!(
                            "potential_{}_{}_{}",
                            col_name, table1, table2
                        )),
                    });
                }
            }
        }
    }

    relations
}
