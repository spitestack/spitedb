//! # Telemetry Store
//!
//! Provides a partitioned, time-sliced SQLite telemetry store for logs,
//! metrics, and traces.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate, Utc};
use rusqlite::{params_from_iter, Connection};

use crate::types::{CommandId, GlobalPos, StreamHash, StreamRev, TenantHash};
use crate::{Error, Result};

mod schema;
mod types;
mod writer;

pub use types::{
    EventRef, MetricKind, SpanStatus, TelemetryConfig, TelemetryCursor, TelemetryKind,
    TelemetryOrder, TelemetryQuery, TelemetryRecord, TimeSlice,
};

use schema::init_schema;
use writer::{spawn_writer, TelemetryWriterHandle};

const TAIL_POLL_INTERVAL_MS: u64 = 50;
const TAIL_WAIT_MS: u64 = 2_000;

/// Telemetry database handle.
#[derive(Debug)]
pub struct TelemetryDB {
    app_dir: PathBuf,
    config: TelemetryConfig,
    writers: Vec<TelemetryWriterHandle>,
    fallback_counter: AtomicU64,
}

impl TelemetryDB {
    /// Opens (or creates) the telemetry store under the given root directory.
    pub fn open(telemetry_root: impl AsRef<Path>, mut config: TelemetryConfig) -> Result<Self> {
        if config.partitions == 0 {
            return Err(Error::Schema("telemetry partitions must be >= 1".to_string()));
        }
        if config.batch_max_records == 0 {
            return Err(Error::Schema("telemetry batch_max_records must be >= 1".to_string()));
        }

        if config.default_service.is_none() {
            config.default_service = Some(config.app_name.clone());
        }

        let root = telemetry_root.as_ref().to_path_buf();
        let app_dir = root.join(&config.app_name);
        std::fs::create_dir_all(&app_dir)
            .map_err(|err| Error::Schema(format!("failed to create telemetry root: {}", err)))?;

        let mut writers = Vec::with_capacity(config.partitions);
        for shard_id in 0..config.partitions {
            writers.push(spawn_writer(app_dir.clone(), shard_id, config.clone()));
        }

        let db = Self {
            app_dir,
            config,
            writers,
            fallback_counter: AtomicU64::new(0),
        };

        db.cleanup_retention()?;
        Ok(db)
    }

    /// Writes a single telemetry record.
    pub fn write(&self, record: TelemetryRecord) -> Result<()> {
        let shard_id = self.partition_for_record(&record);
        self.writers[shard_id].write(record)
    }

    /// Writes multiple telemetry records (grouped by shard and slice).
    pub fn write_batch(&self, records: Vec<TelemetryRecord>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let mut grouped: Vec<BTreeMap<String, Vec<TelemetryRecord>>> =
            vec![BTreeMap::new(); self.config.partitions];

        for record in records {
            let shard_id = self.partition_for_record(&record);
            let slice = slice_from_ts_ms(record.ts_ms)?;
            grouped[shard_id].entry(slice).or_default().push(record);
        }

        for (shard_id, slices) in grouped.into_iter().enumerate() {
            for (_slice, batch) in slices {
                self.writers[shard_id].write_batch(batch)?;
            }
        }

        Ok(())
    }

    /// Flushes all shard writers.
    pub fn flush(&self) -> Result<()> {
        for writer in &self.writers {
            writer.flush()?;
        }
        Ok(())
    }

    /// Shuts down all writer threads.
    pub fn shutdown(mut self) {
        let writers = std::mem::take(&mut self.writers);
        for writer in writers {
            writer.shutdown();
        }
    }

