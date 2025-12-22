//! Language-agnostic intermediate representation.
//!
//! This IR is produced by language frontends and consumed by code generators.
//! It represents the domain concepts (aggregates, events, orchestrators) in a
//! way that's independent of the source language.

mod aggregate;
mod orchestrator;

pub use aggregate::{
    AggregateIR, CommandIR, EventTypeIR, EventVariant, EventField,
    StatementIR, ExpressionIR, BinaryOp, UnaryOp,
};
pub use orchestrator::{OrchestratorDependency, OrchestratorIR};

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

/// The complete domain IR containing all analyzed aggregates and orchestrators.
#[derive(Debug)]
pub struct DomainIR {
    pub aggregates: Vec<AggregateIR>,
    pub orchestrators: Vec<OrchestratorIR>,
    pub source_dir: PathBuf,
}

impl DomainIR {
    pub fn new(source_dir: PathBuf) -> Self {
        Self {
            aggregates: Vec::new(),
            orchestrators: Vec::new(),
            source_dir,
        }
    }
}
