//! Aggregate intermediate representation.

use std::path::PathBuf;
use super::{AccessLevel, DomainType, ObjectType, ParameterIR, InitialValue};

/// IR representation of an aggregate.
#[derive(Debug)]
pub struct AggregateIR {
    /// Name of the aggregate (e.g., "Todo", "Project").
    pub name: String,

    /// Source file path.
    pub source_path: PathBuf,

    /// The state type.
    pub state: ObjectType,

    /// Initial values for state fields.
    pub initial_state: Vec<(String, InitialValue)>,

    /// The event type (discriminated union).
    pub events: EventTypeIR,

    /// Commands that can be executed on this aggregate.
    pub commands: Vec<CommandIR>,

    /// Raw apply method body (switch statement content) from source.
    /// If present, used directly in codegen instead of auto-generating field mapping.
    pub raw_apply_body: Option<String>,
}

/// IR representation of an event type (discriminated union).
#[derive(Debug)]
pub struct EventTypeIR {
    /// Name of the event type (e.g., "TodoEvent").
    pub name: String,

    /// Event variants.
    pub variants: Vec<EventVariant>,
}

/// A single event variant.
#[derive(Debug, Clone)]
pub struct EventVariant {
    /// The discriminant value (e.g., "Created", "Completed").
    pub name: String,

    /// Fields in this event variant.
    pub fields: Vec<EventField>,
}

/// A field in an event variant.
#[derive(Debug, Clone)]
pub struct EventField {
    pub name: String,
    pub typ: DomainType,
}

/// IR representation of a command method.
#[derive(Debug)]
pub struct CommandIR {
    /// Name of the command (e.g., "create", "complete").
    pub name: String,

    /// Parameters to the command.
    pub parameters: Vec<ParameterIR>,

    /// The body statements (for translation to Rust).
    pub body: Vec<StatementIR>,

    /// Access level for this command endpoint.
    pub access: AccessLevel,

    /// Required roles to access this command.
    /// Only applicable for `Internal` and `Private` access levels.
    pub roles: Vec<String>,
}

/// IR representation of a statement.
#[derive(Debug, Clone)]
pub enum StatementIR {
    /// if (condition) { then_branch } else { else_branch }
    If {
        condition: ExpressionIR,
        then_branch: Vec<StatementIR>,
        else_branch: Option<Vec<StatementIR>>,
    },

    /// throw new Error("message")
    Throw {
        message: String,
    },

    /// this.emit({ type: "...", ... })
    Emit {
        event_type: String,
        fields: Vec<(String, ExpressionIR)>,
    },

    /// Variable declaration: const/let name = value
    Let {
        name: String,
        value: ExpressionIR,
    },

    /// Expression statement
    Expression(ExpressionIR),

    /// Return statement
    Return(Option<ExpressionIR>),
}

/// IR representation of an expression.
#[derive(Debug, Clone)]
pub enum ExpressionIR {
    /// String literal
    StringLiteral(String),

    /// Number literal
    NumberLiteral(f64),

    /// Boolean literal
    BooleanLiteral(bool),

    /// Identifier reference
    Identifier(String),

    /// this.state.field
    StateAccess(String),

    /// Property access: obj.field
    PropertyAccess {
        object: Box<ExpressionIR>,
        property: String,
    },

    /// Method call: obj.method(args)
    MethodCall {
        object: Box<ExpressionIR>,
        method: String,
        arguments: Vec<ExpressionIR>,
    },

    /// Function call: func(args)
    Call {
        callee: String,
        arguments: Vec<ExpressionIR>,
    },

    /// New expression: new Type(args)
    New {
        callee: String,
        arguments: Vec<ExpressionIR>,
    },

    /// Binary operation: left op right
    Binary {
        left: Box<ExpressionIR>,
        operator: BinaryOp,
        right: Box<ExpressionIR>,
    },

    /// Unary operation: op operand
    Unary {
        operator: UnaryOp,
        operand: Box<ExpressionIR>,
    },

    /// Object literal: { field: value, ... }
    Object(Vec<(String, ExpressionIR)>),

    /// Array literal: [...]
    Array(Vec<ExpressionIR>),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Eq,      // ===
    NotEq,   // !==
    Lt,      // <
    LtEq,    // <=
    Gt,      // >
    GtEq,    // >=
    And,     // &&
    Or,      // ||
    Add,     // +
    Sub,     // -
    Mul,     // *
    Div,     // /
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Not,     // !
    Neg,     // -
}
