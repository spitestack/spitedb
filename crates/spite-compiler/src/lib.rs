//! # SpiteStack Compiler
//!
//! This crate provides a compiler for domain logic that generates TypeScript code
//! with Bun HTTP handlers. It supports multiple source languages through
//! a pluggable frontend architecture.
//!
//! ## Supported Languages
//!
//! - TypeScript (default)
//!
//! ## Architecture
//!
//! ```text
//! Source Code (TS, etc.)
//!        │
//!        ▼
//! ┌──────────────┐
//! │   Frontend   │  Language-specific parsing
//! │  (TS → AST)  │
//! └──────┬───────┘
//!        │
//!        ▼
//! ┌──────────────┐
//! │      IR      │  Language-agnostic representation
//! │  (AST → IR)  │
//! └──────┬───────┘
//!        │
//!        ▼
//! ┌──────────────┐
//! │   Validate   │  Purity & structure checks
//! │   (IR)       │
//! └──────┬───────┘
//!        │
//!        ▼
//! ┌──────────────┐
//! │   Codegen    │  Generate TypeScript + Bun.serve
//! │  (IR → TS)   │
//! └──────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use spite_compiler::{Compiler, CompilerConfig};
//!
//! let config = CompilerConfig {
//!     domain_dir: "domain".into(),
//!     out_dir: "src/generated".into(),
//!     skip_purity_check: false,
//!     language: "typescript".to_string(),
//! };
//!
//! let compiler = Compiler::new(config);
//! compiler.compile().await?;
//! ```

pub mod config;
pub mod frontend;
pub mod ir;
pub mod validate;
pub mod codegen;
pub mod diagnostic;
pub mod schema;

use std::path::PathBuf;

pub use config::CompilerConfig;
pub use diagnostic::CompilerError;
pub use codegen::project;

/// The main compiler struct that orchestrates the compilation pipeline.
pub struct Compiler {
    config: CompilerConfig,
}

/// Configuration for dev mode.
#[derive(Debug, Clone)]
pub struct DevConfig {
    /// Domain source directory
    pub domain_dir: PathBuf,
    /// Output directory (.spitestack by default)
    pub output_dir: PathBuf,
    /// Source language
    pub language: String,
    /// Port for the dev server
    pub port: u16,
}

impl Compiler {
    /// Creates a new compiler with the given configuration.
    pub fn new(config: CompilerConfig) -> Self {
        Self { config }
    }

    /// Compiles source domain logic to TypeScript code.
    ///
    /// This runs the full pipeline:
    /// 1. Create frontend for the configured language
    /// 2. Parse source files into IR
    /// 3. Parse App configuration from index.ts
    /// 4. Apply access configuration to IR
    /// 5. Validate IR for purity and structure
    /// 6. Generate TypeScript code
    /// 7. Write output files
    pub async fn compile(&self) -> Result<CompileResult, CompilerError> {
        // Phase 1: Create frontend
        let mut frontend = frontend::create_frontend(&self.config.language)?;

        // Phase 2: Parse files into IR
        let mut domain_ir = frontend.parse_directory(&self.config.domain_dir)?;

        // Phase 3: Parse App configuration from index.ts
        let app_config = frontend::typescript::app_parser::parse_app_config(&self.config.domain_dir)?;

        // Phase 4: Apply access configuration
        if let Some(ref config) = app_config {
            frontend::typescript::to_ir::apply_access_config(&mut domain_ir, config);
            domain_ir.app_config = Some(config.clone());
        }

        // Phase 5: Schema evolution check (production mode only)
        self.check_schema_evolution(&domain_ir, &app_config)?;

        // Phase 6: Validate
        if !self.config.skip_purity_check {
            validate::validate_domain(&domain_ir)?;
        }

        // Phase 7: Generate TypeScript code
        // Compute import path from handlers/ to domain source
        let domain_import_path = self.compute_domain_import_path()?;
        let generated = codegen::generate(&domain_ir, &domain_import_path)?;

        // Phase 5: Write output
        self.write_output(&generated)?;

        Ok(CompileResult {
            aggregates: domain_ir.aggregates.len(),
            orchestrators: domain_ir.orchestrators.len(),
            events: domain_ir
                .aggregates
                .iter()
                .map(|a| a.events.variants.len())
                .sum(),
        })
    }

