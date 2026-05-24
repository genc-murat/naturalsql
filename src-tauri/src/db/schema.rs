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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub column: String,
    pub non_unique: bool,
    pub seq: u16,
    pub index_type: String,
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

// ========================
// Enhanced metadata helpers
// ========================

/// Get indexes on a table
pub async fn get_table_indexes(database: &str, table: &str) -> Result<Vec<IndexInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let indexes = conn
        .query_map(
            format!(
                "SELECT INDEX_NAME, COLUMN_NAME, NON_UNIQUE, SEQ_IN_INDEX, INDEX_TYPE
                 FROM information_schema.STATISTICS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'
                 ORDER BY INDEX_NAME, SEQ_IN_INDEX",
                database, table
            ),
            |row: Row| IndexInfo {
                name: row.get(0).unwrap_or_default(),
                column: row.get(1).unwrap_or_default(),
                non_unique: row.get::<u64, _>(2).map(|v| v == 1).unwrap_or(true),
                seq: row.get(3).unwrap_or(1),
                index_type: row.get(4).unwrap_or_default(),
            },
        )
        .await?;

    Ok(indexes)
}

/// Get foreign keys for a table
pub async fn get_table_foreign_keys(database: &str, table: &str) -> Result<Vec<ForeignKeyRelation>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let fks = conn
        .query_map(
            format!(
                "SELECT
                    kcu.COLUMN_NAME,
                    kcu.REFERENCED_TABLE_NAME,
                    kcu.REFERENCED_COLUMN_NAME,
                    kcu.CONSTRAINT_NAME,
                    kcu.REFERENCED_TABLE_SCHEMA
                 FROM information_schema.KEY_COLUMN_USAGE kcu
                 WHERE kcu.TABLE_SCHEMA = '{}'
                     AND kcu.TABLE_NAME = '{}'
                     AND kcu.REFERENCED_TABLE_NAME IS NOT NULL",
                database, table
            ),
            |row: Row| ForeignKeyRelation {
                from_database: database.to_string(),
                from_table: table.to_string(),
                from_column: row.get(0).unwrap_or_default(),
                to_database: row.get(4).unwrap_or_default(),
                to_table: row.get(1).unwrap_or_default(),
                to_column: row.get(2).unwrap_or_default(),
                constraint_name: row.get(3),
            },
        )
        .await?;

    Ok(fks)
}

/// Get all constraints for a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintInfo {
    pub name: String,
    pub column: String,
    pub constraint_type: String, // PRIMARY KEY, UNIQUE, FOREIGN KEY, CHECK
}

pub async fn get_table_constraints(database: &str, table: &str) -> Result<Vec<ConstraintInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let constraints = conn
        .query_map(
            format!(
                "SELECT tc.CONSTRAINT_NAME, kcu.COLUMN_NAME, tc.CONSTRAINT_TYPE
                 FROM information_schema.TABLE_CONSTRAINTS tc
                 JOIN information_schema.KEY_COLUMN_USAGE kcu
                     ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME
                     AND tc.TABLE_SCHEMA = kcu.TABLE_SCHEMA
                     AND tc.TABLE_NAME = kcu.TABLE_NAME
                 WHERE tc.TABLE_SCHEMA = '{}' AND tc.TABLE_NAME = '{}'
                 ORDER BY tc.CONSTRAINT_TYPE, tc.CONSTRAINT_NAME",
                database, table
            ),
            |row: Row| ConstraintInfo {
                name: row.get(0).unwrap_or_default(),
                column: row.get(1).unwrap_or_default(),
                constraint_type: row.get(2).unwrap_or_default(),
            },
        )
        .await?;

    Ok(constraints)
}

/// Get views in a database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewInfo {
    pub name: String,
    pub definition: String,
}

pub async fn get_database_views(database: &str) -> Result<Vec<ViewInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let views = conn
        .query_map(
            format!(
                "SELECT TABLE_NAME, VIEW_DEFINITION
                 FROM information_schema.VIEWS
                 WHERE TABLE_SCHEMA = '{}'
                 ORDER BY TABLE_NAME",
                database
            ),
            |row: Row| ViewInfo {
                name: row.get(0).unwrap_or_default(),
                definition: row.get::<String, _>(1).unwrap_or_default(),
            },
        )
        .await?;

    Ok(views)
}

/// Get stored procedures in a database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcedureInfo {
    pub name: String,
    pub definer: String,
    pub created: String,
    pub params: String,
}

pub async fn get_database_procedures(database: &str) -> Result<Vec<ProcedureInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let procs = conn
        .query_map(
            format!(
                "SELECT ROUTINE_NAME, DEFINER, CREATED,
                        IFNULL(PARAM_LIST, '') as params
                 FROM information_schema.ROUTINES
                 WHERE ROUTINE_SCHEMA = '{}' AND ROUTINE_TYPE = 'PROCEDURE'
                 ORDER BY ROUTINE_NAME",
                database
            ),
            |row: Row| ProcedureInfo {
                name: row.get(0).unwrap_or_default(),
                definer: row.get(1).unwrap_or_default(),
                created: row.get(2).unwrap_or_default(),
                params: row.get(3).unwrap_or_default(),
            },
        )
        .await?;

    Ok(procs)
}

/// Get triggers in a database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerInfo {
    pub name: String,
    pub table: String,
    pub event: String,
    pub timing: String,
    pub statement: String,
}

