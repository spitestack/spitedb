//! # Telemetry Writer
//!
//! Background writer threads for telemetry shards with batched inserts.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rusqlite::{params, Connection};

use crate::{Error, Result};

use super::schema::init_schema;
use super::{slice_from_ts_ms, TelemetryConfig, TelemetryRecord};

#[derive(Debug)]
pub struct TelemetryWriterHandle {
    tx: SyncSender<WriterMessage>,
    join: Option<JoinHandle<()>>,
}

impl TelemetryWriterHandle {
    pub fn write(&self, record: TelemetryRecord) -> Result<()> {
        self.tx
            .try_send(WriterMessage::Write {
                record: Box::new(record),
            })
            .map_err(map_queue_error)?;
        Ok(())
    }

    pub fn write_batch(&self, records: Vec<TelemetryRecord>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        self.tx
            .try_send(WriterMessage::WriteBatch { records })
            .map_err(map_queue_error)?;
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(WriterMessage::Flush { response: response_tx })
            .map_err(|_| Error::Schema("telemetry writer channel closed".to_string()))?;
        response_rx
            .recv()
            .map_err(|_| Error::Schema("telemetry writer dropped response".to_string()))?
    }

    pub fn shutdown(mut self) {
        let _ = self.tx.send(WriterMessage::Shutdown);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

enum WriterMessage {
    Write { record: Box<TelemetryRecord> },
    WriteBatch { records: Vec<TelemetryRecord> },
    Flush {
        response: mpsc::Sender<Result<()>>,
    },
    Shutdown,
}

struct WriterState {
    batch: Vec<TelemetryRecord>,
    batch_bytes: usize,
    batch_records: usize,
    current_slice: Option<String>,
    conn: Option<Connection>,
}

impl WriterState {
    fn new() -> Self {
        Self {
            batch: Vec::new(),
            batch_bytes: 0,
            batch_records: 0,
            current_slice: None,
            conn: None,
        }
    }
}

pub fn spawn_writer(
    app_dir: PathBuf,
    shard_id: usize,
    config: TelemetryConfig,
) -> TelemetryWriterHandle {
    let (tx, rx) = mpsc::sync_channel(config.max_inflight);
    let handle = thread::Builder::new()
        .name(format!("spitedb-telemetry-writer-{}", shard_id))
        .spawn(move || writer_loop(rx, app_dir, shard_id, config))
        .expect("failed to spawn telemetry writer thread");

    TelemetryWriterHandle {
        tx,
        join: Some(handle),
    }
}

fn writer_loop(rx: Receiver<WriterMessage>, app_dir: PathBuf, shard_id: usize, config: TelemetryConfig) {
    let flush_interval = Duration::from_millis(config.batch_max_ms);
    let mut last_flush = Instant::now();
    let mut state = WriterState::new();

    loop {
        let timeout = flush_interval.saturating_sub(last_flush.elapsed());
        match rx.recv_timeout(timeout) {
            Ok(message) => match message {
                WriterMessage::Write { record } => {
                    let _ = enqueue_record(*record, &mut state, &app_dir, shard_id, &config);

                    if should_flush(state.batch_bytes, state.batch_records, &config) {
                        flush_batch(&mut state, &app_dir, shard_id, &config);
                        last_flush = Instant::now();
                    }
                }
                WriterMessage::WriteBatch { records } => {
                    for record in records {
                        let _ = enqueue_record(record, &mut state, &app_dir, shard_id, &config);
                    }

                    if should_flush(state.batch_bytes, state.batch_records, &config) {
                        flush_batch(&mut state, &app_dir, shard_id, &config);
                        last_flush = Instant::now();
                    }
                }
                WriterMessage::Flush { response } => {
                    flush_batch(&mut state, &app_dir, shard_id, &config);
                    let _ = response.send(Ok(()));
                    last_flush = Instant::now();
                }
                WriterMessage::Shutdown => {
                    flush_batch(&mut state, &app_dir, shard_id, &config);
                    break;
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !state.batch.is_empty() {
                    flush_batch(&mut state, &app_dir, shard_id, &config);
                }
                last_flush = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn enqueue_record(
    record: TelemetryRecord,
    state: &mut WriterState,
    app_dir: &Path,
    shard_id: usize,
    config: &TelemetryConfig,
) -> Result<()> {
    let slice = slice_from_ts_ms(record.ts_ms)?;
    let should_switch = state
        .current_slice
        .as_deref()
        .map(|s| s != slice)
        .unwrap_or(true);

    if should_switch && !state.batch.is_empty() {
        flush_batch(state, app_dir, shard_id, config);
    }

    if should_switch {
        state.current_slice = Some(slice.clone());
        state.conn = Some(open_connection(app_dir, &slice, shard_id)?);
    }

    state.batch_bytes += record.approx_size_bytes();
    state.batch.push(record);
    state.batch_records += 1;
    Ok(())
}

fn flush_batch(
    state: &mut WriterState,
    app_dir: &Path,
    shard_id: usize,
    config: &TelemetryConfig,
) {
    if state.batch.is_empty() {
        return;
    }

    let slice = match state.current_slice.as_deref() {
        Some(slice) => slice.to_string(),
        None => match slice_from_ts_ms(state.batch[0].ts_ms) {
            Ok(slice) => slice,
            Err(err) => {
                let _ = err;
                state.batch.clear();
                state.batch_bytes = 0;
                state.batch_records = 0;
                return;
            }
        },
    };

    if state.conn.is_none() {
        match open_connection(app_dir, &slice, shard_id) {
            Ok(opened) => state.conn = Some(opened),
            Err(err) => {
                let _ = err;
                state.batch.clear();
                state.batch_bytes = 0;
                state.batch_records = 0;
                return;
            }
        }
    }

    let result = insert_batch(state.conn.as_mut().unwrap(), &state.batch, config);
    let _ = result;

    state.batch.clear();
    state.batch_bytes = 0;
    state.batch_records = 0;
    state.current_slice = Some(slice);
}

fn open_connection(app_dir: &Path, slice: &str, shard_id: usize) -> Result<Connection> {
    let shard_path = shard_path(app_dir, slice, shard_id);
    if let Some(parent) = shard_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            Error::Schema(format!("failed to create telemetry shard directory: {}", err))
        })?;
    }
    let conn = Connection::open(shard_path)?;
    init_schema(&conn)?;
    Ok(conn)
}

fn shard_path(app_dir: &Path, slice: &str, shard_id: usize) -> PathBuf {
    app_dir
        .join(slice)
        .join(format!("shard-{:03}.db", shard_id))
}

fn insert_batch(conn: &mut Connection, batch: &[TelemetryRecord], config: &TelemetryConfig) -> Result<()> {
    conn.execute_batch("BEGIN")?;

    let mut stmt = conn.prepare(
        "INSERT INTO telemetry (
            ts_ms,
            kind,
            tenant_hash,
            event_global_pos,
            stream_hash,
            stream_rev,
            command_id,
            trace_id,
            span_id,
            parent_span_id,
            name,
            service,
            severity,
            message,
            metric_name,
            metric_value,
            metric_kind,
            metric_unit,
            span_start_ms,
            span_end_ms,
            span_duration_ms,
            span_status,
            attrs_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )?;

    for record in batch {
        let service = record
            .service
            .as_ref()
            .or(config.default_service.as_ref());
        let span_duration = record.span_duration_ms.or_else(|| {
            match (record.span_start_ms, record.span_end_ms) {
                (Some(start), Some(end)) => Some(end.saturating_sub(start)),
                _ => None,
            }
        });

        stmt.execute(params![
            record.ts_ms as i64,
            record.kind.as_i64(),
            record.tenant_hash.as_raw(),
            record.event_global_pos.map(|pos| pos.as_raw() as i64),
            record.stream_hash.map(|hash| hash.as_raw()),
            record.stream_rev.map(|rev| rev.as_raw() as i64),
            record.command_id.as_ref().map(|id| id.as_str()),
            record.trace_id.as_deref(),
            record.span_id.as_deref(),
            record.parent_span_id.as_deref(),
            record.name.as_deref(),
            service.map(|s| s.as_str()),
            record.severity,
            record.message.as_deref(),
            record.metric_name.as_deref(),
            record.metric_value,
            record.metric_kind.map(|kind| kind.as_i64()),
            record.metric_unit.as_deref(),
            record.span_start_ms.map(|value| value as i64),
            record.span_end_ms.map(|value| value as i64),
            span_duration.map(|value| value as i64),
            record.span_status.map(|status| status.as_i64()),
            record.attrs_json.as_deref(),
        ])?;
    }

    conn.execute_batch("COMMIT")?;
    Ok(())
}

fn should_flush(batch_bytes: usize, batch_records: usize, config: &TelemetryConfig) -> bool {
    batch_bytes >= config.batch_max_bytes || batch_records >= config.batch_max_records
}

fn map_queue_error(err: mpsc::TrySendError<WriterMessage>) -> Error {
    match err {
        mpsc::TrySendError::Full(_) => {
            Error::Timeout("telemetry queue full".to_string())
        }
        mpsc::TrySendError::Disconnected(_) => {
            Error::Schema("telemetry writer channel closed".to_string())
        }
    }
}
