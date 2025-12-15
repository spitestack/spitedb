//! # Projection Infrastructure
//!
//! This module manages projection tables and checkpoints. Projections are
//! read models built from the event stream, stored in a separate SQLite file.
//!
//! ## Architecture
//!
//! ```text
//! ProjectionManager
//!     │
//!     ├── projections.db (separate SQLite file)
//!     │   ├── user_stats (projection table)
//!     │   ├── order_totals (projection table)
//!     │   └── _projection_checkpoints (metadata)
//!     │
//!     └── SpiteDB reference (for reading events)
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use spitedb::SpiteDB;

use crate::{BatchResultNapi, ColumnDefNapi, ProjectionOpNapi};

// =============================================================================
// Schema Types
// =============================================================================

/// Column type for projection schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Text,
    Integer,
    Real,
    Blob,
    Boolean,
}

impl ColumnType {
    /// Converts to SQLite type string.
    pub fn to_sql(&self) -> &'static str {
        match self {
            ColumnType::Text => "TEXT",
            ColumnType::Integer => "INTEGER",
            ColumnType::Real => "REAL",
            ColumnType::Blob => "BLOB",
            ColumnType::Boolean => "INTEGER", // SQLite stores booleans as integers
        }
    }

    /// Parses from string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "text" | "string" => Some(ColumnType::Text),
            "integer" | "int" | "bigint" => Some(ColumnType::Integer),
            "real" | "float" | "double" | "decimal" => Some(ColumnType::Real),
            "blob" | "bytes" => Some(ColumnType::Blob),
            "boolean" | "bool" => Some(ColumnType::Boolean),
            _ => None,
        }
    }
}

/// Definition of a column in a projection table.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: ColumnType,
    pub primary_key: bool,
    pub nullable: bool,
    pub default_value: Option<JsonValue>,
}

impl From<ColumnDefNapi> for ColumnDef {
    fn from(napi: ColumnDefNapi) -> Self {
        Self {
            name: napi.name,
            col_type: ColumnType::from_str(&napi.col_type).unwrap_or(ColumnType::Text),
            primary_key: napi.primary_key,
            nullable: napi.nullable,
            default_value: napi
                .default_value
                .and_then(|s| serde_json::from_str(&s).ok()),
        }
    }
}

/// Schema definition for a projection table.
#[derive(Debug, Clone)]
pub struct ProjectionSchema {
    pub table_name: String,
    pub columns: Vec<ColumnDef>,
}

impl ProjectionSchema {
    /// Gets the primary key column(s).
    pub fn primary_key_columns(&self) -> Vec<&ColumnDef> {
        self.columns.iter().filter(|c| c.primary_key).collect()
    }

    /// Gets the primary key column name (assumes single PK).
    pub fn primary_key_name(&self) -> Option<&str> {
        self.columns
            .iter()
            .find(|c| c.primary_key)
            .map(|c| c.name.as_str())
    }
}

// =============================================================================
// Operation Types
// =============================================================================

/// Operation type for projection updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpType {
    Upsert,
    Delete,
}

/// A single projection operation.
#[derive(Debug, Clone)]
pub struct ProjectionOp {
    pub op_type: OpType,
    pub key: String,
    pub value: Option<JsonValue>,
}

impl From<ProjectionOpNapi> for ProjectionOp {
    fn from(napi: ProjectionOpNapi) -> Self {
        Self {
            op_type: match napi.op_type.as_str() {
                "delete" => OpType::Delete,
                _ => OpType::Upsert, // Default to upsert
            },
            key: napi.key,
            value: napi.value.and_then(|s| serde_json::from_str(&s).ok()),
        }
    }
}

/// Result of processing a batch - operations to apply.
#[derive(Debug)]
pub struct BatchResult {
    pub projection_name: String,
    pub operations: Vec<ProjectionOp>,
    pub last_global_pos: i64,
}

impl From<BatchResultNapi> for BatchResult {
    fn from(napi: BatchResultNapi) -> Self {
        Self {
            projection_name: napi.projection_name,
            operations: napi.operations.into_iter().map(ProjectionOp::from).collect(),
            last_global_pos: napi.last_global_pos,
        }
    }
}

// =============================================================================
// Projection Metadata
// =============================================================================

/// Metadata for a registered projection.
struct ProjectionMetadata {
    name: String,
    schema: ProjectionSchema,
}

// =============================================================================
// Projection Manager
// =============================================================================