pub async fn get_database_triggers(database: &str) -> Result<Vec<TriggerInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let triggers = conn
        .query_map(
            format!(
                "SELECT TRIGGER_NAME, EVENT_OBJECT_TABLE, EVENT_MANIPULATION,
                        ACTION_TIMING, ACTION_STATEMENT
                 FROM information_schema.TRIGGERS
                 WHERE TRIGGER_SCHEMA = '{}'
                 ORDER BY EVENT_OBJECT_TABLE, ACTION_TIMING",
                database
            ),
            |row: Row| TriggerInfo {
                name: row.get(0).unwrap_or_default(),
                table: row.get(1).unwrap_or_default(),
                event: row.get(2).unwrap_or_default(),
                timing: row.get(3).unwrap_or_default(),
                statement: row.get::<String, _>(4).unwrap_or_default(),
            },
        )
        .await?;

    Ok(triggers)
}

/// Get table statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStats {
    pub row_count: u64,
    pub data_size_mb: f64,
    pub index_size_mb: f64,
    pub avg_row_length: u64,
}

pub async fn get_table_statistics(database: &str, table: &str) -> Result<TableStats, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let result: Option<(Option<u64>, Option<f64>, Option<f64>, Option<u64>)> = conn
        .query_first(
            format!(
                "SELECT TABLE_ROWS,
                        ROUND(DATA_LENGTH / 1024.0 / 1024.0, 2) as data_mb,
                        ROUND(INDEX_LENGTH / 1024.0 / 1024.0, 2) as index_mb,
                        AVG_ROW_LENGTH
                 FROM information_schema.TABLES
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'",
                database, table
            ),
        )
        .await?;

    match result {
        Some((rows, data_mb, index_mb, avg_len)) => Ok(TableStats {
            row_count: rows.unwrap_or(0),
            data_size_mb: data_mb.unwrap_or(0.0),
            index_size_mb: index_mb.unwrap_or(0.0),
            avg_row_length: avg_len.unwrap_or(0),
        }),
        None => Err(AppError::QueryExecution(format!("Table {}.{} not found", database, table))),
    }
}

/// Find columns with the same name across databases (heuristic FK discovery)
pub async fn find_similar_columns(column_pattern: &str) -> Result<Vec<ColumnLocation>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let locations = conn
        .query_map(
            format!(
                "SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, COLUMN_KEY
                 FROM information_schema.COLUMNS
                 WHERE COLUMN_NAME LIKE '%{}%'
                 ORDER BY TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME",
                column_pattern
            ),
            |row: Row| ColumnLocation {
                database: row.get(0).unwrap_or_default(),
                table: row.get(1).unwrap_or_default(),
                column: row.get(2).unwrap_or_default(),
                column_type: row.get(3).unwrap_or_default(),
                column_key: row.get(4).unwrap_or_default(),
            },
        )
        .await?;

    Ok(locations)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnLocation {
    pub database: String,
    pub table: String,
    pub column: String,
    pub column_type: String,
    pub column_key: String,
}

/// Compare two tables' structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableComparison {
    pub left_only: Vec<String>,
    pub right_only: Vec<String>,
    pub common: Vec<String>,
    pub type_mismatches: Vec<(String, String, String)>, // column, left_type, right_type
}

pub fn compare_tables(
    left_schema: Option<&TableInfo>,
    right_schema: Option<&TableInfo>,
) -> TableComparison {
    let mut result = TableComparison {
        left_only: Vec::new(),
        right_only: Vec::new(),
        common: Vec::new(),
        type_mismatches: Vec::new(),
    };

    let empty_cols = Vec::new();
    let left_cols = left_schema.map_or(&empty_cols, |t| &t.columns);
    let right_cols = right_schema.map_or(&empty_cols, |t| &t.columns);

    let left_names: std::collections::HashSet<_> = left_cols.iter().map(|c| &c.name).collect();
    let right_names: std::collections::HashSet<_> = right_cols.iter().map(|c| &c.name).collect();

    result.left_only = left_names.difference(&right_names).map(|s| s.to_string()).collect();
    result.right_only = right_names.difference(&left_names).map(|s| s.to_string()).collect();
    result.common = left_names.intersection(&right_names).map(|s| s.to_string()).collect();

    // Check type mismatches for common columns
    for col_name in &result.common {
        if let (Some(left_col), Some(right_col)) = (
            left_cols.iter().find(|c| &c.name == col_name),
            right_cols.iter().find(|c| &c.name == col_name),
        ) {
            if left_col.column_type != right_col.column_type {
                result.type_mismatches.push((
                    col_name.clone(),
                    left_col.column_type.clone(),
                    right_col.column_type.clone(),
                ));
            }
        }
    }

    result
}

/// Get MySQL server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub current_user: String,
    pub current_database: String,
    pub character_set: String,
    pub collation: String,
    pub timezone: String,
    pub max_connections: u64,
}

pub async fn get_server_info() -> Result<ServerInfo, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let version: String = conn.query_first("SELECT VERSION()").await?.unwrap_or_default();
    let current_user: String = conn.query_first("SELECT CURRENT_USER()").await?.unwrap_or_default();
    let current_database: String = conn.query_first("SELECT DATABASE()").await?.unwrap_or_default();
    let character_set: String = conn.query_first("SELECT @@character_set_server").await?.unwrap_or_default();
    let collation: String = conn.query_first("SELECT @@collation_server").await?.unwrap_or_default();
    let timezone: String = conn.query_first("SELECT @@system_time_zone").await?.unwrap_or_default();
    let max_connections: u64 = conn.query_first("SELECT @@max_connections").await?.unwrap_or(151);

    Ok(ServerInfo {
        version,
        current_user,
        current_database,
        character_set,
        collation,
        timezone,
        max_connections,
    })
}

/// Get table status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStatus {
    pub engine: String,
    pub row_format: String,
    pub collation: String,
    pub auto_increment: Option<u64>,
    pub create_time: Option<String>,
    pub update_time: Option<String>,
}

