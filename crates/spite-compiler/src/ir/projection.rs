//! Projection intermediate representation.
//!
//! Projections are read models that subscribe to events and build queryable state.
//! They are detected by type shape and exposed via HTTP GET endpoints.

use std::path::PathBuf;
use super::{AccessLevel, DomainType, ParameterIR};

/// The projection type determines storage and query patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionKind {
    /// SQLite-backed, row-based storage.
    /// State shape: `{ [key: string]: ObjectType }`
    /// Fast lookups by indexed columns.
    DenormalizedView,

    /// Memory-resident with time-based checkpointing.
    /// State shape: `{ field: T, ... }` (named fields)
    /// Fast reads, no DB hit.
    Aggregator,

    /// Time-bucketed data with range queries.
    /// State shape: `{ [key: string]: number }` with time signals.
    /// Detected by: timestamp derivation, time-related key name, or range query methods.
    TimeSeries,
}

impl ProjectionKind {
    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectionKind::DenormalizedView => "denormalized_view",
            ProjectionKind::Aggregator => "aggregator",
            ProjectionKind::TimeSeries => "time_series",
        }
    }
}

/// IR representation of a projection.
#[derive(Debug)]
pub struct ProjectionIR {
    /// Name of the projection (e.g., "UserProfiles", "OrderStats").
    pub name: String,

    /// Source file path.
    pub source_path: PathBuf,

    /// The projection type (determines storage and query patterns).
    pub kind: ProjectionKind,

    /// Events this projection subscribes to (from build() method param union type).
    pub subscribed_events: Vec<SubscribedEvent>,

    /// The state/row schema.
    pub schema: ProjectionSchema,

    /// Query methods that become HTTP GET endpoints.
    pub queries: Vec<QueryMethodIR>,

    /// Raw build method body for codegen pass-through.
    pub raw_build_body: Option<String>,

    /// Access level for this projection's endpoints.
    pub access: AccessLevel,

    /// Required roles to access this projection.
    pub roles: Vec<String>,
}

/// An event the projection subscribes to.
#[derive(Debug, Clone)]
pub struct SubscribedEvent {
    /// Event type name (e.g., "UserCreated", "OrderCompleted").
    pub event_name: String,

    /// Aggregate this event belongs to (derived from naming convention).
    /// e.g., "UserCreated" likely belongs to "User" aggregate.
    pub aggregate: Option<String>,
}

/// Schema for projection state/rows.
#[derive(Debug, Clone)]
pub struct ProjectionSchema {
    /// Name of the state property in the source class.
    pub state_property_name: String,

    /// Primary key column(s).
    pub primary_keys: Vec<ColumnDef>,

    /// Non-key columns.
    pub columns: Vec<ColumnDef>,

    /// Indexes derived from query method parameters.
    pub indexes: Vec<IndexDef>,
}

/// Column definition for SQLite schema.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    /// Column name (snake_case).
    pub name: String,

    /// SQLite type.
    pub sql_type: SqlType,

    /// Whether the column can be NULL.
    pub nullable: bool,

    /// Default value (SQL expression).
    pub default: Option<String>,
}

/// SQLite column types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlType {
    /// TEXT - for strings, JSON serialization
    Text,
    /// INTEGER - for integers, booleans (0/1)
    Integer,
    /// REAL - for floating point numbers
    Real,
    /// BLOB - for binary data
    Blob,
}

impl SqlType {
    /// Convert a domain type to SQLite type.
    pub fn from_domain_type(dt: &DomainType) -> Self {
        match dt {
            DomainType::String => SqlType::Text,
            DomainType::Number => SqlType::Real,
            DomainType::Boolean => SqlType::Integer,
            DomainType::Array(_) => SqlType::Text,    // JSON serialized
            DomainType::Option(inner) => Self::from_domain_type(inner),
            DomainType::Object(_) => SqlType::Text,   // JSON serialized
            DomainType::Reference(_) => SqlType::Text, // Assume string ID
        }
    }

    /// Convert to SQL type string.
    pub fn to_sql(&self) -> &'static str {
        match self {
            SqlType::Text => "TEXT",
            SqlType::Integer => "INTEGER",
            SqlType::Real => "REAL",
            SqlType::Blob => "BLOB",
        }
    }
}

/// Index definition.
#[derive(Debug, Clone)]
pub struct IndexDef {
    /// Index name.
    pub name: String,

    /// Columns in the index.
    pub columns: Vec<String>,

    /// Whether the index enforces uniqueness.
    pub unique: bool,
}

/// IR representation of a query method.
#[derive(Debug, Clone)]
pub struct QueryMethodIR {
    /// Method name (e.g., "getById", "getByEmail").
    pub name: String,

    /// Parameters to the query.
    pub parameters: Vec<ParameterIR>,

    /// Return type of the query.
    pub return_type: Option<DomainType>,

    /// Columns that need indexes (derived from parameter names).
    pub indexed_columns: Vec<String>,

    /// Whether this is a range query (has start/end parameters).
    pub is_range_query: bool,

    /// Raw method body for codegen pass-through.
    pub raw_body: Option<String>,
}

/// Analyzed state shape for projection kind detection.
#[derive(Debug, Clone)]
pub enum StateShape {
    /// Index signature with object value: `{ [key: string]: ObjectType }`
    IndexedObject {
        /// The key name from the index signature (e.g., "userId").
        key_name: String,
        /// The value type.
        value_type: DomainType,
    },

    /// Index signature with number value: `{ [key: string]: number }`
    IndexedNumber {
        /// The key name from the index signature (e.g., "date", "category").
        key_name: String,
    },

    /// Named fields (no top-level index signature): `{ field: T, ... }`
    NamedFields {
        /// The field definitions.
        fields: Vec<(String, DomainType)>,
    },
}

/// Time-series detection signals.
#[derive(Debug, Clone, Default)]
pub struct TimeSeriesSignals {
    /// Signal 1: Key is derived from timestamp field in build() method.
    pub has_timestamp_derivation: bool,

    /// Signal 2: Key name contains time-related words.
    pub has_time_related_key_name: bool,

    /// Signal 3: Has query methods with range parameters.
    pub has_range_query_methods: bool,
}

impl TimeSeriesSignals {
    /// Returns true if any time-series signal is present.
    pub fn any(&self) -> bool {
        self.has_timestamp_derivation
            || self.has_time_related_key_name
            || self.has_range_query_methods
    }
}

/// Time-related keywords for detecting Time-Series projections.
pub const TIME_KEYWORDS: &[&str] = &[
    "date", "day", "month", "year", "week", "hour",
    "minute", "time", "timestamp", "period",
];

/// Timestamp field names for detecting timestamp derivation.
pub const TIMESTAMP_FIELDS: &[&str] = &[
    "timestamp", "date", "time", "createdAt", "updatedAt",
    "occurredAt", "eventTime", "eventDate",
];

/// String methods used to derive time keys from timestamps.
pub const TIME_STRING_METHODS: &[&str] = &[
    "slice", "substring", "substr", "toISOString", "toDateString",
    "toLocaleDateString", "toTimeString",
];

/// Range parameter names for detecting Time-Series projections.
pub const RANGE_PARAMS: &[&str] = &[
    "start", "end", "from", "to", "startdate", "enddate",
    "fromdate", "todate", "starttime", "endtime",
];

/// Check if a name contains time-related keywords.
pub fn is_time_related_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    TIME_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Check if a parameter name suggests a range query.
pub fn is_range_param(name: &str) -> bool {
    let lower = name.to_lowercase();
    RANGE_PARAMS.iter().any(|rp| lower.contains(rp))
}
