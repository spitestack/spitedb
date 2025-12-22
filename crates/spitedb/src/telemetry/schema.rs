//! # Telemetry SQLite Schema
//!
//! Defines the telemetry table and indexes for each shard database.

use rusqlite::{Connection, OptionalExtension};

use crate::{Error, Result};

const TELEMETRY_SCHEMA_VERSION: i32 = 1;

const CREATE_TELEMETRY: &str = r#"
CREATE TABLE IF NOT EXISTS telemetry (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    ts_ms            INTEGER NOT NULL,
    kind             INTEGER NOT NULL,
    tenant_hash      INTEGER NOT NULL,

    event_global_pos INTEGER,
    stream_hash      INTEGER,
    stream_rev       INTEGER,
    command_id       TEXT,

    trace_id         TEXT,
    span_id          TEXT,
    parent_span_id   TEXT,

    name             TEXT,
    service          TEXT,

    severity         INTEGER,
    message          TEXT,

    metric_name      TEXT,
    metric_value     REAL,
    metric_kind      INTEGER,
    metric_unit      TEXT,

    span_start_ms    INTEGER,
    span_end_ms      INTEGER,
    span_duration_ms INTEGER,
    span_status      INTEGER,

    attrs_json       TEXT
)
"#;

const CREATE_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS telemetry_tenant_ts
ON telemetry(tenant_hash, ts_ms);

CREATE INDEX IF NOT EXISTS telemetry_kind_ts
ON telemetry(kind, ts_ms);

CREATE INDEX IF NOT EXISTS telemetry_event_pos
ON telemetry(event_global_pos);

CREATE INDEX IF NOT EXISTS telemetry_stream_rev
ON telemetry(stream_hash, stream_rev);

CREATE INDEX IF NOT EXISTS telemetry_command_id
ON telemetry(command_id);

CREATE INDEX IF NOT EXISTS telemetry_trace
ON telemetry(trace_id, span_id);

CREATE INDEX IF NOT EXISTS telemetry_metric_name_ts
ON telemetry(metric_name, ts_ms);
"#;

const CREATE_METADATA: &str = r#"
CREATE TABLE IF NOT EXISTS telemetry_metadata (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
)
"#;

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA journal_mode = WAL")?;
    conn.execute_batch("PRAGMA synchronous = NORMAL")?;
    conn.execute_batch("PRAGMA foreign_keys = ON")?;
    conn.execute_batch("PRAGMA temp_store = MEMORY")?;
    conn.execute_batch(CREATE_TELEMETRY)?;
    conn.execute_batch(CREATE_INDEXES)?;
    conn.execute_batch(CREATE_METADATA)?;

    ensure_schema_version(conn)
}

fn ensure_schema_version(conn: &Connection) -> Result<()> {
    let version: Option<String> = conn
        .query_row(
            "SELECT value FROM telemetry_metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    match version {
        Some(value) => {
            let parsed: i32 = value.parse().map_err(|_| {
                Error::Schema("telemetry schema version is invalid".to_string())
            })?;
            if parsed != TELEMETRY_SCHEMA_VERSION {
                return Err(Error::Schema(format!(
                    "telemetry schema version mismatch: database has version {parsed}, but this SpiteDB version requires {TELEMETRY_SCHEMA_VERSION}"
                )));
            }
        }
        None => {
            conn.execute(
                "INSERT INTO telemetry_metadata (key, value) VALUES ('schema_version', ?)",
                [TELEMETRY_SCHEMA_VERSION.to_string()],
            )?;
        }
    }

    Ok(())
}