    /// Validates source domain logic without generating code.
    pub async fn check(&self) -> Result<(), CompilerError> {
        let mut frontend = frontend::create_frontend(&self.config.language)?;
        let domain_ir = frontend.parse_directory(&self.config.domain_dir)?;
        validate::validate_domain(&domain_ir)?;
        Ok(())
    }

    /// Check schema evolution in production mode.
    ///
    /// In production mode:
    /// - Loads the existing events.lock.json file
    /// - Compares current schemas against the lock file
    /// - Rejects breaking changes with helpful errors
    /// - Allows non-breaking changes (logged as warnings)
    /// - If no lock file exists, generates one
    fn check_schema_evolution(
        &self,
        domain: &ir::DomainIR,
        app_config: &Option<ir::AppConfig>,
    ) -> Result<(), CompilerError> {
        use ir::AppMode;

        // Only check in production mode
        let mode = app_config
            .as_ref()
            .map(|c| c.mode)
            .unwrap_or(AppMode::Greenfield);

        if mode != AppMode::Production {
            return Ok(());
        }

        // Lock file is at project root (parent of domain dir typically)
        let lock_path = self.config.domain_dir.parent()
            .unwrap_or(&self.config.domain_dir)
            .join("events.lock.json");

        // Load existing lock file
        let existing_lock = schema::SchemaLockFile::load(&lock_path)?;

        match existing_lock {
            None => {
                // No lock file exists - generate initial one
                let lock = schema::SchemaLockFile::from_domain_ir(domain, env!("CARGO_PKG_VERSION"));
                lock.save(&lock_path)?;
                eprintln!(
                    "📋 Generated initial schema lock file: {}",
                    lock_path.display()
                );
                Ok(())
            }
            Some(locked) => {
                // Compare schemas
                let diffs = schema::diff_schemas(&locked.aggregates, domain);

                if diffs.is_empty() {
                    // No changes - all good
                    return Ok(());
                }

                // Check for breaking changes
                let breaking_diffs: Vec<_> = diffs.iter().filter(|d| d.is_breaking()).collect();

                if !breaking_diffs.is_empty() {
                    // Report the first breaking change (could report all, but one is clearer)
                    let first = &breaking_diffs[0];
                    return Err(CompilerError::BreakingSchemaChange {
                        aggregate: first.aggregate.clone(),
                        event: first.event.clone(),
                        changes: first.format_changes(),
                    });
                }

                // Non-breaking changes - update lock file and generate upcasts
                let non_breaking: Vec<_> = diffs.iter().filter(|d| d.can_auto_upcast()).collect();
                if !non_breaking.is_empty() {
                    eprintln!("📝 Non-breaking schema changes detected:");
                    for diff in &non_breaking {
                        eprintln!("   {}.{}", diff.aggregate, diff.event);
                        eprintln!("{}", diff.format_changes());
                    }

                    // Generate upcast files for aggregates with changes
                    self.generate_upcasts(&diffs, &locked)?;

                    // Update the lock file with new versions
                    let updated_lock = schema::SchemaLockFile::from_domain_ir(domain, env!("CARGO_PKG_VERSION"));
                    updated_lock.save(&lock_path)?;
                    eprintln!("   Updated events.lock.json with new schema versions");
                }

                Ok(())
            }
        }
    }

