//! # Telemetry Types
//!
//! Domain types for the telemetry store (logs, metrics, spans).
//! These mirror the wide-table schema and provide ergonomic builders.

use std::fmt;

use crate::types::{
    AppendCommand, AppendResult, CommandId, GlobalPos, StreamHash, StreamId, StreamRev, Tenant,
    TenantHash,
};

/// Telemetry record kind (log, metric, span).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TelemetryKind {
    Log,
    Metric,
    Span,
}

impl TelemetryKind {
    pub fn as_i64(self) -> i64 {
        match self {
            TelemetryKind::Log => 0,
            TelemetryKind::Metric => 1,
            TelemetryKind::Span => 2,
        }
    }

    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(TelemetryKind::Log),
            1 => Some(TelemetryKind::Metric),
            2 => Some(TelemetryKind::Span),
            _ => None,
        }
    }
}

/// Metric aggregation semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricKind {
    Gauge,
    Counter,
    Histogram,
    Summary,
}

impl MetricKind {
    pub fn as_i64(self) -> i64 {
        match self {
            MetricKind::Gauge => 0,
            MetricKind::Counter => 1,
            MetricKind::Histogram => 2,
            MetricKind::Summary => 3,
        }
    }

    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(MetricKind::Gauge),
            1 => Some(MetricKind::Counter),
            2 => Some(MetricKind::Histogram),
            3 => Some(MetricKind::Summary),
            _ => None,
        }
    }
}

/// Span status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpanStatus {
    Unset,
    Ok,
    Error,
}

impl SpanStatus {
    pub fn as_i64(self) -> i64 {
        match self {
            SpanStatus::Unset => 0,
            SpanStatus::Ok => 1,
            SpanStatus::Error => 2,
        }
    }

    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(SpanStatus::Unset),
            1 => Some(SpanStatus::Ok),
            2 => Some(SpanStatus::Error),
            _ => None,
        }
    }
}

/// Sort order for queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TelemetryOrder {
    Asc,
    #[default]
    Desc,
}

/// Time-slicing strategy for telemetry shards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeSlice {
    Daily,
}

impl fmt::Display for TimeSlice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeSlice::Daily => write!(f, "daily"),
        }
    }
}

/// Configuration for telemetry storage.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// App name used for directory layout under the telemetry root.
    pub app_name: String,
    /// Number of partitions (shards).
    pub partitions: usize,
    /// Max batch latency in milliseconds.
    pub batch_max_ms: u64,
    /// Max batch size in bytes (approximate).
    pub batch_max_bytes: usize,
    /// Max batch size in records.
    pub batch_max_records: usize,
    /// Maximum in-flight records queued per shard.
    pub max_inflight: usize,
    /// Retention in days (directories older than this are removed).
    pub retention_days: u64,
    /// Time slicing policy.
    pub time_slice: TimeSlice,
    /// Default service name for telemetry records when omitted.
    pub default_service: Option<String>,
}

impl TelemetryConfig {
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            partitions: 8,
            batch_max_ms: 10,
            batch_max_bytes: 256 * 1024,
            batch_max_records: 2_000,
            max_inflight: 50_000,
            retention_days: 30,
            time_slice: TimeSlice::Daily,
            default_service: None,
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self::new("app")
    }
}

/// Correlation reference to a specific event.
#[derive(Debug, Clone)]
pub struct EventRef {
    pub global_pos: GlobalPos,
    pub stream_id: StreamId,
    pub stream_rev: StreamRev,
    pub tenant_hash: TenantHash,
    pub command_id: CommandId,
}

impl EventRef {
    pub fn new(
        global_pos: GlobalPos,
        stream_id: StreamId,
        stream_rev: StreamRev,
        tenant_hash: TenantHash,
        command_id: CommandId,
    ) -> Self {
        Self {
            global_pos,
            stream_id,
            stream_rev,
            tenant_hash,
            command_id,
        }
    }

    pub fn stream_hash(&self) -> StreamHash {
        self.stream_id.hash()
    }

    pub fn from_append(cmd: &AppendCommand, result: &AppendResult) -> Vec<EventRef> {
        let count = result.event_count();
        let mut refs = Vec::with_capacity(count as usize);
        for offset in 0..count {
            refs.push(EventRef::new(
                result.first_pos.add(offset),
                cmd.stream_id.clone(),
                result.first_rev.add(offset),
                cmd.tenant.hash(),
                cmd.command_id.clone(),
            ));
        }
        refs
    }
}

/// A single telemetry record stored in the wide table.
#[derive(Debug, Clone)]
pub struct TelemetryRecord {
    pub id: Option<i64>,
    pub ts_ms: u64,
    pub kind: TelemetryKind,
    pub tenant_hash: TenantHash,

    pub event_global_pos: Option<GlobalPos>,
    pub stream_hash: Option<StreamHash>,
    pub stream_rev: Option<StreamRev>,
    pub command_id: Option<CommandId>,

    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,

    pub name: Option<String>,
    pub service: Option<String>,

    pub severity: Option<i64>,
    pub message: Option<String>,

