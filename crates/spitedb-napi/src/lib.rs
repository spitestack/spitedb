//! # SpiteDB NAPI Bindings
//!
//! This crate provides Node.js/Bun bindings for SpiteDB, enabling JavaScript
//! applications to use the event store with native performance.

use std::sync::{Arc, Mutex as StdMutex};

use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::sync::Mutex;

use serde::Deserialize;
use spitedb::{
    AppendCommand, AppendResult, CommandId, Event, EventData, GlobalPos, MetricsSnapshot, SpiteDB,
    SqlStatement, SqlValue, StreamHash, StreamId, StreamRev, Tenant, TenantHash,
    TelemetryConfig, TelemetryCursor, TelemetryDB, TelemetryKind, TelemetryOrder, TelemetryQuery,
    TelemetryRecord, TimeSlice, MetricKind, SpanStatus,
};

mod projection;

// JSON request structs for appendStreamJson
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppendStreamRequest {
    stream_id: String,
    command_id: String,
    expected_rev: i64,
    events: Vec<serde_json::Value>,
    tenant: String,
}

// JSON request structs for appendBatchJson
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchCommand {
    stream_id: String,
    command_id: String,
    expected_rev: i64,
    events: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchAppendRequest {
    commands: Vec<BatchCommand>,
    tenant: String,
}

pub use projection::{
    BatchResult, ColumnDef, ColumnType, OpType, ProjectionError, ProjectionOp,
    ProjectionRegistry, ProjectionSchema,
};

// =============================================================================
// SpiteDB NAPI Wrapper
// =============================================================================

/// NAPI wrapper for SpiteDB.
#[napi]
pub struct SpiteDBNapi {
    inner: Arc<SpiteDB>,
    projection_registry: Arc<Mutex<Option<ProjectionRegistry>>>,
}

#[napi]
impl SpiteDBNapi {
    /// Opens a SpiteDB database at the given path.
    #[napi(factory)]
    pub async fn open(path: String) -> Result<Self> {
        let db = SpiteDB::open(&path)
            .await
            .map_err(|e| Error::from_reason(format!("Failed to open database: {}", e)))?;

        Ok(Self {
            inner: Arc::new(db),
            projection_registry: Arc::new(Mutex::new(None)),
        })
    }

    /// Simple test method to verify NAPI exports work
    #[napi]
    pub fn test_echo(&self, msg: String) -> String {
        format!("Echo: {}", msg)
    }

    /// Appends events to a stream.
    ///
    /// @param streamId - The stream to append to
    /// @param commandId - Unique command ID for idempotency
    /// @param expectedRev - Expected revision: -1 for "any", 0 for "stream must not exist", >0 for exact revision
    /// @param events - Array of event data buffers
    /// @param tenant - Tenant ID (use DEFAULT_TENANT for single-tenant apps)
    #[napi]
    pub async fn append(
        &self,
        stream_id: String,
        command_id: String,
        expected_rev: i64,
        events: Vec<Buffer>,
        tenant: String,
    ) -> Result<AppendResultNapi> {
        // Validate inputs to prevent panics
        if events.is_empty() {
            return Err(Error::from_reason("events array cannot be empty"));
        }

        // Convert expected_rev:
        // -1 = any revision is ok
        // 0 = stream must not exist (StreamRev::NONE)
        // >0 = exact revision
        let expected = if expected_rev < 0 {
            // For "any", we use the ANY sentinel
            StreamRev::ANY
        } else {
            StreamRev::from_raw(expected_rev as u64)
        };

        let event_data: Vec<EventData> = events
            .into_iter()
            .map(|buf| EventData::new(buf.to_vec()))
            .collect();

        let tenant_obj = Tenant::new(tenant);

        let command = AppendCommand::new_with_tenant(
            CommandId::new(command_id),
            StreamId::new(stream_id),
            tenant_obj,
            expected,
            event_data,
        );

        let result = self
            .inner
            .append(command)
            .await
            .map_err(|e| Error::from_reason(format!("Append failed: {}", e)))?;

        Ok(AppendResultNapi::from(result))
    }

    /// Executes an atomic transaction of appends with optional SQL statements.
    ///
    /// All commands must succeed; any conflict or duplicate aborts the transaction.
    #[napi]
    pub async fn atomic_transaction(
        &self,
        commands: Vec<AppendCommandNapi>,
        sql_ops: Vec<SqlStatementNapi>,
    ) -> Result<Vec<AppendResultNapi>> {
        if commands.is_empty() && sql_ops.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = self.inner.begin_atomic_transaction();

        for cmd in commands {
            let append = build_append_command(cmd)?;
            tx.append(append);
        }

        for stmt in sql_ops {
            let params = stmt
                .params
                .into_iter()
                .map(parse_sql_param)
                .collect::<Result<Vec<_>>>()?;
            tx.push_sql(SqlStatement::new(stmt.sql, params));
        }

        let results = tx
            .submit()
            .await
            .map_err(|e| Error::from_reason(format!("Atomic transaction failed: {}", e)))?;

        Ok(results.into_iter().map(AppendResultNapi::from).collect())
    }

    /// Appends events to multiple streams atomically via batch fsync.
    ///
    /// All commands must succeed; any conflict or duplicate aborts the entire batch.
    /// This is faster than atomicTransaction for multi-stream appends as it uses
    /// batch fsync rather than immediate SQLite transactions.
    ///
    /// @param commands - Array of append commands, one per stream
    /// @param tenant - Tenant ID (shared by all commands)
    #[napi]
    pub async fn append_batch(
        &self,
        commands: Vec<BatchAppendCommandNapi>,
        tenant: String,
    ) -> Result<Vec<AppendResultNapi>> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }

        let tenant_obj = Tenant::new(tenant);

        let append_commands: Vec<AppendCommand> = commands
            .into_iter()
            .filter(|cmd| !cmd.events.is_empty())
            .map(|cmd| {
                let expected = if cmd.expected_rev < 0 {
                    StreamRev::ANY
                } else {
                    StreamRev::from_raw(cmd.expected_rev as u64)
                };

                let event_data: Vec<EventData> = cmd
                    .events
                    .into_iter()
                    .map(|buf| EventData::new(buf.to_vec()))
                    .collect();

                AppendCommand::new_with_tenant(
                    CommandId::new(cmd.command_id),
                    StreamId::new(cmd.stream_id),
                    tenant_obj.clone(),
                    expected,
                    event_data,
                )
            })
            .collect();

        if append_commands.is_empty() {
            return Ok(Vec::new());
        }

        let results = self.inner
            .batch_append(append_commands)
            .await
            .map_err(|e| Error::from_reason(format!("Batch append failed: {}", e)))?;

        Ok(results.into_iter().map(AppendResultNapi::from).collect())
    }

    /// Appends events to a stream using a single JSON payload.
    ///
    /// This is optimized for Bun/Node.js - passing a single JSON string is faster
    /// than passing arrays of Buffers through NAPI due to reduced marshalling overhead.
    ///
    /// @param payload - JSON string containing: { streamId, commandId, expectedRev, events, tenant }
    #[napi]
    pub async fn append_stream_json(&self, payload: String) -> Result<AppendResultNapi> {
        let req: AppendStreamRequest = serde_json::from_str(&payload)
            .map_err(|e| Error::from_reason(format!("Invalid JSON: {}", e)))?;

        if req.events.is_empty() {
            return Err(Error::from_reason("events array cannot be empty"));
        }

        let expected = if req.expected_rev < 0 {
            StreamRev::ANY
        } else {
            StreamRev::from_raw(req.expected_rev as u64)
        };

        // Convert JSON values to event buffers
        let event_data: Vec<EventData> = req
            .events
            .into_iter()
            .map(|v| EventData::new(serde_json::to_vec(&v).unwrap_or_default()))
            .collect();

        let tenant_obj = Tenant::new(req.tenant);

        let command = AppendCommand::new_with_tenant(
            CommandId::new(req.command_id),
            StreamId::new(req.stream_id),
            tenant_obj,
            expected,
            event_data,
        );

        let result = self
            .inner
            .append(command)
            .await
            .map_err(|e| Error::from_reason(format!("Append failed: {}", e)))?;

        Ok(AppendResultNapi::from(result))
    }

    /// Appends events to multiple streams atomically using a single JSON payload.
    ///
    /// This is optimized for Bun/Node.js - passing a single JSON string is faster
    /// than passing arrays of objects through NAPI due to reduced marshalling overhead.
    ///
    /// @param payload - JSON string containing: { commands: [{ streamId, commandId, expectedRev, events }], tenant }
    #[napi]
    pub async fn append_batch_json(&self, payload: String) -> Result<Vec<AppendResultNapi>> {
        let req: BatchAppendRequest = serde_json::from_str(&payload)
            .map_err(|e| Error::from_reason(format!("Invalid JSON: {}", e)))?;

        if req.commands.is_empty() {
            return Ok(Vec::new());
        }

        let tenant_obj = Tenant::new(req.tenant);

        let append_commands: Vec<AppendCommand> = req
            .commands
            .into_iter()
            .filter(|cmd| !cmd.events.is_empty())
            .map(|cmd| {
                let expected = if cmd.expected_rev < 0 {
                    StreamRev::ANY
                } else {
                    StreamRev::from_raw(cmd.expected_rev as u64)
                };

                let event_data: Vec<EventData> = cmd
                    .events
                    .into_iter()
                    .map(|v| EventData::new(serde_json::to_vec(&v).unwrap_or_default()))
                    .collect();

                AppendCommand::new_with_tenant(
                    CommandId::new(cmd.command_id),
                    StreamId::new(cmd.stream_id),
                    tenant_obj.clone(),
                    expected,
                    event_data,
                )
            })
            .collect();

        if append_commands.is_empty() {
            return Ok(Vec::new());
        }

        let results = self
            .inner
            .batch_append(append_commands)
            .await
            .map_err(|e| Error::from_reason(format!("Batch append failed: {}", e)))?;

        Ok(results.into_iter().map(AppendResultNapi::from).collect())
    }

    /// Reads events from a stream.
    ///
    /// @param streamId - The stream to read from
    /// @param fromRev - Starting revision (0 for beginning)
    /// @param limit - Maximum number of events to return
    /// @param tenant - Tenant ID (use DEFAULT_TENANT for single-tenant apps)
    #[napi]
    pub async fn read_stream(
        &self,
        stream_id: String,
        from_rev: i64,
        limit: i64,
        tenant: String,
    ) -> Result<Vec<EventNapi>> {
        let tenant_obj = Tenant::new(tenant);
        let events = self
            .inner
            .read_stream_tenant(stream_id, tenant_obj, StreamRev::from_raw(from_rev as u64), limit as usize)
            .await
            .map_err(|e| Error::from_reason(format!("Read failed: {}", e)))?;

        Ok(events.into_iter().map(EventNapi::from).collect())
    }

    /// Reads events from the global log.
    ///
    /// @param fromPos - Starting position (0 or negative means "from beginning")
    /// @param limit - Maximum number of events to return
    #[napi]
    pub async fn read_global(&self, from_pos: i64, limit: i64) -> Result<Vec<EventNapi>> {
        // GlobalPos doesn't allow 0, so treat <= 0 as "from the beginning" (position 1)
        let safe_pos = if from_pos <= 0 { 1 } else { from_pos as u64 };
        let events = self
            .inner
            .read_global(GlobalPos::from_raw(safe_pos), limit as usize)
            .await
            .map_err(|e| Error::from_reason(format!("Read failed: {}", e)))?;

        Ok(events.into_iter().map(EventNapi::from).collect())
    }

    /// Gets the current revision of a stream.
    ///
    /// @param streamId - The stream to get revision for
    /// @param tenant - Tenant ID (use DEFAULT_TENANT for single-tenant apps)
    #[napi]
    pub async fn get_stream_revision(&self, stream_id: String, tenant: String) -> Result<i64> {
        let tenant_obj = Tenant::new(tenant);
        let rev = self
            .inner
            .get_stream_revision_tenant(stream_id, tenant_obj)
            .await
            .map_err(|e| Error::from_reason(format!("Failed to get revision: {}", e)))?;

        Ok(rev.as_raw() as i64)
    }

    /// Initializes the projection registry.
    ///
    /// @param projectionsDir - Directory where projection databases will be stored.
    ///                         Each projection will have its own .db file in this directory.
    #[napi]
    pub async fn init_projections(&self, projections_dir: String) -> Result<()> {
        let registry =
            ProjectionRegistry::new(std::path::PathBuf::from(projections_dir), self.inner.clone())
                .map_err(|e| Error::from_reason(format!("Failed to init projections: {}", e)))?;

        let mut guard = self.projection_registry.lock().await;
        *guard = Some(registry);

        Ok(())
    }

    /// Registers a projection with the given schema.
    ///
    /// Creates the projection's database file at `{projectionsDir}/{name}.db`.
    /// Tenant isolation is automatically enforced - a `tenant_id` column is
    /// prepended to the schema and becomes part of the composite primary key.
    #[napi]
    pub async fn register_projection(
        &self,
        name: String,
        schema: Vec<ColumnDefNapi>,
    ) -> Result<()> {
        let mut guard = self.projection_registry.lock().await;
        let registry = guard.as_mut().ok_or_else(|| {
            Error::from_reason("Projections not initialized. Call initProjections() first.")
        })?;

        let columns: Vec<ColumnDef> = schema.into_iter().map(ColumnDef::from).collect();
        // Use new_with_tenant to automatically add tenant_id column
        let proj_schema = ProjectionSchema::new_with_tenant(name.clone(), columns);

        registry
            .register(&name, proj_schema)
            .map_err(|e| Error::from_reason(format!("Failed to register projection: {}", e)))?;

        Ok(())
    }

    /// Reads a row from a projection table by tenant_id and primary key (synchronous for proxy support).
    ///
    /// This method is synchronous because the magic proxy syntax (`table[key]`) requires
    /// synchronous property access. The read uses blocking_lock internally.
    /// Tenant isolation is enforced - only rows matching the tenant_id are returned.
    #[napi]
    pub fn read_projection_row(
        &self,
        projection_name: String,
        tenant_id: String,
        key: String,
    ) -> Result<Option<String>> {
        let guard = self.projection_registry.blocking_lock();
        let registry = guard
            .as_ref()
            .ok_or_else(|| Error::from_reason("Projections not initialized"))?;

        // Get the consumer and read synchronously
        let instance = registry
            .get_instance(&projection_name)
            .ok_or_else(|| Error::from_reason(format!("Projection '{}' not found", projection_name)))?;

        let inst_guard = instance.blocking_lock();
        let result = inst_guard
            .read_row(&tenant_id, &key)
            .map_err(|e| Error::from_reason(format!("Read failed: {}", e)))?;

        // Return as JSON string
        Ok(result.map(|v| serde_json::to_string(&v).unwrap_or_default()))
    }

    /// Applies a batch of operations to a projection and updates the checkpoint.
    ///
    /// All operations in the batch must include tenant_id for tenant isolation.
    #[napi]
    pub async fn apply_projection_batch(&self, batch: BatchResultNapi) -> Result<()> {
        let guard = self.projection_registry.lock().await;
        let registry = guard
            .as_ref()
            .ok_or_else(|| Error::from_reason("Projections not initialized"))?;

        let result = BatchResult::from_napi(batch);

        registry
            .apply_batch(
                &result.projection_name,
                result.operations,
                result.last_global_pos,
            )
            .await
            .map_err(|e| Error::from_reason(format!("Apply batch failed: {}", e)))?;

        Ok(())
    }

    /// Gets the next batch of events for a projection.
    #[napi]
    pub async fn get_projection_events(
        &self,
        projection_name: String,
        batch_size: i64,
    ) -> Result<Option<EventBatchNapi>> {
        let guard = self.projection_registry.lock().await;
        let registry = guard
            .as_ref()
            .ok_or_else(|| Error::from_reason("Projections not initialized"))?;

        let result = registry
            .get_events(&projection_name, batch_size as usize)
            .await
            .map_err(|e| Error::from_reason(format!("Failed to get events: {}", e)))?;

        match result {
            Some((events, batch_id)) => Ok(Some(EventBatchNapi {
                projection_name,
                events: events.into_iter().map(EventNapi::from).collect(),
                batch_id,
            })),
            None => Ok(None),
        }
    }

    /// Gets the current checkpoint for a projection.
    #[napi]
    pub async fn get_projection_checkpoint(&self, projection_name: String) -> Result<Option<i64>> {
        let guard = self.projection_registry.lock().await;
        let registry = guard
            .as_ref()
            .ok_or_else(|| Error::from_reason("Projections not initialized"))?;

        registry
            .get_checkpoint(&projection_name)
            .await
            .map_err(|e| Error::from_reason(format!("Failed to get checkpoint: {}", e)))
    }

    /// Deletes all projection data for a tenant (GDPR compliance).
    ///
    /// This is called when a tenant or user is deleted to cascade the deletion
    /// to all their data in projections. Returns the number of rows deleted.
    ///
    /// @param projectionName - Name of the projection to delete from
    /// @param tenantId - Tenant ID whose data should be deleted
    /// @returns number - Count of rows deleted
    #[napi]
    pub async fn delete_tenant_from_projection(
        &self,
        projection_name: String,
        tenant_id: String,
    ) -> Result<i64> {
        let guard = self.projection_registry.lock().await;
        let registry = guard
            .as_ref()
            .ok_or_else(|| Error::from_reason("Projections not initialized"))?;

        let instance = registry
            .get_instance(&projection_name)
            .ok_or_else(|| Error::from_reason(format!("Projection '{}' not found", projection_name)))?;

        let deleted = tokio::task::spawn_blocking(move || {
            let mut inst_guard = instance.blocking_lock();
            inst_guard.delete_tenant(&tenant_id)
        })
        .await
        .map_err(|e| Error::from_reason(format!("Task join error: {}", e)))?
        .map_err(|e| Error::from_reason(format!("Delete failed: {}", e)))?;

        Ok(deleted as i64)
    }

    /// Gets current admission control metrics.
    ///
    /// Returns a snapshot of the adaptive admission control system's state,
    /// useful for monitoring and debugging performance issues.
    ///
    /// @returns AdmissionMetricsNapi - current admission metrics snapshot
    #[napi]
    pub fn get_admission_metrics(&self) -> AdmissionMetricsNapi {
        AdmissionMetricsNapi::from(self.inner.admission_metrics())
    }
}

