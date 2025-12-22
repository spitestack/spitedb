# SpiteDB Telemetry Extension Spec (Draft)

Status: Draft

This document defines the telemetry extension for SpiteDB. It introduces a
dedicated telemetry store (logs, metrics, traces) that is correlated to the
event store but does not share its strict global ordering requirements.

## Goals

- Store logs, metrics, and traces with simple querying and streaming.
- Preserve correlation with events (global_pos, stream_id, command_id).
- Maximize write throughput via batching and partitioned SQLite files.
- Zero-config defaults (developer joy first).
- Keep event storage isolated from telemetry for contention and future sharding.

## Non-goals

- Full OpenTelemetry compatibility in v1 (we can map to it later).
- Distributed query engine or cross-node federation.
- Strong global ordering for telemetry.

## Storage Layout

SpiteStack keeps event and telemetry data in separate folders so they can be
sharded across volumes later:

```
/data
  /events
    app.db
  /telemetry
    app/
      2025-01-01/
        shard-000.db
        shard-001.db
        ...
        shard-007.db
      2025-01-02/
        shard-000.db
        shard-001.db
        ...
        shard-007.db
```

Notes:
- The event store still uses a single SQLite file.
- Telemetry uses multiple SQLite files (partitions) with identical schema.
- Telemetry shards are grouped by day (UTC) for retention and pruning.
- File locations are configurable, but the above is the default convention.

## Partitioning

Default partitions: **8**.
Time slicing: **daily (UTC)**.

Partition routing is deterministic to keep related data on one shard:

1. If `trace_id` is present, hash `trace_id`.
2. Else if `stream_id` is present, hash `stream_id`.
3. Else if `command_id` is present, hash `command_id`.
4. Else hash `tenant_hash`.

`partition = xxh3_64(key) % partitions`
`slice = YYYY-MM-DD from ts_ms`

Shard files live under the slice directory, e.g. `telemetry/app/2025-01-02/shard-003.db`.

Rationale:
- Traces stay on a single shard for efficient lookup.
- Event-correlated telemetry is colocated by stream/command.
- Tenant-only telemetry still spreads across shards.

If none of the fields above are provided, fall back to hashing the
`ts_ms` plus a monotonic counter (per process) to spread writes.

## Schema (Single Wide Table)

Each shard contains the same tables:

```
CREATE TABLE IF NOT EXISTS telemetry (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    ts_ms            INTEGER NOT NULL,
    kind             INTEGER NOT NULL,  -- 0=log, 1=metric, 2=span
    tenant_hash      INTEGER NOT NULL,

    -- correlation to events
    event_global_pos INTEGER,
    stream_hash      INTEGER,
    stream_rev       INTEGER,
    command_id       TEXT,

    -- tracing
    trace_id         TEXT,
    span_id          TEXT,
    parent_span_id   TEXT,

    -- common names (service defaults to app name if omitted)
    name             TEXT,
    service          TEXT,

    -- logs
    severity         INTEGER,
    message          TEXT,

    -- metrics
    metric_name      TEXT,
    metric_value     REAL,
    metric_kind      INTEGER, -- 0=gauge, 1=counter, 2=histogram, 3=summary
    metric_unit      TEXT,

    -- spans
    span_start_ms    INTEGER,
    span_end_ms      INTEGER,
    span_duration_ms INTEGER,
    span_status      INTEGER, -- 0=unset, 1=ok, 2=error

    -- flexible attributes (JSON)
    attrs_json       TEXT
);
```

Indexes:

```
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
```

Schema metadata:

```
CREATE TABLE IF NOT EXISTS telemetry_metadata (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Each shard stores its own metadata with `schema_version`.

## Write Path

- One writer thread per shard.
- Aggressive batching with a flush interval (default ~10ms) and max bytes.
- `synchronous = NORMAL` and `journal_mode = WAL` for telemetry shards.
- Batching groups inserts in a single transaction; telemetry rows are not
  compressed in v1 (JSON stored as TEXT).
- Writes are append-only; no tombstones or compaction in the hot path.
- Shards roll daily based on `ts_ms` (UTC).
- Writes are fire-and-forget at the API boundary; enqueueing is fast and does
  not wait for SQLite commit.

Ordering:
- `id` is monotonically increasing per shard.
- There is no global ordering across shards.

## Read Path

Simple queries:
- Filter by `tenant_hash`, `kind`, `ts_ms` range, `severity`, `metric_name`.
- Correlate by `event_global_pos`, `stream_hash + stream_rev`, or `command_id`.

Streaming:
- `tail(cursor, limit)` long-polls all shards in the slice and merges by `ts_ms`.
- Cursor tracks `slice` plus `last_ids` per shard.

## Correlation Model

Telemetry should reference events whenever possible:

- `event_global_pos` when the exact event position is known.
- `stream_hash + stream_rev` for stream-local correlation.
- `command_id` for pre-append or grouped operations.

The event writer exposes an `EventRef` derived from append results:

```
EventRef {
  global_pos,
  stream_id,
  stream_rev,
  tenant_hash,
  command_id,
}
```

Clients may attach this reference to telemetry records.

## API Surface (Rust)

Proposed new public types:

- `TelemetryDB`
- `TelemetryConfig`
- `TelemetryRecord`
- `TelemetryKind`
- `TelemetryQuery`
- `TelemetryCursor`
- `EventRef`

Key methods:

- `TelemetryDB::open(base_dir, TelemetryConfig)`
- `TelemetryDB::write(record)`
- `TelemetryDB::write_batch(records)`
- `TelemetryDB::query(query) -> Vec<TelemetryRecord>`
- `TelemetryDB::tail(cursor, limit) -> (Vec<TelemetryRecord>, TelemetryCursor)`

Integration helpers:

- `SpiteDB::open_with_telemetry(events_path, telemetry_root)`
- `AppendResult::to_event_refs()` or `EventRef::from_append(...)`

Default config:
- `partitions = 8`
- `batch_max_ms = 10`
- `batch_max_bytes = 256 * 1024`
- `batch_max_records = 2000`
- `max_inflight = 50_000`
- `retention_days = 30`
- `time_slice = daily`

## Retention

Retention is handled by dropping old time-sliced directories.
Default retention is **30 days**. No synchronous deletes in the hot path.

## Testing

Minimum required tests:

- Ingest and query logs/metrics/spans by kind and time range.
- Correlation by `event_global_pos`, `stream_hash + stream_rev`, and `command_id`.
- Deterministic partition routing for the same key.
- Time-slice routing by `ts_ms` (daily rollover).
- Tail cursor yields no duplicates or gaps within a shard.
- Retention cleanup leaves event data untouched.

## Open Questions

None for v1.
