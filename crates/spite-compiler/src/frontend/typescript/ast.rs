//! TypeScript-specific AST types.

use std::path::PathBuf;
use crate::diagnostic::Span;

/// A parsed TypeScript file.
#[derive(Debug)]
pub struct ParsedFile {
    pub path: PathBuf,
    pub imports: Vec<ImportDecl>,
    pub type_aliases: Vec<TypeAlias>,
    pub classes: Vec<ClassDecl>,
}

/// An import declaration.
#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub specifiers: Vec<ImportSpecifier>,
    pub source: String,
    pub span: Span,
}

/// An import specifier.
#[derive(Debug, Clone)]
pub struct ImportSpecifier {
    pub name: String,
    pub alias: Option<String>,
}

/// A type alias declaration.
#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub name: String,
    pub type_node: TypeNode,
    pub exported: bool,
    pub span: Span,
}

/// Type AST nodes.
#[derive(Debug, Clone)]
pub enum TypeNode {
    /// string, number, boolean, etc.
    Primitive(String),

    /// T[]
    Array(Box<TypeNode>),

    /// T | U
    Union(Vec<TypeNode>),

    /// { type: "Foo", field: T }
    ObjectLiteral(Vec<ObjectProperty>),

    /// { [key: string]: T } - index signature
    IndexSignature {
        /// The key name (e.g., "userId", "date")
        key_name: String,
        /// The key type (usually string)
        key_type: Box<TypeNode>,
        /// The value type
        value_type: Box<TypeNode>,
    },

    /// Reference to another type
    Reference(String),

    /// T | undefined or T?
    Optional(Box<TypeNode>),
}

/// A property in an object literal type.
#[derive(Debug, Clone)]
pub struct ObjectProperty {
    pub name: String,
    pub type_node: TypeNode,
    pub optional: bool,
}

/// A class declaration.
#[derive(Debug)]
pub struct ClassDecl {
    pub name: String,
    pub properties: Vec<PropertyDecl>,
    pub methods: Vec<MethodDecl>,
    pub exported: bool,
    pub span: Span,
}

/// A class property declaration.
#[derive(Debug, Clone)]
pub struct PropertyDecl {
    pub name: String,
    pub type_node: Option<TypeNode>,
    pub is_static: bool,
    pub is_readonly: bool,
    pub initializer: Option<String>,
    pub span: Span,
}

/// A method declaration.
#[derive(Debug)]
pub struct MethodDecl {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeNode>,
    pub body: Vec<Statement>,
    /// Raw source text of the method body (for TS→TS pass-through like apply methods).
    pub raw_body: Option<String>,
    pub is_async: bool,
    pub visibility: Visibility,
    pub span: Span,
}

/// Method visibility.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

/// A parameter.
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_node: Option<TypeNode>,
    pub optional: bool,
    pub default_value: Option<String>,
}

/// A statement.
#[derive(Debug, Clone)]
pub enum Statement {
    If {
        condition: Expression,
        then_branch: Vec<Statement>,
        else_branch: Option<Vec<Statement>>,
        span: Span,
    },
    Switch {
        discriminant: Expression,
        cases: Vec<SwitchCase>,
        span: Span,
    },
    Return {
        value: Option<Expression>,
        span: Span,
    },
    Throw {
        argument: Expression,
        span: Span,
    },
    Expression {
        expression: Expression,
        span: Span,
    },
    VariableDecl {
        kind: VarKind,
        name: String,
        type_node: Option<TypeNode>,
        initializer: Option<Expression>,
        span: Span,
    },
    Block {
        statements: Vec<Statement>,
        span: Span,
    },
}

/// Variable declaration kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VarKind {
    Const,
    Let,
    Var,
}

/// A switch case.
#[derive(Debug, Clone)]
pub struct SwitchCase {
    pub test: Option<Expression>, // None for default case
    pub consequent: Vec<Statement>,
}

/// An expression.
#[derive(Debug, Clone)]
pub enum Expression {
    Identifier {
        name: String,
        span: Span,
    },
    StringLiteral {
        value: String,
        span: Span,
    },
    NumberLiteral {
        value: f64,
        span: Span,
    },
    BooleanLiteral {
        value: bool,
        span: Span,
    },
    NullLiteral {
        span: Span,
    },
    ArrayLiteral {
        elements: Vec<Expression>,
        span: Span,
    },
    ObjectLiteral {
        properties: Vec<(String, Expression)>,
        span: Span,
    },
    MemberAccess {
        object: Box<Expression>,
        property: String,
        span: Span,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
        span: Span,
    },
    New {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
        span: Span,
    },
    Binary {
        left: Box<Expression>,
        operator: String,
        right: Box<Expression>,
        span: Span,
    },
    Unary {
        operator: String,
        argument: Box<Expression>,
        prefix: bool,
        span: Span,
    },
    This {
        span: Span,
    },
    Await {
        argument: Box<Expression>,
        span: Span,
    },
    Spread {
        argument: Box<Expression>,
        span: Span,
    },
}