pub async fn get_table_status(database: &str, table: &str) -> Result<TableStatus, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let rows: Vec<Row> = conn
        .exec(format!(
            "SELECT Engine, Row_format, Collation, Auto_increment, Create_time, Update_time
             FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'",
            database, table
        ), ())
        .await?;

    if let Some(row) = rows.into_iter().next() {
        Ok(TableStatus {
            engine: row.get(0).unwrap_or_default(),
            row_format: row.get(1).unwrap_or_default(),
            collation: row.get(2).unwrap_or_default(),
            auto_increment: row.get(3),
            create_time: row.get(4),
            update_time: row.get(5),
        })
    } else {
        Err(AppError::QueryExecution(format!("Table {}.{} not found", database, table)))
    }
}

/// Get approximate row count for a table (fast, uses table statistics)
pub async fn get_table_row_count(database: &str, table: &str) -> Result<u64, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let count: Option<u64> = conn
        .query_first(format!(
            "SELECT TABLE_ROWS FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'",
            database, table
        ))
        .await?;

    Ok(count.unwrap_or(0))
}

/// Get distinct values for a column (with limit to prevent large results)
pub async fn get_column_distinct_values(
    database: &str,
    table: &str,
    column: &str,
    limit: usize,
) -> Result<Vec<String>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let values: Vec<String> = conn
        .query_map(
            format!(
                "SELECT DISTINCT `{}` FROM `{}`.`{}`
                 WHERE `{}` IS NOT NULL
                 ORDER BY `{}`
                 LIMIT {}",
                column, database, table, column, column, limit
            ),
            |row: Row| {
                let val: mysql_async::Value = row.get(0).unwrap_or(mysql_async::Value::NULL);
                match val {
                    mysql_async::Value::NULL => String::new(),
                    mysql_async::Value::Bytes(b) => String::from_utf8_lossy(&b).to_string(),
                    mysql_async::Value::Int(v) => v.to_string(),
                    mysql_async::Value::UInt(v) => v.to_string(),
                    mysql_async::Value::Float(v) => v.to_string(),
                    mysql_async::Value::Double(v) => v.to_string(),
                    _ => "?".to_string(),
                }
            },
        )
        .await?;

    Ok(values)
}

/// Get recent data from a table (last N rows)
pub async fn get_recent_data(
    database: &str,
    table: &str,
    limit: usize,
) -> Result<(Vec<String>, Vec<Vec<String>>), AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    // First, try to find a suitable ordering column (id, created_at, etc.)
    let schema = load_cached_schema(database)?;
    let ordering_col = if let Some(schema) = schema {
        if let Some(tbl) = schema.tables.iter().find(|t| t.name == table) {
            // Prefer: id, created_at, created_date, timestamp, or first PRIMARY KEY
            let priority = ["id", "created_at", "created_date", "timestamp", "updated_at"];
            let found = priority.iter().find(|&p| tbl.columns.iter().any(|c| c.name == *p));
            if let Some(&col) = found {
                col.to_string()
            } else if let Some(pk) = tbl.columns.iter().find(|c| c.column_key == "PRI") {
                pk.name.clone()
            } else if !tbl.columns.is_empty() {
                tbl.columns[0].name.clone()
            } else {
                return Err(AppError::QueryExecution(format!("Table {}.{} has no columns", database, table)));
            }
        } else {
            return Err(AppError::QueryExecution(format!("Table {}.{} not found in cache", database, table)));
        }
    } else {
        return Err(AppError::QueryExecution(format!("Database {} not cached", database)));
    };

    let query = format!(
        "SELECT * FROM `{}`.`{}` ORDER BY `{}` DESC LIMIT {}",
        database, table, ordering_col, limit
    );

    let rows: Vec<mysql_async::Row> = conn.query(&query).await?;
    if rows.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let cols = rows[0].columns();
    let col_names: Vec<String> = cols.as_ref().iter().map(|c| c.name_str().to_string()).collect();

    let data_rows: Vec<Vec<String>> = rows.iter().map(|row| {
        col_names.iter().enumerate().map(|(i, _)| {
            match row.get_opt::<mysql_async::Value, usize>(i) {
                Some(Ok(v)) => match v {
                    mysql_async::Value::NULL => "NULL".to_string(),
                    mysql_async::Value::Bytes(b) => String::from_utf8_lossy(&b).chars().take(50).collect(),
                    mysql_async::Value::Int(v) => v.to_string(),
                    mysql_async::Value::UInt(v) => v.to_string(),
                    mysql_async::Value::Float(v) => v.to_string(),
                    mysql_async::Value::Double(v) => v.to_string(),
                    _ => "?".to_string(),
                },
                _ => "NULL".to_string(),
            }
        }).collect()
    }).collect();

    Ok((col_names, data_rows))
}