// =============================================================================
// Telemetry NAPI Wrapper
// =============================================================================

/// NAPI wrapper for the telemetry store.
#[napi]
pub struct TelemetryDbNapi {
    inner: Arc<StdMutex<TelemetryDB>>,
}

#[napi]
impl TelemetryDbNapi {
    /// Opens (or creates) a telemetry store under the given root directory.
    #[napi(factory)]
    pub async fn open(root: String, config: Option<TelemetryConfigNapi>) -> Result<Self> {
        let config = config.unwrap_or_else(TelemetryConfigNapi::default_config);
        let telemetry_config = config.to_config()?;

        let db = tokio::task::spawn_blocking(move || TelemetryDB::open(root, telemetry_config))
            .await
            .map_err(|e| Error::from_reason(format!("Telemetry open task failed: {}", e)))?
            .map_err(|e| Error::from_reason(format!("Failed to open telemetry store: {}", e)))?;

        Ok(Self {
            inner: Arc::new(StdMutex::new(db)),
        })
    }

    /// Writes a single telemetry record.
    #[napi]
    pub async fn write(&self, record: TelemetryRecordNapi) -> Result<()> {
        let db = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let record = record.into_record()?;
            let db = db
                .lock()
                .map_err(|_| Error::from_reason("Telemetry DB lock poisoned"))?;
            db.write(record)
                .map_err(|e| Error::from_reason(format!("Telemetry write failed: {}", e)))
        })
        .await
        .map_err(|e| Error::from_reason(format!("Telemetry write task failed: {}", e)))?
    }

    /// Writes multiple telemetry records.
    #[napi]
    pub async fn write_batch(&self, records: Vec<TelemetryRecordNapi>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let db = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut converted = Vec::with_capacity(records.len());
            for record in records {
                converted.push(record.into_record()?);
            }
            let db = db
                .lock()
                .map_err(|_| Error::from_reason("Telemetry DB lock poisoned"))?;
            db.write_batch(converted)
                .map_err(|e| Error::from_reason(format!("Telemetry batch write failed: {}", e)))
        })
        .await
        .map_err(|e| Error::from_reason(format!("Telemetry write task failed: {}", e)))?
    }

    /// Flushes all telemetry writers.
    #[napi]
    pub async fn flush(&self) -> Result<()> {
        let db = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let db = db
                .lock()
                .map_err(|_| Error::from_reason("Telemetry DB lock poisoned"))?;
            db.flush()
                .map_err(|e| Error::from_reason(format!("Telemetry flush failed: {}", e)))
        })
        .await
        .map_err(|e| Error::from_reason(format!("Telemetry flush task failed: {}", e)))?
    }

    /// Queries telemetry records.
    #[napi]
    pub async fn query(&self, query: TelemetryQueryNapi) -> Result<Vec<TelemetryRecordNapi>> {
        let db = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let query = query.into_query()?;
            let db = db
                .lock()
                .map_err(|_| Error::from_reason("Telemetry DB lock poisoned"))?;
            let records = db
                .query(query)
                .map_err(|e| Error::from_reason(format!("Telemetry query failed: {}", e)))?;
            Ok(records.into_iter().map(TelemetryRecordNapi::from).collect())
        })
        .await
        .map_err(|e| Error::from_reason(format!("Telemetry query task failed: {}", e)))?
    }

    /// Tails a specific shard starting from a cursor.
    #[napi]
    pub async fn tail(
        &self,
        cursor: TelemetryCursorNapi,
        limit: i64,
    ) -> Result<TelemetryTailResultNapi> {
        if limit <= 0 {
            return Ok(TelemetryTailResultNapi {
                records: Vec::new(),
                cursor,
            });
        }
        let db = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let cursor_native = cursor.to_cursor()?;
            let db = db
                .lock()
                .map_err(|_| Error::from_reason("Telemetry DB lock poisoned"))?;
            let (records, next_cursor) = db
                .tail(&cursor_native, limit as usize)
                .map_err(|e| Error::from_reason(format!("Telemetry tail failed: {}", e)))?;
            Ok(TelemetryTailResultNapi {
                records: records.into_iter().map(TelemetryRecordNapi::from).collect(),
                cursor: TelemetryCursorNapi::from(next_cursor),
            })
        })
        .await
        .map_err(|e| Error::from_reason(format!("Telemetry tail task failed: {}", e)))?
    }

    /// Drops telemetry slices older than the retention window.
    #[napi]
    pub async fn cleanup_retention(&self) -> Result<()> {
        let db = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let db = db
                .lock()
                .map_err(|_| Error::from_reason("Telemetry DB lock poisoned"))?;
            db.cleanup_retention().map_err(|e| {
                Error::from_reason(format!("Telemetry retention cleanup failed: {}", e))
            })
        })
        .await
        .map_err(|e| Error::from_reason(format!("Telemetry cleanup task failed: {}", e)))?
    }
}

