//! Orchestrator intermediate representation.

use std::path::PathBuf;
use super::ParameterIR;

/// IR representation of an orchestrator.
#[derive(Debug)]
pub struct OrchestratorIR {
    /// Name of the orchestrator.
    pub name: String,

    /// Source file path.
    pub source_path: PathBuf,

    /// Aggregates this orchestrator depends on.
    pub dependencies: Vec<OrchestratorDependency>,

    /// The orchestrate method parameters.
    pub parameters: Vec<ParameterIR>,

    /// Whether the orchestrator is async.
    pub is_async: bool,
}

/// A dependency of an orchestrator.
#[derive(Debug)]
pub struct OrchestratorDependency {
    /// Name of the dependency parameter.
    pub name: String,

    /// Type of the dependency (aggregate name or adapter interface).
    pub typ: String,

    /// Whether this is an optional dependency.
    pub optional: bool,
}
