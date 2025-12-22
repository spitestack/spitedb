use chrono::{NaiveDate, Utc};

use spitedb::telemetry::{TelemetryConfig, TelemetryCursor, TelemetryDB, TelemetryKind, TelemetryQuery, TelemetryRecord};
use spitedb::types::Tenant;

fn ts_ms_from_date(date: NaiveDate) -> u64 {
    date.and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis() as u64
}

#[test]
fn telemetry_write_and_query() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("telemetry");

    let mut config = TelemetryConfig::new("testapp");
    config.batch_max_ms = 1;
    config.partitions = 2;

    let db = TelemetryDB::open(&root, config).unwrap();
    let tenant_hash = Tenant::new("tenant-a").hash();
    let ts_ms = Utc::now().timestamp_millis() as u64;

    db.write(TelemetryRecord::log(ts_ms, tenant_hash, "hello telemetry"))
        .unwrap();
    db.flush().unwrap();

    let mut query = TelemetryQuery::new();
    query.tenant_hash = Some(tenant_hash);
    query.kind = Some(TelemetryKind::Log);
    query.start_ms = Some(ts_ms);
    query.end_ms = Some(ts_ms);

    let results = db.query(query).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].message.as_deref(), Some("hello telemetry"));

    db.shutdown();
}

#[test]
fn telemetry_time_slices_create_directories() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("telemetry");

    let mut config = TelemetryConfig::new("testapp");
    config.batch_max_ms = 1;
    config.partitions = 2;

    let db = TelemetryDB::open(&root, config).unwrap();
    let tenant_hash = Tenant::new("tenant-a").hash();

    let day1 = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let day2 = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();

    db.write(TelemetryRecord::log(ts_ms_from_date(day1), tenant_hash, "day1"))
        .unwrap();
    db.write(TelemetryRecord::log(ts_ms_from_date(day2), tenant_hash, "day2"))
        .unwrap();
    db.flush().unwrap();

    let app_dir = root.join("testapp");
    assert!(app_dir.join("2025-01-01").exists());
    assert!(app_dir.join("2025-01-02").exists());

    db.shutdown();
}

#[test]
fn telemetry_retention_removes_old_slices() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("telemetry");
    let app_dir = root.join("testapp");

    std::fs::create_dir_all(&app_dir).unwrap();

    let today = Utc::now().date_naive();
    let old_slice = today - chrono::Duration::days(45);
    let keep_slice = today - chrono::Duration::days(10);

    std::fs::create_dir_all(app_dir.join(old_slice.format("%Y-%m-%d").to_string())).unwrap();
    std::fs::create_dir_all(app_dir.join(keep_slice.format("%Y-%m-%d").to_string())).unwrap();

    let mut config = TelemetryConfig::new("testapp");
    config.retention_days = 30;
    config.partitions = 1;

    let db = TelemetryDB::open(&root, config).unwrap();
    db.cleanup_retention().unwrap();

    assert!(!app_dir.join(old_slice.format("%Y-%m-%d").to_string()).exists());
    assert!(app_dir.join(keep_slice.format("%Y-%m-%d").to_string()).exists());

    db.shutdown();
}

#[test]
fn telemetry_tail_advances_cursor() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("telemetry");

    let mut config = TelemetryConfig::new("testapp");
    config.batch_max_ms = 1;
    config.partitions = 1;

    let partitions = config.partitions;
    let db = TelemetryDB::open(&root, config).unwrap();
    let tenant_hash = Tenant::new("tenant-a").hash();
    let ts_ms = Utc::now().timestamp_millis() as u64;

    db.write(TelemetryRecord::log(ts_ms, tenant_hash, "first"))
        .unwrap();
    db.write(TelemetryRecord::log(ts_ms + 1, tenant_hash, "second"))
        .unwrap();
    db.flush().unwrap();

    let slice = chrono::DateTime::<Utc>::from_timestamp_millis(ts_ms as i64)
        .unwrap()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();

    let cursor = TelemetryCursor::new(slice, partitions);
    let (records, next_cursor) = db.tail(&cursor, 10).unwrap();

    assert_eq!(records.len(), 2);
    assert!(next_cursor.last_ids.iter().any(|id| *id > 0));

    db.shutdown();
}