// =============================================================================
// NAPI Types
// =============================================================================

/// Telemetry record kind.
#[napi(string_enum)]
pub enum TelemetryKindNapi {
    Log,
    Metric,
    Span,
}

impl From<TelemetryKindNapi> for TelemetryKind {
    fn from(value: TelemetryKindNapi) -> Self {
        match value {
            TelemetryKindNapi::Log => TelemetryKind::Log,
            TelemetryKindNapi::Metric => TelemetryKind::Metric,
            TelemetryKindNapi::Span => TelemetryKind::Span,
        }
    }
}

impl From<TelemetryKind> for TelemetryKindNapi {
    fn from(value: TelemetryKind) -> Self {
        match value {
            TelemetryKind::Log => TelemetryKindNapi::Log,
            TelemetryKind::Metric => TelemetryKindNapi::Metric,
            TelemetryKind::Span => TelemetryKindNapi::Span,
        }
    }
}

/// Metric aggregation semantics.
#[napi(string_enum)]
pub enum MetricKindNapi {
    Gauge,
    Counter,
    Histogram,
    Summary,
}

impl From<MetricKindNapi> for MetricKind {
    fn from(value: MetricKindNapi) -> Self {
        match value {
            MetricKindNapi::Gauge => MetricKind::Gauge,
            MetricKindNapi::Counter => MetricKind::Counter,
            MetricKindNapi::Histogram => MetricKind::Histogram,
            MetricKindNapi::Summary => MetricKind::Summary,
        }
    }
}