/// Manages projection tables and checkpoints.
///
/// Each projection has:
/// - A SQLite table for storing the read model
/// - A checkpoint tracking the last processed event position
pub struct ProjectionManager {
    /// SQLite connection for projection tables (separate from event store)
    conn: Connection,

    /// Reference to the event store (for reading events)
    #[allow(dead_code)]
    event_store: Arc<SpiteDB>,

    /// Registered projections
    projections: HashMap<String, ProjectionMetadata>,
}

impl ProjectionManager {
    /// Creates a new projection manager.
    ///
    /// Opens or creates the SQLite file for projection tables.
    pub fn new(path: &str, event_store: Arc<SpiteDB>) -> Result<Self, ProjectionError> {
        let conn = Connection::open(path)?;

        // Configure SQLite for performance
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;

        let mut manager = Self {
            conn,
            event_store,
            projections: HashMap::new(),
        };

        manager.ensure_checkpoint_table()?;

        Ok(manager)
    }

    /// Ensures the checkpoint table exists.
    fn ensure_checkpoint_table(&mut self) -> Result<(), ProjectionError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS _projection_checkpoints (
                projection_name TEXT PRIMARY KEY,
                last_global_pos INTEGER NOT NULL,
                last_processed_ms INTEGER NOT NULL,
                event_count INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;
        Ok(())
    }

    /// Registers a new projection.
    ///
    /// Creates the projection table if it doesn't exist.
    pub fn register_projection(
        &mut self,
        name: &str,
        schema: ProjectionSchema,
    ) -> Result<(), ProjectionError> {
        // Create the table
        self.create_projection_table(&schema)?;

        // Store metadata
        self.projections.insert(
            name.to_string(),
            ProjectionMetadata {
                name: name.to_string(),
                schema,
            },
        );

        Ok(())
    }

    /// Creates a projection table from its schema.
    fn create_projection_table(&self, schema: &ProjectionSchema) -> Result<(), ProjectionError> {
        let mut sql = format!("CREATE TABLE IF NOT EXISTS {} (\n", schema.table_name);

        let mut primary_keys = Vec::new();

        for (i, col) in schema.columns.iter().enumerate() {
            if i > 0 {
                sql.push_str(",\n");
            }

            sql.push_str(&format!("    {} {}", col.name, col.col_type.to_sql()));

            if !col.nullable {
                sql.push_str(" NOT NULL");
            }

            if let Some(ref default) = col.default_value {
                sql.push_str(&format!(" DEFAULT {}", json_to_sql_literal(default)));
            }

            if col.primary_key {
                primary_keys.push(col.name.clone());
            }
        }

        if !primary_keys.is_empty() {
            sql.push_str(&format!(",\n    PRIMARY KEY ({})", primary_keys.join(", ")));
        }

        sql.push_str("\n)");

        self.conn.execute(&sql, [])?;

        Ok(())
    }

    /// Reads a row by primary key.
    ///
    /// Returns the row as a JSON object, or None if not found.
    pub fn read_row(
        &self,
        projection_name: &str,
        key: &str,
    ) -> Result<Option<JsonValue>, ProjectionError> {
        let metadata = self
            .projections
            .get(projection_name)
            .ok_or_else(|| ProjectionError::NotFound(projection_name.to_string()))?;

        let pk_name = metadata
            .schema
            .primary_key_name()
            .ok_or_else(|| ProjectionError::NoPrimaryKey(projection_name.to_string()))?;

        let sql = format!(
            "SELECT * FROM {} WHERE {} = ?",
            metadata.schema.table_name, pk_name
        );

        let mut stmt = self.conn.prepare(&sql)?;

        let result = stmt
            .query_row([key], |row| {
                let mut obj = serde_json::Map::new();

                for (i, col) in metadata.schema.columns.iter().enumerate() {
                    let value = row_value_to_json(row, i, col.col_type)?;
                    obj.insert(col.name.clone(), value);
                }

                Ok(JsonValue::Object(obj))
            })
            .optional()?;

        Ok(result)
    }

