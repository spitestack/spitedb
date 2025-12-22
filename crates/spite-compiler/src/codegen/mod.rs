//! TypeScript code generation from domain IR.
//!
//! The compiler validates user source code and generates only the wiring/boilerplate:
//! - Validators (pure TypeScript runtime validation)
//! - Handlers (HTTP handlers that wire aggregates to SpiteDB)
//! - Router (Bun.serve routing)
//! - Runtime modules (auth, utilities, etc.)
//!
//! User's source files (events.ts, state.ts, aggregate.ts) are NOT regenerated -
//! we import them directly from the domain folder.

mod ts_types;
mod validators;
mod handlers;
mod router;
mod orchestrator;
mod runtime;
pub mod project;

use crate::diagnostic::CompilerError;
use crate::ir::DomainIR;
use ts_types::to_snake_case;

/// Generated TypeScript code.
pub struct GeneratedCode {
    /// Map of filename to content.
    pub files: Vec<(String, String)>,
}

/// Generates TypeScript code from domain IR.
/// 
/// Only generates wiring code (validators, handlers, router).
/// User's source files are imported directly, not regenerated.
/// 
/// `domain_import_path` is the relative path from the generated handlers directory 
/// to the domain source directory (e.g., "../../../../domain" for typical project structure).
pub fn generate(domain: &DomainIR, domain_import_path: &str) -> Result<GeneratedCode, CompilerError> {
    let mut files = Vec::new();

    // Generate code for each aggregate
    for aggregate in &domain.aggregates {
        let snake_name = to_snake_case(&aggregate.name);

        // Validators - generates runtime validation for commands
        let validators_code = validators::generate_validators(aggregate, domain_import_path);
        files.push((
            format!("validators/{}.validator.ts", snake_name),
            validators_code,
        ));

        // Handlers - wires aggregates to HTTP + SpiteDB
        let handlers_code = handlers::generate_handlers(aggregate, domain_import_path);
        files.push((
            format!("handlers/{}.handlers.ts", snake_name),
            handlers_code,
        ));
    }

    // Generate orchestrators
    for orch in &domain.orchestrators {
        let snake_name = to_snake_case(&orch.name);
        let orchestrator_code = orchestrator::generate_orchestrator(orch);
        files.push((
            format!("orchestrators/{}.orchestrator.ts", snake_name),
            orchestrator_code,
        ));
    }

    // Generate router
    let router_code = router::generate_router(domain);
    files.push(("router.ts".to_string(), router_code));

    // Generate index re-exports
    let aggregate_names: Vec<String> = domain.aggregates.iter().map(|a| a.name.clone()).collect();
    let index_code = project::generate_generated_index(&aggregate_names, domain_import_path);
    files.push(("index.ts".to_string(), index_code));

    // Include runtime modules (auth, utilities, etc.)
    for (filename, content) in runtime::get_runtime_modules() {
        files.push((filename.to_string(), content.to_string()));
    }

    Ok(GeneratedCode { files })
}