impl From<MetricKind> for MetricKindNapi {
    fn from(value: MetricKind) -> Self {
        match value {
            MetricKind::Gauge => MetricKindNapi::Gauge,
            MetricKind::Counter => MetricKindNapi::Counter,
            MetricKind::Histogram => MetricKindNapi::Histogram,
            MetricKind::Summary => MetricKindNapi::Summary,
        }
    }
}

/// Span status code.
#[napi(string_enum)]
pub enum SpanStatusNapi {
    Unset,
    Ok,
    Error,
}

impl From<SpanStatusNapi> for SpanStatus {
    fn from(value: SpanStatusNapi) -> Self {
        match value {
            SpanStatusNapi::Unset => SpanStatus::Unset,
            SpanStatusNapi::Ok => SpanStatus::Ok,
            SpanStatusNapi::Error => SpanStatus::Error,
        }
    }
}

impl From<SpanStatus> for SpanStatusNapi {
    fn from(value: SpanStatus) -> Self {
        match value {
            SpanStatus::Unset => SpanStatusNapi::Unset,
            SpanStatus::Ok => SpanStatusNapi::Ok,
            SpanStatus::Error => SpanStatusNapi::Error,
        }
    }
}

/// Sort order for queries.
#[napi(string_enum)]
pub enum TelemetryOrderNapi {
    Asc,
    Desc,
}

