//! Language frontends for parsing source code into IR.
//!
//! Each frontend is responsible for:
//! 1. Parsing source files in its language
//! 2. Converting the AST to the common IR
//!
//! This allows the compiler to support multiple source languages
//! while sharing the validation, code generation, and tooling.

pub mod typescript;

use std::path::Path;
use crate::diagnostic::CompilerError;
use crate::ir::DomainIR;

/// Trait for language frontends.
pub trait Frontend {
    /// Returns the language name (e.g., "typescript", "python").
    fn language(&self) -> &str;

    /// Returns file extensions this frontend handles (e.g., ["ts", "tsx"]).
    fn extensions(&self) -> &[&str];

    /// Parses all source files in the given directory and returns IR.
    fn parse_directory(&mut self, dir: &Path) -> Result<DomainIR, CompilerError>;
}

/// Creates a frontend for the given language.
pub fn create_frontend(language: &str) -> Result<Box<dyn Frontend>, CompilerError> {
    match language {
        "typescript" | "ts" => Ok(Box::new(typescript::TypeScriptFrontend::new()?)),
        _ => Err(CompilerError::UnsupportedLanguage {
            language: language.to_string(),
        }),
    }
}