    /// Generate upcast TypeScript files for schema changes.
    fn generate_upcasts(
        &self,
        diffs: &[schema::SchemaDiff],
        locked: &schema::SchemaLockFile,
    ) -> Result<(), CompilerError> {
        use std::collections::HashMap;

        // Group diffs by aggregate
        let mut by_aggregate: HashMap<&str, Vec<&schema::SchemaDiff>> = HashMap::new();
        for diff in diffs {
            if diff.can_auto_upcast() {
                by_aggregate
                    .entry(&diff.aggregate)
                    .or_default()
                    .push(diff);
            }
        }

        if by_aggregate.is_empty() {
            return Ok(());
        }

        // Create upcasts directory
        let upcasts_dir = self.config.out_dir.join("src").join("generated").join("upcasts");
        std::fs::create_dir_all(&upcasts_dir).map_err(|e| CompilerError::IoError {
            path: upcasts_dir.clone(),
            message: e.to_string(),
        })?;

        // Generate upcast file for each aggregate with changes
        for (aggregate_name, aggregate_diffs) in by_aggregate {
            // Get current versions from the locked schema
            let current_versions: HashMap<String, u32> = locked
                .aggregates
                .get(aggregate_name)
                .map(|agg| {
                    agg.events
                        .iter()
                        .map(|(name, schema)| (name.clone(), schema.version + 1))
                        .collect()
                })
                .unwrap_or_default();

            let code = schema::UpcastGenerator::generate_upcast_module(
                aggregate_name,
                aggregate_diffs
                    .iter()
                    .map(|d| (*d).clone())
                    .collect::<Vec<_>>()
                    .as_slice(),
                &current_versions,
            );

            let snake_name = aggregate_name
                .chars()
                .enumerate()
                .flat_map(|(i, c)| {
                    if c.is_uppercase() && i > 0 {
                        vec!['_', c.to_lowercase().next().unwrap()]
                    } else {
                        vec![c.to_lowercase().next().unwrap()]
                    }
                })
                .collect::<String>();

            let file_path = upcasts_dir.join(format!("{}.upcast.ts", snake_name));
            std::fs::write(&file_path, code).map_err(|e| CompilerError::IoError {
                path: file_path.clone(),
                message: e.to_string(),
            })?;

            eprintln!("   Generated upcast file: {}", file_path.display());
        }

        Ok(())
    }