    /// Applies a batch of operations atomically.
    ///
    /// All operations and the checkpoint update happen in a single transaction.
    pub fn apply_batch(&mut self, batch: BatchResult) -> Result<(), ProjectionError> {
        let metadata = self
            .projections
            .get(&batch.projection_name)
            .ok_or_else(|| ProjectionError::NotFound(batch.projection_name.clone()))?
            .clone();

        // Start transaction
        self.conn.execute("BEGIN IMMEDIATE", [])?;

        let result = self.apply_batch_inner(&metadata.schema, &batch);

        match result {
            Ok(()) => {
                self.conn.execute("COMMIT", [])?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    fn apply_batch_inner(
        &self,
        schema: &ProjectionSchema,
        batch: &BatchResult,
    ) -> Result<(), ProjectionError> {
        let pk_name = schema
            .primary_key_name()
            .ok_or_else(|| ProjectionError::NoPrimaryKey(batch.projection_name.clone()))?;

        for op in &batch.operations {
            match op.op_type {
                OpType::Upsert => {
                    self.execute_upsert(schema, pk_name, &op.key, op.value.as_ref())?;
                }
                OpType::Delete => {
                    self.execute_delete(schema, pk_name, &op.key)?;
                }
            }
        }

        // Update checkpoint
        self.conn.execute(
            "INSERT INTO _projection_checkpoints (projection_name, last_global_pos, last_processed_ms, event_count)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(projection_name) DO UPDATE SET
                 last_global_pos = excluded.last_global_pos,
                 last_processed_ms = excluded.last_processed_ms,
                 event_count = event_count + excluded.event_count",
            params![
                batch.projection_name,
                batch.last_global_pos,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64,
                batch.operations.len() as i64,
            ],
        )?;

        Ok(())
    }

    fn execute_upsert(
        &self,
        schema: &ProjectionSchema,
        pk_name: &str,
        key: &str,
        value: Option<&JsonValue>,
    ) -> Result<(), ProjectionError> {
        let value = value.ok_or(ProjectionError::MissingValue)?;

        // Build column list and values
        let mut columns = vec![pk_name.to_string()];
        let mut placeholders = vec!["?".to_string()];
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(key.to_string())];

        if let JsonValue::Object(obj) = value {
            for col in &schema.columns {
                if col.name == pk_name {
                    continue; // Already handled
                }

                if let Some(v) = obj.get(&col.name) {
                    columns.push(col.name.clone());
                    placeholders.push("?".to_string());
                    values.push(json_value_to_sql(v));
                }
            }
        }

        let sql = format!(
            "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
            schema.table_name,
            columns.join(", "),
            placeholders.join(", ")
        );

        let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        self.conn.execute(&sql, params.as_slice())?;

        Ok(())
    }

    fn execute_delete(
        &self,
        schema: &ProjectionSchema,
        pk_name: &str,
        key: &str,
    ) -> Result<(), ProjectionError> {
        let sql = format!("DELETE FROM {} WHERE {} = ?", schema.table_name, pk_name);
        self.conn.execute(&sql, [key])?;
        Ok(())
    }

    /// Gets the current checkpoint for a projection.
    pub fn get_checkpoint(&self, projection_name: &str) -> Result<Option<i64>, ProjectionError> {
        let result: Option<i64> = self
            .conn
            .query_row(
                "SELECT last_global_pos FROM _projection_checkpoints WHERE projection_name = ?",
                [projection_name],
                |row| row.get(0),
            )
            .optional()?;

        Ok(result)
    }
}

// Clone ProjectionMetadata for use in apply_batch
impl Clone for ProjectionMetadata {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            schema: self.schema.clone(),
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Converts a JSON value to a SQL literal string.
fn json_to_sql_literal(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "NULL".to_string(),
        JsonValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        _ => "NULL".to_string(),
    }
}

/// Converts a JSON value to a boxed SQL parameter.
fn json_value_to_sql(value: &JsonValue) -> Box<dyn rusqlite::ToSql> {
    match value {
        JsonValue::Null => Box::new(Option::<String>::None),
        JsonValue::Bool(b) => Box::new(if *b { 1i64 } else { 0i64 }),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        JsonValue::String(s) => Box::new(s.clone()),
        _ => Box::new(serde_json::to_string(value).unwrap_or_default()),
    }
}

/// Converts a row value to JSON based on column type.
fn row_value_to_json(
    row: &rusqlite::Row,
    idx: usize,
    col_type: ColumnType,
) -> rusqlite::Result<JsonValue> {
    match col_type {
        ColumnType::Text => {
            let v: Option<String> = row.get(idx)?;
            Ok(v.map(JsonValue::String).unwrap_or(JsonValue::Null))
        }
        ColumnType::Integer => {
            let v: Option<i64> = row.get(idx)?;
            Ok(v.map(|n| JsonValue::Number(n.into()))
                .unwrap_or(JsonValue::Null))
        }
        ColumnType::Real => {
            let v: Option<f64> = row.get(idx)?;
            Ok(v.and_then(|n| serde_json::Number::from_f64(n).map(JsonValue::Number))
                .unwrap_or(JsonValue::Null))
        }
        ColumnType::Boolean => {
            let v: Option<i64> = row.get(idx)?;
            Ok(v.map(|n| JsonValue::Bool(n != 0))
                .unwrap_or(JsonValue::Null))
        }
        ColumnType::Blob => {
            let v: Option<Vec<u8>> = row.get(idx)?;
            // Return as base64 string for JSON compatibility
            Ok(v.map(|bytes| {
                use base64::Engine;
                JsonValue::String(base64::engine::general_purpose::STANDARD.encode(bytes))
            })
            .unwrap_or(JsonValue::Null))
        }
    }
}

// =============================================================================
// Error Type
// =============================================================================

/// Errors that can occur in projection operations.
#[derive(Debug)]
pub enum ProjectionError {
    /// SQLite error
    Sqlite(rusqlite::Error),
    /// Projection not found
    NotFound(String),
    /// Projection has no primary key defined
    NoPrimaryKey(String),
    /// Upsert operation missing value
    MissingValue,
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectionError::Sqlite(e) => write!(f, "SQLite error: {}", e),
            ProjectionError::NotFound(name) => write!(f, "Projection '{}' not found", name),
            ProjectionError::NoPrimaryKey(name) => {
                write!(f, "Projection '{}' has no primary key", name)
            }
            ProjectionError::MissingValue => write!(f, "Upsert operation missing value"),
        }
    }
}