/// Search data in a table using LIKE pattern
pub async fn search_table_data(
    database: &str,
    table: &str,
    search_pattern: &str,
    columns: Option<&[String]>,
    limit: usize,
) -> Result<(Vec<String>, Vec<Vec<String>>, usize), AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    // Get column names to search in
    let schema = load_cached_schema(database)?;
    let search_cols = if let Some(cols) = columns {
        cols.to_vec()
    } else if let Some(schema) = schema {
        if let Some(tbl) = schema.tables.iter().find(|t| t.name == table) {
            tbl.columns.iter().filter(|c| {
                // Search in string columns only
                c.column_type.to_lowercase().contains("char")
                    || c.column_type.to_lowercase().contains("text")
                    || c.column_type.to_lowercase().contains("varchar")
            }).map(|c| c.name.clone()).collect()
        } else {
            return Err(AppError::QueryExecution(format!("Table {}.{} not found", database, table)));
        }
    } else {
        return Err(AppError::QueryExecution(format!("Database {} not cached", database)));
    };

    if search_cols.is_empty() {
        return Err(AppError::QueryExecution("No searchable columns found".to_string()));
    }

    // Build WHERE clause with LIKE for each column
    let where_clause = search_cols.iter()
        .map(|c| format!("`{}` LIKE '%{}%'", c, search_pattern.replace("'", "''")))
        .collect::<Vec<_>>()
        .join(" OR ");

    // Get total count
    let count_query = format!(
        "SELECT COUNT(*) FROM `{}`.`{}` WHERE {}",
        database, table, where_clause
    );
    let total_count: Option<u64> = conn.query_first(&count_query).await?;
    let total = total_count.unwrap_or(0) as usize;

    // Get limited results
    let data_query = format!(
        "SELECT * FROM `{}`.`{}` WHERE {} LIMIT {}",
        database, table, where_clause, limit
    );
    let rows: Vec<mysql_async::Row> = conn.query(&data_query).await?;

    if rows.is_empty() {
        return Ok((Vec::new(), Vec::new(), total));
    }

    let cols = rows[0].columns();
    let col_names: Vec<String> = cols.as_ref().iter().map(|c| c.name_str().to_string()).collect();

    let data_rows: Vec<Vec<String>> = rows.iter().map(|row| {
        col_names.iter().enumerate().map(|(i, _)| {
            match row.get_opt::<mysql_async::Value, usize>(i) {
                Some(Ok(v)) => match v {
                    mysql_async::Value::NULL => "NULL".to_string(),
                    mysql_async::Value::Bytes(b) => String::from_utf8_lossy(&b).chars().take(50).collect(),
                    mysql_async::Value::Int(v) => v.to_string(),
                    mysql_async::Value::UInt(v) => v.to_string(),
                    mysql_async::Value::Float(v) => v.to_string(),
                    mysql_async::Value::Double(v) => v.to_string(),
                    _ => "?".to_string(),
                },
                _ => "NULL".to_string(),
            }
        }).collect()
    }).collect();

    Ok((col_names, data_rows, total))
}

/// Get CREATE TABLE DDL for a table
pub async fn get_table_ddl(database: &str, table: &str) -> Result<String, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    // SHOW CREATE TABLE returns two columns: Table and Create Table
    // We need to extract the second column
    let rows: Vec<mysql_async::Row> = conn
        .query(format!("SHOW CREATE TABLE `{}`.`{}`", database, table))
        .await?;

    if let Some(row) = rows.into_iter().next() {
        // Second column is the CREATE TABLE statement
        match row.get_opt::<String, usize>(1) {
            Some(Ok(ddl)) => Ok(ddl),
            _ => Err(AppError::QueryExecution("Failed to get DDL".to_string())),
        }
    } else {
        Err(AppError::QueryExecution(format!("Table {}.{} not found", database, table)))
    }
}

/// Get MySQL server variable value
pub async fn get_server_variable(name: &str) -> Result<String, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let value: Option<String> = conn
        .query_first(format!(
            "SELECT @@{}",
            name
        ))
        .await?;

    Ok(value.unwrap_or_else(|| "NULL".to_string()))
}

/// Get user privileges for current database
pub async fn get_user_privileges(database: &str) -> Result<Vec<String>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let privileges: Vec<String> = conn
        .query_map(
            format!(
                "SELECT PRIVILEGE_TYPE, TABLE_NAME
                 FROM information_schema.TABLE_PRIVILEGES
                 WHERE TABLE_SCHEMA = '{}'
                 ORDER BY TABLE_NAME, PRIVILEGE_TYPE",
                database
            ),
            |row: Row| {
                let priv_type: String = row.get(0).unwrap_or_default();
                let table: String = row.get(1).unwrap_or_default();
                format!("{} on {}", priv_type, table)
            },
        )
        .await?;

    if privileges.is_empty() {
        // Fallback: try SHOW GRANTS
        let grants: Vec<String> = conn
            .query_map("SHOW GRANTS", |row: Row| {
                row.get(0).unwrap_or_default()
            })
            .await?;
        return Ok(grants);
    }

    Ok(privileges)
}