    pub metric_name: Option<String>,
    pub metric_value: Option<f64>,
    pub metric_kind: Option<MetricKind>,
    pub metric_unit: Option<String>,

    pub span_start_ms: Option<u64>,
    pub span_end_ms: Option<u64>,
    pub span_duration_ms: Option<u64>,
    pub span_status: Option<SpanStatus>,

    pub attrs_json: Option<String>,
}

impl TelemetryRecord {
    pub fn new(kind: TelemetryKind, ts_ms: u64, tenant_hash: TenantHash) -> Self {
        Self {
            id: None,
            ts_ms,
            kind,
            tenant_hash,
            event_global_pos: None,
            stream_hash: None,
            stream_rev: None,
            command_id: None,
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            name: None,
            service: None,
            severity: None,
            message: None,
            metric_name: None,
            metric_value: None,
            metric_kind: None,
            metric_unit: None,
            span_start_ms: None,
            span_end_ms: None,
            span_duration_ms: None,
            span_status: None,
            attrs_json: None,
        }
    }

    pub fn log(ts_ms: u64, tenant_hash: TenantHash, message: impl Into<String>) -> Self {
        let mut record = Self::new(TelemetryKind::Log, ts_ms, tenant_hash);
        record.message = Some(message.into());
        record
    }

    pub fn metric(ts_ms: u64, tenant_hash: TenantHash, name: impl Into<String>, value: f64) -> Self {
        let mut record = Self::new(TelemetryKind::Metric, ts_ms, tenant_hash);
        record.metric_name = Some(name.into());
        record.metric_value = Some(value);
        record
    }

    pub fn span(
        ts_ms: u64,
        tenant_hash: TenantHash,
        name: impl Into<String>,
        start_ms: u64,
        end_ms: u64,
    ) -> Self {
        let mut record = Self::new(TelemetryKind::Span, ts_ms, tenant_hash);
        record.name = Some(name.into());
        record.span_start_ms = Some(start_ms);
        record.span_end_ms = Some(end_ms);
        record.span_duration_ms = Some(end_ms.saturating_sub(start_ms));
        record
    }

    pub fn with_event_ref(mut self, event_ref: &EventRef) -> Self {
        self.event_global_pos = Some(event_ref.global_pos);
        self.stream_hash = Some(event_ref.stream_hash());
        self.stream_rev = Some(event_ref.stream_rev);
        self.command_id = Some(event_ref.command_id.clone());
        self.tenant_hash = event_ref.tenant_hash;
        self
    }

    pub fn with_tenant(mut self, tenant: &Tenant) -> Self {
        self.tenant_hash = tenant.hash();
        self
    }

    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = Some(service.into());
        self
    }

    pub fn with_attrs_json(mut self, json: impl Into<String>) -> Self {
        self.attrs_json = Some(json.into());
        self
    }

    pub fn approx_size_bytes(&self) -> usize {
        let mut size = 0usize;
        size += self.message.as_ref().map(|s| s.len()).unwrap_or(0);
        size += self.metric_name.as_ref().map(|s| s.len()).unwrap_or(0);
        size += self.metric_unit.as_ref().map(|s| s.len()).unwrap_or(0);
        size += self.trace_id.as_ref().map(|s| s.len()).unwrap_or(0);
        size += self.span_id.as_ref().map(|s| s.len()).unwrap_or(0);
        size += self.parent_span_id.as_ref().map(|s| s.len()).unwrap_or(0);
        size += self.name.as_ref().map(|s| s.len()).unwrap_or(0);
        size += self.service.as_ref().map(|s| s.len()).unwrap_or(0);
        size += self.command_id.as_ref().map(|s| s.as_str().len()).unwrap_or(0);
        size += self.attrs_json.as_ref().map(|s| s.len()).unwrap_or(0);
        size + 128
    }
}

/// Query parameters for telemetry searches.
#[derive(Debug, Clone, Default)]
pub struct TelemetryQuery {
    pub tenant_hash: Option<TenantHash>,
    pub kind: Option<TelemetryKind>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub severity: Option<i64>,
    pub metric_name: Option<String>,
    pub event_global_pos: Option<GlobalPos>,
    pub stream_hash: Option<StreamHash>,
    pub stream_rev: Option<StreamRev>,
    pub command_id: Option<CommandId>,
    pub trace_id: Option<String>,
    pub limit: Option<usize>,
    pub order: TelemetryOrder,
    pub slice: Option<String>,
    pub shard_id: Option<usize>,
}

impl TelemetryQuery {
    pub fn new() -> Self {
        Self {
            order: TelemetryOrder::Desc,
            ..Default::default()
        }
    }
}

/// Cursor for tailing a shard.
#[derive(Debug, Clone)]
pub struct TelemetryCursor {
    pub slice: String,
    pub last_ids: Vec<i64>,
}

impl TelemetryCursor {
    pub fn new(slice: impl Into<String>, partitions: usize) -> Self {
        Self {
            slice: slice.into(),
            last_ids: vec![0; partitions],
        }
    }
}