    /// Queries telemetry records based on a filter.
    pub fn query(&self, query: TelemetryQuery) -> Result<Vec<TelemetryRecord>> {
        let slices = self.resolve_slices(&query)?;
        let shard_ids = self.resolve_shards(&query)?;

        let mut results = Vec::new();
        for slice in slices {
            for shard_id in &shard_ids {
                if let Some(conn) = self.open_read_connection(&slice, *shard_id)? {
                    let mut shard_results = query_shard(&conn, &query)?;
                    results.append(&mut shard_results);
                }
            }
        }

        results.sort_by(|a, b| a.ts_ms.cmp(&b.ts_ms));
        if query.order == TelemetryOrder::Desc {
            results.reverse();
        }

        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    /// Tails a specific shard starting from the provided cursor.
    pub fn tail(
        &self,
        cursor: &TelemetryCursor,
        limit: usize,
    ) -> Result<(Vec<TelemetryRecord>, TelemetryCursor)> {
        let start = Instant::now();
        let mut current_cursor = normalize_cursor(cursor, self.config.partitions);

        loop {
            let (records, next_cursor) = self.tail_once(&current_cursor, limit)?;
            if !records.is_empty() {
                return Ok((records, next_cursor));
            }

            if start.elapsed() >= Duration::from_millis(TAIL_WAIT_MS) {
                return Ok((Vec::new(), current_cursor));
            }

            let live_slice = current_slice()?;
            if current_cursor.slice < live_slice {
                current_cursor = TelemetryCursor::new(live_slice, self.config.partitions);
            }

            std::thread::sleep(Duration::from_millis(TAIL_POLL_INTERVAL_MS));
        }
    }

    /// Removes telemetry slices older than the retention window.
    pub fn cleanup_retention(&self) -> Result<()> {
        if self.config.retention_days == 0 {
            return Ok(());
        }

        let today = Utc::now().date_naive();
        let cutoff = today - chrono::Duration::days(self.config.retention_days as i64);

        if !self.app_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.app_dir)
            .map_err(|err| Error::Schema(format!("failed to read telemetry root: {}", err)))?
        {
            let entry = entry.map_err(|err| {
                Error::Schema(format!("failed to read telemetry root entry: {}", err))
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = entry.file_name();
            let name = name.to_string_lossy();
            let date = match NaiveDate::parse_from_str(&name, "%Y-%m-%d") {
                Ok(date) => date,
                Err(_) => continue,
            };

            if date < cutoff {
                std::fs::remove_dir_all(&path).map_err(|err| {
                    Error::Schema(format!("failed to remove telemetry slice {}: {}", name, err))
                })?;
            }
        }

        Ok(())
    }

    fn resolve_shards(&self, query: &TelemetryQuery) -> Result<Vec<usize>> {
        if let Some(shard_id) = query.shard_id {
            if shard_id >= self.config.partitions {
                return Err(Error::Schema(format!(
                    "telemetry shard {} out of range",
                    shard_id
                )));
            }
            return Ok(vec![shard_id]);
        }

        Ok((0..self.config.partitions).collect())
    }

    fn resolve_slices(&self, query: &TelemetryQuery) -> Result<Vec<String>> {
        if let Some(slice) = &query.slice {
            return Ok(vec![slice.clone()]);
        }

        if query.start_ms.is_some() || query.end_ms.is_some() {
            let slice_list = self.list_slices().ok().unwrap_or_default();
            if slice_list.is_empty() && query.start_ms.is_none() {
                return Ok(vec![current_slice()?]);
            }

            let start = query.start_ms.unwrap_or_else(|| {
                let earliest = slice_list
                    .first()
                    .cloned()
                    .and_then(|slice| NaiveDate::parse_from_str(&slice, "%Y-%m-%d").ok());
                earliest
                    .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis() as u64)
                    .unwrap_or_else(now_ms)
            });
            let end = query.end_ms.unwrap_or_else(now_ms);
            return slices_for_range(start, end);
        }

        Ok(vec![current_slice()?])
    }

    fn list_slices(&self) -> Result<Vec<String>> {
        let mut slices = Vec::new();
        if !self.app_dir.exists() {
            return Ok(slices);
        }

        for entry in std::fs::read_dir(&self.app_dir)
            .map_err(|err| Error::Schema(format!("failed to read telemetry root: {}", err)))?
        {
            let entry = entry.map_err(|err| {
                Error::Schema(format!("failed to read telemetry root entry: {}", err))
            })?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if NaiveDate::parse_from_str(&name, "%Y-%m-%d").is_ok() {
                slices.push(name.to_string());
            }
        }

        slices.sort();
        Ok(slices)
    }

    fn open_read_connection(&self, slice: &str, shard_id: usize) -> Result<Option<Connection>> {
        let shard_path = shard_path(&self.app_dir, slice, shard_id);
        if !shard_path.exists() {
            return Ok(None);
        }
        let conn = Connection::open(shard_path)?;
        init_schema(&conn)?;
        Ok(Some(conn))
    }

    fn partition_for_record(&self, record: &TelemetryRecord) -> usize {
        if let Some(trace_id) = record.trace_id.as_deref() {
            return (xxhash_rust::xxh3::xxh3_64(trace_id.as_bytes())
                % self.config.partitions as u64) as usize;
        }

        if let Some(stream_hash) = record.stream_hash {
            return (stream_hash.as_raw() as u64 % self.config.partitions as u64) as usize;
        }

        if let Some(command_id) = record.command_id.as_ref() {
            return (xxhash_rust::xxh3::xxh3_64(command_id.as_str().as_bytes())
                % self.config.partitions as u64) as usize;
        }

        let tenant_hash = record.tenant_hash.as_raw() as u64;
        if tenant_hash != 0 {
            return (tenant_hash % self.config.partitions as u64) as usize;
        }

        let counter = self.fallback_counter.fetch_add(1, Ordering::Relaxed);
        let seed = record.ts_ms ^ counter;
        (xxhash_rust::xxh3::xxh3_64(&seed.to_le_bytes()) % self.config.partitions as u64) as usize
    }

    fn tail_once(
        &self,
        cursor: &TelemetryCursor,
        limit: usize,
    ) -> Result<(Vec<TelemetryRecord>, TelemetryCursor)> {
        let mut combined: Vec<(usize, TelemetryRecord)> = Vec::new();
        let per_shard_limit = limit.max(1);

        for shard_id in 0..self.config.partitions {
            let last_id = cursor
                .last_ids
                .get(shard_id)
                .copied()
                .unwrap_or(0);
            let conn = match self.open_read_connection(&cursor.slice, shard_id)? {
                Some(conn) => conn,
                None => continue,
            };

            let mut stmt = conn.prepare(
                "SELECT id, ts_ms, kind, tenant_hash, event_global_pos, stream_hash, stream_rev, command_id,
                        trace_id, span_id, parent_span_id, name, service, severity, message,
                        metric_name, metric_value, metric_kind, metric_unit, span_start_ms, span_end_ms,
                        span_duration_ms, span_status, attrs_json
                 FROM telemetry
                 WHERE id > ?
                 ORDER BY id
                 LIMIT ?",
            )?;

            let mut rows = stmt.query([last_id, per_shard_limit as i64])?;
            while let Some(row) = rows.next()? {
                let record = row_to_record(row)?;
                combined.push((shard_id, record));
            }
        }

        if combined.is_empty() {
            return Ok((Vec::new(), cursor.clone()));
        }

        combined.sort_by(|a, b| {
            let ts_cmp = a.1.ts_ms.cmp(&b.1.ts_ms);
            if ts_cmp == std::cmp::Ordering::Equal {
                a.1.id.cmp(&b.1.id)
            } else {
                ts_cmp
            }
        });

        let mut next_cursor = cursor.clone();
        if next_cursor.last_ids.len() < self.config.partitions {
            next_cursor
                .last_ids
                .resize(self.config.partitions, 0);
        }

        let mut records = Vec::new();
        for (shard_id, record) in combined.into_iter().take(limit) {
            if let Some(id) = record.id {
                let slot = next_cursor
                    .last_ids
                    .get_mut(shard_id)
                    .expect("cursor shard index exists");
                if id > *slot {
                    *slot = id;
                }
            }
            records.push(record);
        }

        Ok((records, next_cursor))
    }
}

impl Drop for TelemetryDB {
    fn drop(&mut self) {
        let writers = std::mem::take(&mut self.writers);
        for writer in writers {
            writer.shutdown();
        }
    }
}

pub(crate) fn slice_from_ts_ms(ts_ms: u64) -> Result<String> {
    let ts = ts_ms as i64;
    let datetime = chrono::DateTime::<Utc>::from_timestamp_millis(ts).ok_or_else(|| {
        Error::Schema("telemetry timestamp is out of range".to_string())
    })?;
    let date = datetime.date_naive();
    Ok(format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day()))
}