/// Analyze table health (fragmentation, engine info)
pub async fn analyze_table_health(database: &str, table: &str) -> Result<TableHealth, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let rows: Vec<mysql_async::Row> = conn
        .query(format!(
            "SELECT ENGINE, TABLE_ROWS, DATA_LENGTH, INDEX_LENGTH,
                    DATA_FREE, AVG_ROW_LENGTH, DATA_FREE / DATA_LENGTH * 100 as frag_pct
             FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'",
            database, table
        ))
        .await?;

    if let Some(row) = rows.into_iter().next() {
        let engine: String = row.get(0).unwrap_or_default();
        let data_rows: u64 = row.get(1).unwrap_or(0);
        let data_length: u64 = row.get(2).unwrap_or(0);
        let index_length: u64 = row.get(3).unwrap_or(0);
        let data_free: u64 = row.get(4).unwrap_or(0);
        let avg_row_length: u64 = row.get(5).unwrap_or(0);
        let frag_pct: f64 = row.get(6).unwrap_or(0.0);

        Ok(TableHealth {
            engine,
            row_count: data_rows,
            data_size_bytes: data_length,
            index_size_bytes: index_length,
            free_space_bytes: data_free,
            avg_row_length,
            fragmentation_percent: frag_pct.round() as f64,
        })
    } else {
        Err(AppError::QueryExecution(format!("Table {}.{} not found", database, table)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableHealth {
    pub engine: String,
    pub row_count: u64,
    pub data_size_bytes: u64,
    pub index_size_bytes: u64,
    pub free_space_bytes: u64,
    pub avg_row_length: u64,
    pub fragmentation_percent: f64,
}

pub async fn get_primary_key_columns(database: &str, table: &str) -> Result<Vec<String>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let cols: Vec<String> = conn
        .query_map(
            format!(
                "SELECT COLUMN_NAME FROM information_schema.COLUMNS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' AND COLUMN_KEY = 'PRI'
                 ORDER BY ORDINAL_POSITION",
                database, table
            ),
            |row: Row| row.get(0).unwrap_or_default(),
        )
        .await?;

    Ok(cols)
}

pub async fn get_unique_key_columns(database: &str, table: &str) -> Result<Vec<IndexInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let indexes = conn
        .query_map(
            format!(
                "SELECT INDEX_NAME, COLUMN_NAME, NON_UNIQUE, SEQ_IN_INDEX, INDEX_TYPE
                 FROM information_schema.STATISTICS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' AND NON_UNIQUE = 0
                 ORDER BY INDEX_NAME, SEQ_IN_INDEX",
                database, table
            ),
            |row: Row| IndexInfo {
                name: row.get(0).unwrap_or_default(),
                column: row.get(1).unwrap_or_default(),
                non_unique: false,
                seq: row.get(3).unwrap_or(1),
                index_type: row.get(4).unwrap_or_default(),
            },
        )
        .await?;

    Ok(indexes)
}

pub async fn get_enum_values(database: &str, table: &str, column: &str) -> Result<Vec<String>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let col_type: Option<String> = conn
        .query_first(format!(
            "SELECT COLUMN_TYPE FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' AND COLUMN_NAME = '{}'",
            database, table, column
        ))
        .await?;

    let col_type = col_type.unwrap_or_default();
    let col_type_lower = col_type.to_lowercase();

    if !col_type_lower.starts_with("enum(") && !col_type_lower.starts_with("set(") {
        return Ok(Vec::new());
    }

    let start = match col_type.find('(') {
        Some(i) => i + 1,
        None => return Ok(Vec::new()),
    };
    let end = match col_type.rfind(')') {
        Some(i) => i,
        None => return Ok(Vec::new()),
    };

    let inner = &col_type[start..end];
    let values: Vec<String> = inner
        .split(',')
        .map(|v| v.trim().trim_matches('\'').to_string())
        .collect();

    Ok(values)
}

pub async fn get_auto_increment_value(database: &str, table: &str) -> Result<Option<u64>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let result: Option<u64> = conn
        .query_first(format!(
            "SELECT AUTO_INCREMENT FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'",
            database, table
        ))
        .await?;

    Ok(result)
}

pub async fn get_referencing_tables(database: &str, table: &str) -> Result<Vec<ForeignKeyRelation>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let refs: Vec<ForeignKeyRelation> = conn
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
                 WHERE kcu.REFERENCED_TABLE_SCHEMA = '{}'
                     AND kcu.REFERENCED_TABLE_NAME = '{}'",
                database, table
            ),
            |row: Row| ForeignKeyRelation {
                from_database: row.get(0).unwrap_or_default(),
                from_table: row.get(1).unwrap_or_default(),
                from_column: row.get(2).unwrap_or_default(),
                to_database: row.get(3).unwrap_or_default(),
                to_table: row.get(4).unwrap_or_default(),
                to_column: row.get(5).unwrap_or_default(),
                constraint_name: row.get(6),
            },
        )
        .await?;

    Ok(refs)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NullCountInfo {
    pub column: String,
    pub null_count: u64,
    pub total_count: u64,
}

pub async fn count_nulls_per_column(database: &str, table: &str) -> Result<Vec<NullCountInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let columns: Vec<String> = conn
        .query_map(
            format!(
                "SELECT COLUMN_NAME FROM information_schema.COLUMNS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'
                 ORDER BY ORDINAL_POSITION",
                database, table
            ),
            |row: Row| row.get(0).unwrap_or_default(),
        )
        .await?;

    let total: u64 = conn
        .query_first(format!("SELECT COUNT(*) FROM `{}`.`{}`", database, table))
        .await?
        .unwrap_or(0);

    let mut results = Vec::new();
    for col in &columns {
        let null_count: u64 = conn
            .query_first(format!(
                "SELECT COUNT(*) FROM `{}`.`{}` WHERE `{}` IS NULL",
                database, table, col
            ))
            .await?
            .unwrap_or(0);
        results.push(NullCountInfo {
            column: col.clone(),
            null_count,
            total_count: total,
        });
    }

    Ok(results)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStat {
    pub column: String,
    pub data_type: String,
    pub min_val: Option<String>,
    pub max_val: Option<String>,
    pub avg_val: Option<f64>,
    pub count: u64,
    pub distinct_count: u64,
}