impl From<TelemetryOrderNapi> for TelemetryOrder {
    fn from(value: TelemetryOrderNapi) -> Self {
        match value {
            TelemetryOrderNapi::Asc => TelemetryOrder::Asc,
            TelemetryOrderNapi::Desc => TelemetryOrder::Desc,
        }
    }
}

impl From<TelemetryOrder> for TelemetryOrderNapi {
    fn from(value: TelemetryOrder) -> Self {
        match value {
            TelemetryOrder::Asc => TelemetryOrderNapi::Asc,
            TelemetryOrder::Desc => TelemetryOrderNapi::Desc,
        }
    }
}

/// Time-slice policy.
#[napi(string_enum)]
pub enum TimeSliceNapi {
    Daily,
}

impl From<TimeSliceNapi> for TimeSlice {
    fn from(value: TimeSliceNapi) -> Self {
        match value {
            TimeSliceNapi::Daily => TimeSlice::Daily,
        }
    }
}

/// Configuration for telemetry storage.
#[napi(object)]
pub struct TelemetryConfigNapi {
    pub app_name: Option<String>,
    pub partitions: Option<u32>,
    pub batch_max_ms: Option<u32>,
    pub batch_max_bytes: Option<u32>,
    pub batch_max_records: Option<u32>,
    pub max_inflight: Option<u32>,
    pub retention_days: Option<u32>,
    pub time_slice: Option<TimeSliceNapi>,
    pub default_service: Option<String>,
}

impl TelemetryConfigNapi {
    fn default_config() -> Self {
        Self {
            app_name: Some("app".to_string()),
            partitions: None,
            batch_max_ms: None,
            batch_max_bytes: None,
            batch_max_records: None,
            max_inflight: None,
            retention_days: None,
            time_slice: None,
            default_service: None,
        }
    }

    fn to_config(&self) -> Result<TelemetryConfig> {
        let app_name = self.app_name.clone().unwrap_or_else(|| "app".to_string());
        let mut config = TelemetryConfig::new(app_name);

        if let Some(partitions) = self.partitions {
            if partitions == 0 {
                return Err(Error::from_reason("telemetry partitions must be >= 1"));
            }
            config.partitions = partitions as usize;
        }

        if let Some(batch_max_ms) = self.batch_max_ms {
            config.batch_max_ms = batch_max_ms as u64;
        }

        if let Some(batch_max_bytes) = self.batch_max_bytes {
            config.batch_max_bytes = batch_max_bytes as usize;
        }

        if let Some(batch_max_records) = self.batch_max_records {
            config.batch_max_records = batch_max_records as usize;
        }

        if let Some(max_inflight) = self.max_inflight {
            config.max_inflight = max_inflight as usize;
        }

        if let Some(retention_days) = self.retention_days {
            config.retention_days = retention_days as u64;
        }

        if let Some(time_slice) = self.time_slice {
            config.time_slice = time_slice.into();
        }

        if let Some(default_service) = self.default_service.clone() {
            config.default_service = Some(default_service);
        }

        Ok(config)
    }
}

