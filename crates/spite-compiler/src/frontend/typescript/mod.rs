//! TypeScript frontend for the SpiteStack compiler.

pub mod ast;
pub mod parser;
pub mod to_ir;

use std::path::Path;
use walkdir::WalkDir;

use crate::diagnostic::CompilerError;
use crate::ir::DomainIR;
use super::Frontend;
use parser::TypeScriptParser;

/// TypeScript frontend implementation.
pub struct TypeScriptFrontend {
    parser: TypeScriptParser,
}

impl TypeScriptFrontend {
    /// Creates a new TypeScript frontend.
    pub fn new() -> Result<Self, CompilerError> {
        Ok(Self {
            parser: TypeScriptParser::new()?,
        })
    }
}

impl Frontend for TypeScriptFrontend {
    fn language(&self) -> &str {
        "typescript"
    }

    fn extensions(&self) -> &[&str] {
        &["ts", "tsx"]
    }

    fn parse_directory(&mut self, dir: &Path) -> Result<DomainIR, CompilerError> {
        let mut parsed_files = Vec::new();

        // Discover TypeScript files
        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy();
                    if self.extensions().contains(&ext_str.as_ref())
                        && !path.to_string_lossy().ends_with(".d.ts")
                    {
                        let source = std::fs::read_to_string(path).map_err(|e| {
                            CompilerError::IoError {
                                path: path.to_path_buf(),
                                message: e.to_string(),
                            }
                        })?;

                        let parsed = self.parser.parse(&source, path)?;
                        parsed_files.push(parsed);
                    }
                }
            }
        }

        // Convert to IR
        to_ir::to_ir(&parsed_files, dir.to_path_buf())
    }
}