    /// Writes generated code to the output directory.
    fn write_output(&self, generated: &codegen::GeneratedCode) -> Result<(), CompilerError> {
        std::fs::create_dir_all(&self.config.out_dir).map_err(|e| CompilerError::IoError {
            path: self.config.out_dir.clone(),
            message: e.to_string(),
        })?;

        for (filename, content) in &generated.files {
            let path = self.config.out_dir.join(filename);
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CompilerError::IoError {
                    path: parent.to_path_buf(),
                    message: e.to_string(),
                })?;
            }
            std::fs::write(&path, content).map_err(|e| CompilerError::IoError {
                path,
                message: e.to_string(),
            })?;
        }

        Ok(())
    }

    /// Computes the relative import path from generated handlers/ to domain source.
    /// This is used to import user's source files (events.ts, state.ts, aggregate.ts)
    /// directly rather than regenerating them.
    fn compute_domain_import_path(&self) -> Result<String, CompilerError> {
        // Generated handlers are at: out_dir/src/generated/handlers/
        // Domain source is at: domain_dir/
        // 
        // We need the relative path from handlers/ to domain_dir/
        let handlers_dir = self.config.out_dir.join("src").join("generated").join("handlers");
        
        // Canonicalize paths (or use as-is if they don't exist yet)
        let handlers_abs = handlers_dir.canonicalize()
            .unwrap_or_else(|_| {
                // If output doesn't exist yet, compute from current dir
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(&handlers_dir)
            });
        
        let domain_abs = self.config.domain_dir.canonicalize()
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(&self.config.domain_dir)
            });
        
        // Compute relative path
        if let Some(rel_path) = pathdiff::diff_paths(&domain_abs, &handlers_abs) {
            Ok(rel_path.to_string_lossy().to_string())
        } else {
            // Fallback to a sensible default
            Ok("../../../../domain".to_string())
        }
    }

    /// Compiles to a full standalone Bun project in the specified directory.
    /// Creates package.json, tsconfig.json, index.ts, and generated domain code.
    pub async fn compile_project(&self, project_name: &str, port: u16) -> Result<CompileResult, CompilerError> {
        // First, compile the domain code
        let mut frontend = frontend::create_frontend(&self.config.language)?;
        let domain_ir = frontend.parse_directory(&self.config.domain_dir)?;

        if !self.config.skip_purity_check {
            validate::validate_domain(&domain_ir)?;
        }

        let domain_import_path = self.compute_domain_import_path()?;
        let generated = codegen::generate(&domain_ir, &domain_import_path)?;

        // Create project structure
        let project_dir = &self.config.out_dir;
        let src_dir = project_dir.join("src");
        let generated_dir = src_dir.join("generated");

        // Create subdirectories for generated code
        for subdir in &["validators", "handlers", "orchestrators"] {
            std::fs::create_dir_all(generated_dir.join(subdir)).map_err(|e| CompilerError::IoError {
                path: generated_dir.join(subdir),
                message: e.to_string(),
            })?;
        }

        // Detect if we're in the monorepo and get the napi path
        let napi_path = project::detect_napi_path(project_dir);

        // Write package.json
        let package_json = project::generate_package_json(project_name, napi_path.as_deref());
        std::fs::write(project_dir.join("package.json"), package_json).map_err(|e| CompilerError::IoError {
            path: project_dir.join("package.json"),
            message: e.to_string(),
        })?;

        // Write tsconfig.json
        let tsconfig = project::generate_tsconfig();
        std::fs::write(project_dir.join("tsconfig.json"), tsconfig).map_err(|e| CompilerError::IoError {
            path: project_dir.join("tsconfig.json"),
            message: e.to_string(),
        })?;

        // Write src/index.ts
        std::fs::create_dir_all(&src_dir).map_err(|e| CompilerError::IoError {
            path: src_dir.clone(),
            message: e.to_string(),
        })?;
        let index_ts = project::generate_index_ts(port, project_name);
        std::fs::write(src_dir.join("index.ts"), index_ts).map_err(|e| CompilerError::IoError {
            path: src_dir.join("index.ts"),
            message: e.to_string(),
        })?;

        // Write .gitignore
        let gitignore = project::generate_gitignore();
        std::fs::write(project_dir.join(".gitignore"), gitignore).map_err(|e| CompilerError::IoError {
            path: project_dir.join(".gitignore"),
            message: e.to_string(),
        })?;

        // Write README
        let readme = project::generate_readme(project_name);
        std::fs::write(project_dir.join("README.md"), readme).map_err(|e| CompilerError::IoError {
            path: project_dir.join("README.md"),
            message: e.to_string(),
        })?;

        // Write generated domain code
        for (filename, content) in &generated.files {
            let path = generated_dir.join(filename);
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CompilerError::IoError {
                    path: parent.to_path_buf(),
                    message: e.to_string(),
                })?;
            }
            std::fs::write(&path, content).map_err(|e| CompilerError::IoError {
                path,
                message: e.to_string(),
            })?;
        }

        Ok(CompileResult {
            aggregates: domain_ir.aggregates.len(),
            orchestrators: domain_ir.orchestrators.len(),
            events: domain_ir
                .aggregates
                .iter()
                .map(|a| a.events.variants.len())
                .sum(),
        })
    }

    /// Re-compiles just the generated domain code (for watch mode).
    pub async fn recompile_domain(&self) -> Result<CompileResult, CompilerError> {
        let mut frontend = frontend::create_frontend(&self.config.language)?;
        let domain_ir = frontend.parse_directory(&self.config.domain_dir)?;

        if !self.config.skip_purity_check {
            validate::validate_domain(&domain_ir)?;
        }

        let domain_import_path = self.compute_domain_import_path()?;
        let generated = codegen::generate(&domain_ir, &domain_import_path)?;

        // Write only the generated wiring code
        let generated_dir = self.config.out_dir.join("src").join("generated");

        // Create subdirectories (validators, handlers, orchestrators - not events/state/aggregates)
        for subdir in &["validators", "handlers", "orchestrators"] {
            std::fs::create_dir_all(generated_dir.join(subdir)).map_err(|e| CompilerError::IoError {
                path: generated_dir.join(subdir),
                message: e.to_string(),
            })?;
        }

        for (filename, content) in &generated.files {
            let path = generated_dir.join(filename);
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CompilerError::IoError {
                    path: parent.to_path_buf(),
                    message: e.to_string(),
                })?;
            }
            std::fs::write(&path, content).map_err(|e| CompilerError::IoError {
                path,
                message: e.to_string(),
            })?;
        }

        Ok(CompileResult {
            aggregates: domain_ir.aggregates.len(),
            orchestrators: domain_ir.orchestrators.len(),
            events: domain_ir
                .aggregates
                .iter()
                .map(|a| a.events.variants.len())
                .sum(),
        })
    }
}

/// Result of a successful compilation.
#[derive(Debug)]
pub struct CompileResult {
    /// Number of aggregates compiled.
    pub aggregates: usize,
    /// Number of orchestrators compiled.
    pub orchestrators: usize,
    /// Total number of event variants across all aggregates.
    pub events: usize,
}