pub async fn get_column_statistics(database: &str, table: &str) -> Result<Vec<ColumnStat>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let columns: Vec<(String, String)> = conn
        .query_map(
            format!(
                "SELECT COLUMN_NAME, DATA_TYPE FROM information_schema.COLUMNS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'
                 ORDER BY ORDINAL_POSITION",
                database, table
            ),
            |row: Row| (row.get(0).unwrap_or_default(), row.get(1).unwrap_or_default()),
        )
        .await?;

    let numeric_types = [
        "int", "tinyint", "smallint", "mediumint", "bigint",
        "float", "double", "decimal", "numeric",
        "date", "datetime", "timestamp", "time", "year",
    ];

    let mut results = Vec::new();
    for (col, dtype) in &columns {
        let is_numeric = numeric_types.contains(&dtype.as_str());

        let count: u64 = conn
            .query_first(format!(
                "SELECT COUNT(*) FROM `{}`.`{}` WHERE `{}` IS NOT NULL",
                database, table, col
            ))
            .await?
            .unwrap_or(0);

        let distinct_count: u64 = conn
            .query_first(format!(
                "SELECT COUNT(DISTINCT `{}`) FROM `{}`.`{}`",
                col, database, table
            ))
            .await?
            .unwrap_or(0);

        let (min_val, max_val, avg_val) = if is_numeric && count > 0 {
            let min_val: Option<String> = conn
                .query_first(format!(
                    "SELECT CAST(MIN(`{}`) AS CHAR) FROM `{}`.`{}`",
                    col, database, table
                ))
                .await?;
            let max_val: Option<String> = conn
                .query_first(format!(
                    "SELECT CAST(MAX(`{}`) AS CHAR) FROM `{}`.`{}`",
                    col, database, table
                ))
                .await?;
            let avg_val: Option<f64> = conn
                .query_first(format!(
                    "SELECT AVG(`{}`) FROM `{}`.`{}`",
                    col, database, table
                ))
                .await?;
            (min_val, max_val, avg_val)
        } else if count > 0 {
            let min_val: Option<String> = conn
                .query_first(format!(
                    "SELECT MIN(`{}`) FROM `{}`.`{}`",
                    col, database, table
                ))
                .await?;
            let max_val: Option<String> = conn
                .query_first(format!(
                    "SELECT MAX(`{}`) FROM `{}`.`{}`",
                    col, database, table
                ))
                .await?;
            (min_val, max_val, None)
        } else {
            (None, None, None)
        };

        results.push(ColumnStat {
            column: col.clone(),
            data_type: dtype.clone(),
            min_val,
            max_val,
            avg_val,
            count,
            distinct_count,
        });
    }

    Ok(results)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSize {
    pub total_size_mb: f64,
    pub data_size_mb: f64,
    pub index_size_mb: f64,
    pub table_count: u64,
    pub total_rows: u64,
}

pub async fn get_database_size(database: &str) -> Result<DatabaseSize, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let result: Option<(Option<f64>, Option<f64>, Option<f64>, Option<u64>, Option<u64>)> = conn
        .query_first(format!(
            "SELECT
                ROUND((SUM(DATA_LENGTH) + SUM(INDEX_LENGTH)) / 1024.0 / 1024.0, 2) as total_mb,
                ROUND(SUM(DATA_LENGTH) / 1024.0 / 1024.0, 2) as data_mb,
                ROUND(SUM(INDEX_LENGTH) / 1024.0 / 1024.0, 2) as index_mb,
                COUNT(*) as table_count,
                SUM(TABLE_ROWS) as total_rows
             FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = '{}' AND TABLE_TYPE = 'BASE TABLE'",
            database
        ))
        .await?;

    match result {
        Some((total, data, index, tables, rows)) => Ok(DatabaseSize {
            total_size_mb: total.unwrap_or(0.0),
            data_size_mb: data.unwrap_or(0.0),
            index_size_mb: index.unwrap_or(0.0),
            table_count: tables.unwrap_or(0),
            total_rows: rows.unwrap_or(0),
        }),
        None => Ok(DatabaseSize {
            total_size_mb: 0.0,
            data_size_mb: 0.0,
            index_size_mb: 0.0,
            table_count: 0,
            total_rows: 0,
        }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSizeEntry {
    pub table: String,
    pub rows: u64,
    pub data_size_mb: f64,
    pub index_size_mb: f64,
    pub total_size_mb: f64,
}

pub async fn get_table_size_ranking(database: &str) -> Result<Vec<TableSizeEntry>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let entries = conn
        .query_map(
            format!(
                "SELECT TABLE_NAME, TABLE_ROWS,
                        ROUND(DATA_LENGTH / 1024.0 / 1024.0, 2),
                        ROUND(INDEX_LENGTH / 1024.0 / 1024.0, 2),
                        ROUND((DATA_LENGTH + INDEX_LENGTH) / 1024.0 / 1024.0, 2)
                 FROM information_schema.TABLES
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_TYPE = 'BASE TABLE'
                 ORDER BY (DATA_LENGTH + INDEX_LENGTH) DESC",
                database
            ),
            |row: Row| TableSizeEntry {
                table: row.get(0).unwrap_or_default(),
                rows: row.get(1).unwrap_or(0),
                data_size_mb: row.get(2).unwrap_or(0.0),
                index_size_mb: row.get(3).unwrap_or(0.0),
                total_size_mb: row.get(4).unwrap_or(0.0),
            },
        )
        .await?;

    Ok(entries)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub id: u64,
    pub user: String,
    pub host: String,
    pub database: Option<String>,
    pub command: String,
    pub time: u64,
    pub state: Option<String>,
    pub info: Option<String>,
}

pub async fn get_active_connections() -> Result<Vec<ConnectionInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let connections = conn
        .query_map(
            "SELECT ID, USER, HOST, DB, COMMAND, TIME, STATE, INFO
             FROM information_schema.PROCESSLIST
             ORDER BY TIME DESC",
            |row: Row| ConnectionInfo {
                id: row.get(0).unwrap_or(0),
                user: row.get(1).unwrap_or_default(),
                host: row.get(2).unwrap_or_default(),
                database: row.get(3),
                command: row.get(4).unwrap_or_default(),
                time: row.get(5).unwrap_or(0),
                state: row.get(6),
                info: row.get(7),
            },
        )
        .await?;

    Ok(connections)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowQueryInfo {
    pub query: String,
    pub exec_count: u64,
    pub avg_timer_ms: f64,
    pub total_timer_ms: f64,
    pub rows_examined: u64,
    pub rows_sent: u64,
}

pub async fn get_slow_queries(limit: usize) -> Result<Vec<SlowQueryInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let has_perf_schema: u64 = conn
        .query_first(
            "SELECT COUNT(*) FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = 'performance_schema'
             AND TABLE_NAME = 'events_statements_summary_by_digest'"
        )
        .await?
        .unwrap_or(0);

    if has_perf_schema == 0 {
        let slow_on: u64 = conn
            .query_first("SELECT @@slow_query_log")
            .await?
            .unwrap_or(0);

        if slow_on == 0 {
            return Ok(Vec::new());
        }

        let file: String = conn
            .query_first("SELECT @@slow_query_log_file")
            .await?
            .unwrap_or_else(|| "not configured".to_string());

        return Err(AppError::QueryExecution(format!(
            "performance_schema not available. Slow query log is {} (file: {})",
            if slow_on != 0 { "ON" } else { "OFF" },
            file
        )));
    }

    let queries = conn
        .query_map(
            format!(
                "SELECT
                    DIGEST_TEXT as query,
                    COUNT_STAR as exec_count,
                    ROUND(AVG_TIMER_WAIT / 1000000000, 2) as avg_ms,
                    ROUND(SUM_TIMER_WAIT / 1000000000, 2) as total_ms,
                    SUM_ROWS_EXAMINED as rows_examined,
                    SUM_ROWS_SENT as rows_sent
                 FROM performance_schema.events_statements_summary_by_digest
                 WHERE DIGEST_TEXT IS NOT NULL
                 ORDER BY SUM_TIMER_WAIT DESC
                 LIMIT {}",
                limit
            ),
            |row: Row| SlowQueryInfo {
                query: row.get(0).unwrap_or_default(),
                exec_count: row.get(1).unwrap_or(0),
                avg_timer_ms: row.get(2).unwrap_or(0.0),
                total_timer_ms: row.get(3).unwrap_or(0.0),
                rows_examined: row.get(4).unwrap_or(0),
                rows_sent: row.get(5).unwrap_or(0),
            },
        )
        .await?;

    Ok(queries)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub method: String,
    pub expression: Option<String>,
    pub rows: u64,
    pub data_length: u64,
    pub description: Option<String>,
}

