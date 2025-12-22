//! Diagnostic types for error reporting.

mod error;
mod span;

pub use error::CompilerError;
pub use span::Span;
