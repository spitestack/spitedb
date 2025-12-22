//! Compiler error types.
#![allow(unused_assignments)]

use std::path::PathBuf;
use miette::Diagnostic;
use thiserror::Error;

/// Errors that can occur during compilation.
#[allow(unused_assignments)]
#[derive(Error, Diagnostic, Debug)]
pub enum CompilerError {
    // =========================================================================
    // IO Errors
    // =========================================================================
    #[error("Failed to read file '{path}': {message}")]
    #[diagnostic(code(spitestack::io::read_error))]
    IoError {
        path: PathBuf,
        message: String,
    },

    // =========================================================================
    // Parse Errors
    // =========================================================================
    #[error("Failed to initialize parser")]
    #[diagnostic(code(spitestack::parse::init_failed))]
    ParserInitFailed,

    #[error("Failed to parse file: {}", path.display())]
    #[diagnostic(code(spitestack::parse::parse_failed))]
    ParseFailed {
        path: PathBuf,
    },

    #[error("Syntax error: {message}")]
    #[diagnostic(code(spitestack::parse::syntax_error))]
    SyntaxError {
        message: String,
        file: PathBuf,
        line: usize,
        column: usize,
    },

    // =========================================================================
    // Structure Errors
    // =========================================================================
    #[error("Aggregate '{aggregate}' is missing required member: {member}")]
    #[diagnostic(
        code(spitestack::structure::missing_member),
        help("Aggregates must have: initialState (static), state, events, emit(), apply()")
    )]
    MissingMember {
        member: String,
        aggregate: String,
    },

    #[error("Event type '{type_name}' must be a discriminated union with 'type' field")]
    #[diagnostic(
        code(spitestack::structure::invalid_event_type),
        help("Events should be defined as: type FooEvent = {{ type: 'Created', ... }} | {{ type: 'Updated', ... }}")
    )]
    InvalidEventType {
        type_name: String,
    },

    #[error("State type '{type_name}' must be an object type")]
    #[diagnostic(code(spitestack::structure::invalid_state_type))]
    InvalidStateType {
        type_name: String,
    },

    // =========================================================================
    // Purity Errors
    // =========================================================================
    #[error("Domain logic cannot use '{name}' - it has side effects")]
    #[diagnostic(
        code(spitestack::purity::forbidden_call),
        help("Domain logic must be pure. Move side effects to adapters.")
    )]
    ForbiddenCall {
        name: String,
        file: PathBuf,
        line: usize,
    },

    #[error("Domain logic cannot use 'await' in aggregates")]
    #[diagnostic(
        code(spitestack::purity::forbidden_await),
        help("Async operations are only allowed in orchestrators. Move async logic to adapters.")
    )]
    ForbiddenAwait {
        file: PathBuf,
        line: usize,
    },

    #[error("Cannot import external package '{package}'")]
    #[diagnostic(
        code(spitestack::purity::forbidden_import),
        help("Only relative imports within the domain directory are allowed.")
    )]
    ForbiddenImport {
        package: String,
        file: PathBuf,
        line: usize,
    },

    // =========================================================================
    // Type Errors
    // =========================================================================
    #[error("Cannot serialize type '{type_desc}' to JSON")]
    #[diagnostic(
        code(spitestack::types::not_serializable),
        help("Event and state types must be JSON-serializable. Avoid functions, symbols, etc.")
    )]
    NotSerializable {
        type_desc: String,
    },

    #[error("Unknown type reference: {name}")]
    #[diagnostic(code(spitestack::types::unknown_reference))]
    UnknownTypeReference {
        name: String,
    },

    // =========================================================================
    // Code Generation Errors
    // =========================================================================
    #[error("Failed to generate Rust code: {message}")]
    #[diagnostic(code(spitestack::codegen::generation_failed))]
    CodegenFailed {
        message: String,
    },

    #[error("Failed to format generated code: {message}")]
    #[diagnostic(code(spitestack::codegen::format_failed))]
    FormatFailed {
        message: String,
    },

    // =========================================================================
    // Analysis Errors
    // =========================================================================
    #[error("No aggregates found in domain directory")]
    #[diagnostic(
        code(spitestack::analysis::no_aggregates),
        help("Create aggregate files in the domain directory following the pattern: domain/Todo/aggregate.ts")
    )]
    NoAggregates,

    #[error("Duplicate aggregate name: {name}")]
    #[diagnostic(code(spitestack::analysis::duplicate_aggregate))]
    DuplicateAggregate {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },

    #[error("Circular import detected")]
    #[diagnostic(
        code(spitestack::analysis::circular_import),
        help("Break the circular dependency by restructuring the imports")
    )]
    CircularImport {
        cycle: Vec<PathBuf>,
    },

    // =========================================================================
    // Frontend Errors
    // =========================================================================
    #[error("Unsupported language: {language}")]
    #[diagnostic(code(spitestack::frontend::unsupported_language))]
    UnsupportedLanguage {
        language: String,
    },
}

impl CompilerError {
    /// Creates an IO error.
    pub fn io(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::IoError {
            path: path.into(),
            message: message.into(),
        }
    }
}