pub async fn get_table_partitions(database: &str, table: &str) -> Result<Vec<PartitionInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let partitions = conn
        .query_map(
            format!(
                "SELECT PARTITION_NAME, PARTITION_METHOD, PARTITION_EXPRESSION,
                        TABLE_ROWS, DATA_LENGTH, PARTITION_DESCRIPTION
                 FROM information_schema.PARTITIONS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'
                 ORDER BY PARTITION_ORDINAL_POSITION",
                database, table
            ),
            |row: Row| PartitionInfo {
                name: row.get(0).unwrap_or_else(|| "full table".to_string()),
                method: row.get(1).unwrap_or_else(|| "NONE".to_string()),
                expression: row.get(2),
                rows: row.get(3).unwrap_or(0),
                data_length: row.get(4).unwrap_or(0),
                description: row.get(5),
            },
        )
        .await?;

    Ok(partitions)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnCharsetInfo {
    pub column: String,
    pub data_type: String,
    pub character_set: Option<String>,
    pub collation: Option<String>,
}

pub async fn get_column_charset_info(database: &str, table: &str) -> Result<Vec<ColumnCharsetInfo>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let infos = conn
        .query_map(
            format!(
                "SELECT COLUMN_NAME, DATA_TYPE, CHARACTER_SET_NAME, COLLATION_NAME
                 FROM information_schema.COLUMNS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'
                 ORDER BY ORDINAL_POSITION",
                database, table
            ),
            |row: Row| ColumnCharsetInfo {
                column: row.get(0).unwrap_or_default(),
                data_type: row.get(1).unwrap_or_default(),
                character_set: row.get(2),
                collation: row.get(3),
            },
        )
        .await?;

    Ok(infos)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTypeSummary {
    pub data_type: String,
    pub column_count: u64,
    pub tables: Vec<String>,
}

pub async fn get_data_type_summary(database: &str) -> Result<Vec<DataTypeSummary>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let summaries = conn
        .query_map(
            format!(
                "SELECT DATA_TYPE, COUNT(*) as cnt, GROUP_CONCAT(DISTINCT TABLE_NAME ORDER BY TABLE_NAME SEPARATOR ', ') as tbls
                 FROM information_schema.COLUMNS
                 WHERE TABLE_SCHEMA = '{}'
                 GROUP BY DATA_TYPE
                 ORDER BY cnt DESC",
                database
            ),
            |row: Row| DataTypeSummary {
                data_type: row.get(0).unwrap_or_default(),
                column_count: row.get(1).unwrap_or(0),
                tables: row.get::<String, _>(2)
                    .unwrap_or_default()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            },
        )
        .await?;

    Ok(summaries)
}

pub async fn find_orphan_records(
    database: &str,
    table: &str,
    column: &str,
    ref_database: &str,
    ref_table: &str,
    ref_column: &str,
) -> Result<u64, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let count: u64 = conn
        .query_first(format!(
            "SELECT COUNT(*) FROM `{}`.`{}` t1
             LEFT JOIN `{}`.`{}` t2 ON t1.`{}` = t2.`{}`
             WHERE t1.`{}` IS NOT NULL AND t2.`{}` IS NULL",
            database, table, ref_database, ref_table, column, ref_column, column, ref_column
        ))
        .await?
        .unwrap_or(0);

    Ok(count)
}

pub fn get_similar_tables(schemas: &[Schema]) -> Vec<(String, String, f64)> {
    let mut all_tables: Vec<(&str, &str)> = Vec::new();
    for schema in schemas {
        for table in &schema.tables {
            all_tables.push((&schema.database, &table.name));
        }
    }

    let mut similar = Vec::new();
    for i in 0..all_tables.len() {
        for j in (i + 1)..all_tables.len() {
            let (db1, t1) = all_tables[i];
            let (db2, t2) = all_tables[j];

            let t1_lower = t1.to_lowercase();
            let t2_lower = t2.to_lowercase();

            let score = if t1_lower == t2_lower {
                1.0
            } else if t1_lower.starts_with(&t2_lower) || t2_lower.starts_with(&t1_lower) {
                0.8
            } else if t1_lower.contains(&t2_lower) || t2_lower.contains(&t1_lower) {
                0.6
            } else if levenshtein_similarity(&t1_lower, &t2_lower) > 0.7 {
                levenshtein_similarity(&t1_lower, &t2_lower)
            } else {
                continue;
            };

            similar.push((
                format!("{}.{}", db1, t1),
                format!("{}.{}", db2, t2),
                score,
            ));
        }
    }

    similar.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    similar
}