/// Cursor for tailing a telemetry shard.
#[napi(object)]
pub struct TelemetryCursorNapi {
    pub slice: String,
    pub last_ids: Vec<i64>,
}

impl TelemetryCursorNapi {
    fn to_cursor(&self) -> Result<TelemetryCursor> {
        Ok(TelemetryCursor {
            slice: self.slice.clone(),
            last_ids: self.last_ids.clone(),
        })
    }
}

impl From<TelemetryCursor> for TelemetryCursorNapi {
    fn from(cursor: TelemetryCursor) -> Self {
        Self {
            slice: cursor.slice,
            last_ids: cursor.last_ids,
        }
    }
}

/// Telemetry record payload for writes and reads.
#[napi(object)]
pub struct TelemetryRecordNapi {
    pub ts_ms: i64,
    pub kind: TelemetryKindNapi,
    pub tenant_id: Option<String>,
    pub tenant_hash: Option<i64>,

    pub event_global_pos: Option<i64>,
    pub stream_id: Option<String>,
    pub stream_hash: Option<i64>,
    pub stream_rev: Option<i64>,
    pub command_id: Option<String>,

    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,

    pub name: Option<String>,
    pub service: Option<String>,

    pub severity: Option<i64>,
    pub message: Option<String>,

    pub metric_name: Option<String>,
    pub metric_value: Option<f64>,
    pub metric_kind: Option<MetricKindNapi>,
    pub metric_unit: Option<String>,

    pub span_start_ms: Option<i64>,
    pub span_end_ms: Option<i64>,
    pub span_duration_ms: Option<i64>,
    pub span_status: Option<SpanStatusNapi>,

    pub attrs_json: Option<String>,
}

impl TelemetryRecordNapi {
    fn into_record(self) -> Result<TelemetryRecord> {
        let ts_ms = non_negative("tsMs", self.ts_ms)? as u64;
        let tenant_hash = if let Some(hash) = self.tenant_hash {
            TenantHash::from_raw(hash)
        } else {
            let tenant = self
                .tenant_id
                .clone()
                .unwrap_or_else(|| Tenant::DEFAULT_STR.to_string());
            Tenant::new(tenant).hash()
        };

        let mut record = TelemetryRecord::new(self.kind.into(), ts_ms, tenant_hash);

        record.event_global_pos = optional_global_pos(self.event_global_pos)?;

        record.stream_hash = if let Some(stream_id) = self.stream_id {
            Some(StreamId::new(stream_id).hash())
        } else {
            self.stream_hash.map(StreamHash::from_raw)
        };

        record.stream_rev = optional_stream_rev(self.stream_rev)?;
        record.command_id = self.command_id.map(CommandId::from);

        record.trace_id = self.trace_id;
        record.span_id = self.span_id;
        record.parent_span_id = self.parent_span_id;

        record.name = self.name;
        record.service = self.service;

        record.severity = self.severity;
        record.message = self.message;

        record.metric_name = self.metric_name;
        record.metric_value = self.metric_value;
        record.metric_kind = self.metric_kind.map(Into::into);
        record.metric_unit = self.metric_unit;

        record.span_start_ms = optional_u64(self.span_start_ms, "spanStartMs")?;
        record.span_end_ms = optional_u64(self.span_end_ms, "spanEndMs")?;
        record.span_duration_ms = optional_u64(self.span_duration_ms, "spanDurationMs")?;
        record.span_status = self.span_status.map(Into::into);

        record.attrs_json = self.attrs_json;

        Ok(record)
    }
}

impl From<TelemetryRecord> for TelemetryRecordNapi {
    fn from(record: TelemetryRecord) -> Self {
        Self {
            ts_ms: record.ts_ms as i64,
            kind: record.kind.into(),
            tenant_id: None,
            tenant_hash: Some(record.tenant_hash.as_raw()),
            event_global_pos: record.event_global_pos.map(|pos| pos.as_raw() as i64),
            stream_id: None,
            stream_hash: record.stream_hash.map(|hash| hash.as_raw()),
            stream_rev: record.stream_rev.map(|rev| rev.as_raw() as i64),
            command_id: record.command_id.map(|id| id.as_str().to_string()),
            trace_id: record.trace_id,
            span_id: record.span_id,
            parent_span_id: record.parent_span_id,
            name: record.name,
            service: record.service,
            severity: record.severity,
            message: record.message,
            metric_name: record.metric_name,
            metric_value: record.metric_value,
            metric_kind: record.metric_kind.map(Into::into),
            metric_unit: record.metric_unit,
            span_start_ms: record.span_start_ms.map(|value| value as i64),
            span_end_ms: record.span_end_ms.map(|value| value as i64),
            span_duration_ms: record.span_duration_ms.map(|value| value as i64),
            span_status: record.span_status.map(Into::into),
            attrs_json: record.attrs_json,
        }
    }
}

