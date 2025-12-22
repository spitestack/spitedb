//! Compiler configuration.

use std::path::PathBuf;

/// Configuration for the SpiteStack compiler.
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// Directory containing domain source files.
    pub domain_dir: PathBuf,

    /// Directory to write generated Rust code.
    pub out_dir: PathBuf,

    /// Skip purity checks (for testing).
    pub skip_purity_check: bool,

    /// Source language (default: "typescript").
    pub language: String,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            domain_dir: PathBuf::from("domain"),
            out_dir: PathBuf::from("src/generated"),
            skip_purity_check: false,
            language: "typescript".to_string(),
        }
    }
}