fn levenshtein_similarity(a: &str, b: &str) -> f64 {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 && b_len == 0 {
        return 1.0;
    }
    if a_len == 0 || b_len == 0 {
        return 0.0;
    }

    let max_len = a_len.max(b_len) as f64;
    let dist = levenshtein_distance(a, b);
    1.0 - (dist as f64 / max_len)
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 { return b_len; }
    if b_len == 0 { return a_len; }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    for i in 0..=a_len { matrix[i][0] = i; }
    for j in 0..=b_len { matrix[0][j] = j; }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSuggestion {
    pub table: String,
    pub column: String,
    pub reason: String,
    pub priority: String,
}

pub async fn suggest_indexes(database: &str, table: &str) -> Result<Vec<IndexSuggestion>, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let mut suggestions = Vec::new();

    let columns: Vec<(String, String, String)> = conn
        .query_map(
            format!(
                "SELECT COLUMN_NAME, COLUMN_KEY, DATA_TYPE FROM information_schema.COLUMNS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'
                 ORDER BY ORDINAL_POSITION",
                database, table
            ),
            |row: Row| (row.get(0).unwrap_or_default(), row.get(1).unwrap_or_default(), row.get(2).unwrap_or_default()),
        )
        .await?;

    let existing_indexes: Vec<String> = conn
        .query_map(
            format!(
                "SELECT DISTINCT INDEX_NAME FROM information_schema.STATISTICS
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'",
                database, table
            ),
            |row: Row| row.get(0).unwrap_or_default(),
        )
        .await?;

    for (col, key, _dtype) in &columns {
        if !key.is_empty() {
            continue;
        }

        let col_lower = col.to_lowercase();
        let existing_lower: Vec<String> = existing_indexes.iter().map(|i| i.to_lowercase()).collect();

        if (col_lower.ends_with("_id") || col_lower.ends_with("id"))
            && !existing_lower.iter().any(|i| i.contains(&col_lower))
        {
            suggestions.push(IndexSuggestion {
                table: format!("{}.{}", database, table),
                column: col.clone(),
                reason: "Foreign key column — index recommended for JOIN performance".to_string(),
                priority: "HIGH".to_string(),
            });
        }

        if (col_lower.contains("status") || col_lower.contains("type") || col_lower.contains("category"))
            && !existing_lower.iter().any(|i| i.contains(&col_lower))
        {
            suggestions.push(IndexSuggestion {
                table: format!("{}.{}", database, table),
                column: col.clone(),
                reason: "Low-cardinality filtering column — index for WHERE clauses".to_string(),
                priority: "MEDIUM".to_string(),
            });
        }

        if (col_lower.contains("date") || col_lower.contains("time") || col_lower.contains("created") || col_lower.contains("updated"))
            && !existing_lower.iter().any(|i| i.contains(&col_lower))
        {
            suggestions.push(IndexSuggestion {
                table: format!("{}.{}", database, table),
                column: col.clone(),
                reason: "Date/time column — index for range queries and ORDER BY".to_string(),
                priority: "HIGH".to_string(),
            });
        }

        if (col_lower == "email" || col_lower == "username" || col_lower == "phone" || col_lower == "slug")
            && !existing_lower.iter().any(|i| i.contains(&col_lower))
        {
            suggestions.push(IndexSuggestion {
                table: format!("{}.{}", database, table),
                column: col.clone(),
                reason: "Lookup column — UNIQUE index recommended".to_string(),
                priority: "HIGH".to_string(),
            });
        }

        if col_lower.contains("name") && !existing_lower.iter().any(|i| i.contains(&col_lower)) {
            suggestions.push(IndexSuggestion {
                table: format!("{}.{}", database, table),
                column: col.clone(),
                reason: "Name column — index for search and ORDER BY".to_string(),
                priority: "LOW".to_string(),
            });
        }
    }

    Ok(suggestions)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOptions {
    pub engine: String,
    pub row_format: String,
    pub create_options: String,
    pub table_collation: String,
    pub auto_increment: Option<u64>,
    pub pack_keys: Option<String>,
    pub checksum: Option<u64>,
    pub delay_key_write: Option<String>,
}

pub async fn get_create_options(database: &str, table: &str) -> Result<CreateOptions, AppError> {
    let pool = get_pool().await?;
    let mut conn = pool.get_conn().await?;

    let result: Option<(
        Option<String>, Option<String>, Option<String>, Option<String>,
        Option<u64>, Option<String>, Option<u64>, Option<String>,
    )> = conn
        .query_first(format!(
            "SELECT ENGINE, ROW_FORMAT, CREATE_OPTIONS, TABLE_COLLATION,
                    AUTO_INCREMENT, PACK_KEYS, CHECKSUM, DELAY_KEY_WRITE
             FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'",
            database, table
        ))
        .await?;

    match result {
        Some((engine, row_format, create_opts, collation, auto_inc, pack_keys, checksum, delay_key_write)) => Ok(CreateOptions {
            engine: engine.unwrap_or_default(),
            row_format: row_format.unwrap_or_default(),
            create_options: create_opts.unwrap_or_default(),
            table_collation: collation.unwrap_or_default(),
            auto_increment: auto_inc,
            pack_keys,
            checksum,
            delay_key_write,
        }),
        None => Err(AppError::QueryExecution(format!("Table {}.{} not found", database, table))),
    }
}