/// Query parameters for telemetry searches.
#[napi(object)]
pub struct TelemetryQueryNapi {
    pub tenant_id: Option<String>,
    pub tenant_hash: Option<i64>,
    pub kind: Option<TelemetryKindNapi>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub severity: Option<i64>,
    pub metric_name: Option<String>,
    pub event_global_pos: Option<i64>,
    pub stream_id: Option<String>,
    pub stream_hash: Option<i64>,
    pub stream_rev: Option<i64>,
    pub command_id: Option<String>,
    pub trace_id: Option<String>,
    pub limit: Option<i64>,
    pub order: Option<TelemetryOrderNapi>,
    pub slice: Option<String>,
    pub shard_id: Option<i64>,
}

impl TelemetryQueryNapi {
    fn into_query(self) -> Result<TelemetryQuery> {
        let mut query = TelemetryQuery::new();

        query.tenant_hash = if let Some(hash) = self.tenant_hash {
            Some(TenantHash::from_raw(hash))
        } else {
            self.tenant_id.map(|tenant| Tenant::new(tenant).hash())
        };

        query.kind = self.kind.map(Into::into);
        query.start_ms = optional_u64(self.start_ms, "startMs")?;
        query.end_ms = optional_u64(self.end_ms, "endMs")?;
        query.severity = self.severity;
        query.metric_name = self.metric_name;
        query.event_global_pos = optional_global_pos(self.event_global_pos)?;

        query.stream_hash = if let Some(stream_id) = self.stream_id {
            Some(StreamId::new(stream_id).hash())
        } else {
            self.stream_hash.map(StreamHash::from_raw)
        };

        query.stream_rev = optional_stream_rev(self.stream_rev)?;
        query.command_id = self.command_id.map(CommandId::from);
        query.trace_id = self.trace_id;

        query.limit = if let Some(limit) = self.limit {
            if limit < 0 {
                return Err(Error::from_reason("limit must be >= 0"));
            }
            Some(limit as usize)
        } else {
            None
        };

        query.order = self.order.map(Into::into).unwrap_or(TelemetryOrder::Desc);
        query.slice = self.slice;
        query.shard_id = if let Some(shard_id) = self.shard_id {
            if shard_id < 0 {
                return Err(Error::from_reason("shardId must be >= 0"));
            }
            Some(shard_id as usize)
        } else {
            None
        };

        Ok(query)
    }
}

/// Result of a telemetry tail call.
#[napi(object)]
pub struct TelemetryTailResultNapi {
    pub records: Vec<TelemetryRecordNapi>,
    pub cursor: TelemetryCursorNapi,
}

fn non_negative(field: &str, value: i64) -> Result<u64> {
    if value < 0 {
        return Err(Error::from_reason(format!(
            "{field} must be >= 0"
        )));
    }
    Ok(value as u64)
}

fn optional_u64(value: Option<i64>, field: &str) -> Result<Option<u64>> {
    match value {
        Some(value) => Ok(Some(non_negative(field, value)?)),
        None => Ok(None),
    }
}

fn optional_global_pos(value: Option<i64>) -> Result<Option<GlobalPos>> {
    match value {
        Some(value) => {
            let raw = non_negative("eventGlobalPos", value)?;
            if raw == 0 {
                return Err(Error::from_reason("eventGlobalPos must be >= 1"));
            }
            Ok(Some(GlobalPos::from_raw(raw)))
        }
        None => Ok(None),
    }
}

fn optional_stream_rev(value: Option<i64>) -> Result<Option<StreamRev>> {
    match value {
        Some(value) => Ok(Some(StreamRev::from_raw(non_negative("streamRev", value)?))),
        None => Ok(None),
    }
}

/// Result of an append operation.
#[napi(object)]
pub struct AppendResultNapi {
    /// First global position assigned
    pub first_pos: i64,
    /// Last global position assigned
    pub last_pos: i64,
    /// First stream revision assigned
    pub first_rev: i64,
    /// Last stream revision assigned
    pub last_rev: i64,
}

/// Append command payload for atomic transactions.
#[napi(object)]
pub struct AppendCommandNapi {
    /// The stream to append to
    pub stream_id: String,
    /// Unique command ID for idempotency
    pub command_id: String,
    /// Expected revision: -1 for "any", 0 for "stream must not exist", >0 for exact revision
    pub expected_rev: i64,
    /// Array of event data buffers
    pub events: Vec<Buffer>,
    /// Tenant ID (use DEFAULT_TENANT for single-tenant apps)
    pub tenant: String,
}

/// Append command payload for batch appends (tenant specified at batch level).
#[napi(object)]
pub struct BatchAppendCommandNapi {
    /// The stream to append to
    pub stream_id: String,
    /// Unique command ID for idempotency
    pub command_id: String,
    /// Expected revision: -1 for "any", 0 for "stream must not exist", >0 for exact revision
    pub expected_rev: i64,
    /// Array of event data buffers
    pub events: Vec<Buffer>,
}

/// SQL parameter for atomic transactions.
#[napi(object)]
pub struct SqlParamNapi {
    /// Parameter type: "null", "integer", "real", "text", "blob", "bool"
    pub kind: String,
    /// String value (used for integer/real/text/bool)
    pub value: Option<String>,
    /// Blob value (used for blob)
    pub blob: Option<Buffer>,
}

/// SQL statement for atomic transactions.
#[napi(object)]
pub struct SqlStatementNapi {
    /// SQL statement with ? placeholders
    pub sql: String,
    /// Parameters for the statement
    pub params: Vec<SqlParamNapi>,
}

impl From<AppendResult> for AppendResultNapi {
    fn from(result: AppendResult) -> Self {
        Self {
            first_pos: result.first_pos.as_raw() as i64,
            last_pos: result.last_pos.as_raw() as i64,
            first_rev: result.first_rev.as_raw() as i64,
            last_rev: result.last_rev.as_raw() as i64,
        }
    }
}