fn current_slice() -> Result<String> {
    slice_from_ts_ms(now_ms())
}

fn now_ms() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis() as u64
}

fn slices_for_range(start_ms: u64, end_ms: u64) -> Result<Vec<String>> {
    let start_date = chrono::DateTime::<Utc>::from_timestamp_millis(start_ms as i64)
        .ok_or_else(|| Error::Schema("telemetry start time is out of range".to_string()))?
        .date_naive();
    let end_date = chrono::DateTime::<Utc>::from_timestamp_millis(end_ms as i64)
        .ok_or_else(|| Error::Schema("telemetry end time is out of range".to_string()))?
        .date_naive();

    if end_date < start_date {
        return Ok(Vec::new());
    }

    let mut slices = Vec::new();
    let mut cursor = start_date;
    while cursor <= end_date {
        slices.push(format!("{:04}-{:02}-{:02}", cursor.year(), cursor.month(), cursor.day()));
        cursor = cursor.succ_opt().unwrap();
    }

    Ok(slices)
}

fn shard_path(app_dir: &Path, slice: &str, shard_id: usize) -> PathBuf {
    app_dir
        .join(slice)
        .join(format!("shard-{:03}.db", shard_id))
}

fn normalize_cursor(cursor: &TelemetryCursor, partitions: usize) -> TelemetryCursor {
    let mut next = cursor.clone();
    if next.last_ids.len() < partitions {
        next.last_ids.resize(partitions, 0);
    } else if next.last_ids.len() > partitions {
        next.last_ids.truncate(partitions);
    }
    next
}

