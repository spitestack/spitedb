//! Language-agnostic intermediate representation.
//!
//! This IR is produced by language frontends and consumed by code generators.
//! It represents the domain concepts (aggregates, events, orchestrators, projections) in a
//! way that's independent of the source language.

mod access;
mod aggregate;
mod orchestrator;
mod projection;

pub use access::{AccessLevel, AppConfig, AppMode, EntityAccessConfig, MethodAccessConfig};
pub use aggregate::{
    AggregateIR, CommandIR, EventTypeIR, EventVariant, EventField,
    StatementIR, ExpressionIR, BinaryOp, UnaryOp,
};
pub use orchestrator::{OrchestratorDependency, OrchestratorIR};
pub use projection::{
    ProjectionIR, ProjectionKind, ProjectionSchema, QueryMethodIR,
    SubscribedEvent, ColumnDef, IndexDef, SqlType, StateShape, TimeSeriesSignals,
    is_time_related_name, is_range_param,
    TIME_KEYWORDS, TIMESTAMP_FIELDS, TIME_STRING_METHODS, RANGE_PARAMS,
};

use std::path::PathBuf;

/// Domain types that can be represented in the IR.
#[derive(Debug, Clone, PartialEq)]
pub enum DomainType {
    String,
    Number,
    Boolean,
    Array(Box<DomainType>),
    Option(Box<DomainType>),
    Object(ObjectType),
    Reference(String),
}

/// An object type with named fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectType {
    pub fields: Vec<FieldDef>,
}

/// A field definition.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub typ: DomainType,
    pub optional: bool,
}

/// A parameter to a command or function.
#[derive(Debug, Clone)]
pub struct ParameterIR {
    pub name: String,
    pub typ: DomainType,
}

/// Initial value for state fields.
#[derive(Debug, Clone)]
pub enum InitialValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    EmptyArray,
    EmptyObject,
}

/// The complete domain IR containing all analyzed aggregates, orchestrators, and projections.
#[derive(Debug)]
pub struct DomainIR {
    pub aggregates: Vec<AggregateIR>,
    pub orchestrators: Vec<OrchestratorIR>,
    pub projections: Vec<ProjectionIR>,
    pub source_dir: PathBuf,
    /// App configuration for access control (parsed from index.ts).
    pub app_config: Option<AppConfig>,
}

impl DomainIR {
    pub fn new(source_dir: PathBuf) -> Self {
        Self {
            aggregates: Vec::new(),
            orchestrators: Vec::new(),
            projections: Vec::new(),
            source_dir,
            app_config: None,
        }
    }
}