/// An event read from the store.
#[napi(object)]
pub struct EventNapi {
    /// Global position in the log
    pub global_pos: i64,
    /// Stream this event belongs to
    pub stream_id: String,
    /// Tenant hash for this event
    pub tenant_hash: i64,
    /// Revision within the stream
    pub stream_rev: i64,
    /// Timestamp when stored (Unix milliseconds)
    pub timestamp_ms: i64,
    /// Event payload
    pub data: Buffer,
}

impl From<Event> for EventNapi {
    fn from(event: Event) -> Self {
        Self {
            global_pos: event.global_pos.as_raw() as i64,
            stream_id: event.stream_id.to_string(),
            tenant_hash: event.tenant_hash.as_raw(),
            stream_rev: event.stream_rev.as_raw() as i64,
            timestamp_ms: event.timestamp_ms as i64,
            data: Buffer::from(event.data),
        }
    }
}

/// A batch of events for projection processing.
#[napi(object)]
pub struct EventBatchNapi {
    /// Name of the projection this batch is for
    pub projection_name: String,
    /// Events in the batch
    pub events: Vec<EventNapi>,
    /// Batch ID for acknowledgment
    pub batch_id: i64,
}

/// Column definition for a projection schema.
#[napi(object)]
pub struct ColumnDefNapi {
    /// Column name
    pub name: String,
    /// Column type: "text", "integer", "real", "blob", "boolean"
    pub col_type: String,
    /// Whether this column is part of the primary key
    pub primary_key: bool,
    /// Whether this column allows NULL values
    pub nullable: bool,
    /// Default value (as JSON string)
    pub default_value: Option<String>,
}

/// A single projection operation.
#[napi(object)]
pub struct ProjectionOpNapi {
    /// Operation type: "upsert" or "delete"
    pub op_type: String,
    /// Primary key value
    pub key: String,
    /// Row values for upsert (JSON string)
    pub value: Option<String>,
}

/// Result of processing a batch - operations to apply.
#[napi(object)]
pub struct BatchResultNapi {
    /// Name of the projection
    pub projection_name: String,
    /// Tenant ID for all operations in this batch (framework-enforced)
    pub tenant_id: String,
    /// Operations to apply
    pub operations: Vec<ProjectionOpNapi>,
    /// Last global position processed (for checkpoint)
    pub last_global_pos: i64,
}

/// Admission control metrics snapshot.
///
/// Provides visibility into the adaptive admission control system's state.
#[napi(object)]
pub struct AdmissionMetricsNapi {
    /// Current max in-flight events (auto-adjusted based on observed latency)
    pub current_limit: i64,
    /// Observed p99 latency in milliseconds
    pub observed_p99_ms: f64,
    /// Target p99 latency in milliseconds (default: 60)
    pub target_p99_ms: f64,
    /// Total requests that completed successfully
    pub requests_accepted: i64,
    /// Total requests that timed out due to backpressure
    pub requests_rejected: i64,
    /// Ratio of rejected to total requests (0.0 to 1.0)
    pub rejection_rate: f64,
    /// Number of times the controller adjusted the max_inflight limit
    pub adjustments: i64,
}

impl From<MetricsSnapshot> for AdmissionMetricsNapi {
    fn from(m: MetricsSnapshot) -> Self {
        Self {
            current_limit: m.current_limit as i64,
            observed_p99_ms: m.observed_p99_ms,
            target_p99_ms: m.target_p99_ms,
            requests_accepted: m.requests_accepted as i64,
            requests_rejected: m.requests_rejected as i64,
            rejection_rate: m.rejection_rate,
            adjustments: m.adjustments as i64,
        }
    }
}

fn build_append_command(cmd: AppendCommandNapi) -> Result<AppendCommand> {
    if cmd.events.is_empty() {
        return Err(Error::from_reason("events array cannot be empty"));
    }

    let expected = if cmd.expected_rev < 0 {
        StreamRev::ANY
    } else {
        StreamRev::from_raw(cmd.expected_rev as u64)
    };

    let event_data: Vec<EventData> = cmd
        .events
        .into_iter()
        .map(|buf| EventData::new(buf.to_vec()))
        .collect();

    let tenant_obj = Tenant::new(cmd.tenant);

    Ok(AppendCommand::new_with_tenant(
        CommandId::new(cmd.command_id),
        StreamId::new(cmd.stream_id),
        tenant_obj,
        expected,
        event_data,
    ))
}

fn parse_sql_param(param: SqlParamNapi) -> Result<SqlValue> {
    match param.kind.to_lowercase().as_str() {
        "null" => Ok(SqlValue::Null),
        "integer" => {
            let value = param
                .value
                .ok_or_else(|| Error::from_reason("integer param missing value"))?;
            let parsed = value
                .parse::<i64>()
                .map_err(|_| Error::from_reason("integer param must be i64"))?;
            Ok(SqlValue::Integer(parsed))
        }
        "real" => {
            let value = param
                .value
                .ok_or_else(|| Error::from_reason("real param missing value"))?;
            let parsed = value
                .parse::<f64>()
                .map_err(|_| Error::from_reason("real param must be f64"))?;
            Ok(SqlValue::Real(parsed))
        }
        "text" => {
            let value = param
                .value
                .ok_or_else(|| Error::from_reason("text param missing value"))?;
            Ok(SqlValue::Text(value))
        }
        "bool" => {
            let value = param
                .value
                .ok_or_else(|| Error::from_reason("bool param missing value"))?;
            let parsed = match value.as_str() {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => {
                    return Err(Error::from_reason(
                        "bool param must be true/false/1/0",
                    ))
                }
            };
            Ok(SqlValue::Bool(parsed))
        }
        "blob" => {
            let value = param
                .blob
                .ok_or_else(|| Error::from_reason("blob param missing value"))?;
            Ok(SqlValue::Blob(value.to_vec()))
        }
        other => Err(Error::from_reason(format!(
            "unsupported SQL param kind: {}",
            other
        ))),
    }
}