fn query_shard(conn: &Connection, query: &TelemetryQuery) -> Result<Vec<TelemetryRecord>> {
    let mut clauses: Vec<&str> = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(tenant_hash) = query.tenant_hash {
        clauses.push("tenant_hash = ?");
        params.push(tenant_hash.as_raw().into());
    }

    if let Some(kind) = query.kind {
        clauses.push("kind = ?");
        params.push(kind.as_i64().into());
    }

    if let Some(start_ms) = query.start_ms {
        clauses.push("ts_ms >= ?");
        params.push((start_ms as i64).into());
    }

    if let Some(end_ms) = query.end_ms {
        clauses.push("ts_ms <= ?");
        params.push((end_ms as i64).into());
    }

    if let Some(severity) = query.severity {
        clauses.push("severity = ?");
        params.push(severity.into());
    }

    if let Some(metric_name) = query.metric_name.as_deref() {
        clauses.push("metric_name = ?");
        params.push(metric_name.to_string().into());
    }

    if let Some(event_global_pos) = query.event_global_pos {
        clauses.push("event_global_pos = ?");
        params.push((event_global_pos.as_raw() as i64).into());
    }

    if let Some(stream_hash) = query.stream_hash {
        clauses.push("stream_hash = ?");
        params.push(stream_hash.as_raw().into());
    }

    if let Some(stream_rev) = query.stream_rev {
        clauses.push("stream_rev = ?");
        params.push((stream_rev.as_raw() as i64).into());
    }

    if let Some(command_id) = query.command_id.as_ref() {
        clauses.push("command_id = ?");
        params.push(command_id.as_str().to_string().into());
    }

    if let Some(trace_id) = query.trace_id.as_deref() {
        clauses.push("trace_id = ?");
        params.push(trace_id.to_string().into());
    }

    let mut sql = String::from(
        "SELECT id, ts_ms, kind, tenant_hash, event_global_pos, stream_hash, stream_rev, command_id,
                trace_id, span_id, parent_span_id, name, service, severity, message,
                metric_name, metric_value, metric_kind, metric_unit, span_start_ms, span_end_ms,
                span_duration_ms, span_status, attrs_json
         FROM telemetry",
    );

    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }

    let order = if query.order == TelemetryOrder::Asc {
        "ASC"
    } else {
        "DESC"
    };

    sql.push_str(&format!(" ORDER BY ts_ms {}", order));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(params))?;
    let mut records = Vec::new();

    while let Some(row) = rows.next()? {
        records.push(row_to_record(row)?);
    }

    Ok(records)
}

fn row_to_record(row: &rusqlite::Row<'_>) -> Result<TelemetryRecord> {
    let id: i64 = row.get(0)?;
    let ts_ms: i64 = row.get(1)?;
    let kind_value: i64 = row.get(2)?;
    let tenant_hash: i64 = row.get(3)?;

    let kind = TelemetryKind::from_i64(kind_value).ok_or_else(|| {
        Error::Schema(format!("invalid telemetry kind {}", kind_value))
    })?;

    let event_global_pos: Option<i64> = row.get(4)?;
    let stream_hash: Option<i64> = row.get(5)?;
    let stream_rev: Option<i64> = row.get(6)?;
    let command_id: Option<String> = row.get(7)?;

    let trace_id: Option<String> = row.get(8)?;
    let span_id: Option<String> = row.get(9)?;
    let parent_span_id: Option<String> = row.get(10)?;

    let name: Option<String> = row.get(11)?;
    let service: Option<String> = row.get(12)?;

    let severity: Option<i64> = row.get(13)?;
    let message: Option<String> = row.get(14)?;

    let metric_name: Option<String> = row.get(15)?;
    let metric_value: Option<f64> = row.get(16)?;
    let metric_kind: Option<i64> = row.get(17)?;
    let metric_unit: Option<String> = row.get(18)?;

    let span_start_ms: Option<i64> = row.get(19)?;
    let span_end_ms: Option<i64> = row.get(20)?;
    let span_duration_ms: Option<i64> = row.get(21)?;
    let span_status: Option<i64> = row.get(22)?;

    let attrs_json: Option<String> = row.get(23)?;

    Ok(TelemetryRecord {
        id: Some(id),
        ts_ms: ts_ms as u64,
        kind,
        tenant_hash: TenantHash::from_raw(tenant_hash),
        event_global_pos: event_global_pos.map(|pos| GlobalPos::from_raw_unchecked(pos as u64)),
        stream_hash: stream_hash.map(StreamHash::from_raw),
        stream_rev: stream_rev.map(|rev| StreamRev::from_raw(rev as u64)),
        command_id: command_id.map(CommandId::from),
        trace_id,
        span_id,
        parent_span_id,
        name,
        service,
        severity,
        message,
        metric_name,
        metric_value,
        metric_kind: metric_kind.and_then(MetricKind::from_i64),
        metric_unit,
        span_start_ms: span_start_ms.map(|value| value as u64),
        span_end_ms: span_end_ms.map(|value| value as u64),
        span_duration_ms: span_duration_ms.map(|value| value as u64),
        span_status: span_status.and_then(SpanStatus::from_i64),
        attrs_json,
    })
}