impl std::error::Error for ProjectionError {}

impl From<rusqlite::Error> for ProjectionError {
    fn from(e: rusqlite::Error) -> Self {
        ProjectionError::Sqlite(e)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> ProjectionManager {
        let event_store = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { SpiteDB::open_in_memory().await.unwrap() });

        ProjectionManager::new(":memory:", Arc::new(event_store)).unwrap()
    }

    #[test]
    fn test_register_projection() {
        let mut manager = create_test_manager();

        let schema = ProjectionSchema {
            table_name: "user_stats".to_string(),
            columns: vec![
                ColumnDef {
                    name: "user_id".to_string(),
                    col_type: ColumnType::Text,
                    primary_key: true,
                    nullable: false,
                    default_value: None,
                },
                ColumnDef {
                    name: "login_count".to_string(),
                    col_type: ColumnType::Integer,
                    primary_key: false,
                    nullable: false,
                    default_value: Some(JsonValue::Number(0.into())),
                },
            ],
        };

        manager.register_projection("user_stats", schema).unwrap();

        // Verify table was created
        let count: i64 = manager
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='user_stats'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn test_upsert_and_read() {
        let mut manager = create_test_manager();

        let schema = ProjectionSchema {
            table_name: "test_table".to_string(),
            columns: vec![
                ColumnDef {
                    name: "id".to_string(),
                    col_type: ColumnType::Text,
                    primary_key: true,
                    nullable: false,
                    default_value: None,
                },
                ColumnDef {
                    name: "count".to_string(),
                    col_type: ColumnType::Integer,
                    primary_key: false,
                    nullable: false,
                    default_value: None,
                },
            ],
        };

        manager.register_projection("test", schema).unwrap();

        // Apply upsert
        let batch = BatchResult {
            projection_name: "test".to_string(),
            operations: vec![ProjectionOp {
                op_type: OpType::Upsert,
                key: "key1".to_string(),
                value: Some(serde_json::json!({"count": 42})),
            }],
            last_global_pos: 1,
        };

        manager.apply_batch(batch).unwrap();

        // Read back
        let row = manager.read_row("test", "key1").unwrap();
        assert!(row.is_some());

        let obj = row.unwrap();
        assert_eq!(obj["id"], "key1");
        assert_eq!(obj["count"], 42);
    }

    #[test]
    fn test_checkpoint() {
        let mut manager = create_test_manager();

        let schema = ProjectionSchema {
            table_name: "test_ckpt".to_string(),
            columns: vec![ColumnDef {
                name: "id".to_string(),
                col_type: ColumnType::Text,
                primary_key: true,
                nullable: false,
                default_value: None,
            }],
        };

        manager.register_projection("test_ckpt", schema).unwrap();

        // Initially no checkpoint
        let ckpt = manager.get_checkpoint("test_ckpt").unwrap();
        assert!(ckpt.is_none());

        // Apply batch updates checkpoint
        let batch = BatchResult {
            projection_name: "test_ckpt".to_string(),
            operations: vec![],
            last_global_pos: 100,
        };

        manager.apply_batch(batch).unwrap();

        let ckpt = manager.get_checkpoint("test_ckpt").unwrap();
        assert_eq!(ckpt, Some(100));
    }
}
