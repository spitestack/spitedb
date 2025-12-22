//! Validation of domain IR.

mod purity;
mod structure;

use crate::diagnostic::CompilerError;
use crate::ir::DomainIR;

/// Validates the entire domain.
pub fn validate_domain(domain: &DomainIR) -> Result<(), CompilerError> {
    // Validate structure
    structure::validate_structure(domain)?;

    // Validate purity of aggregates
    for aggregate in &domain.aggregates {
        purity::validate_aggregate_purity(aggregate)?;
    }

    Ok(())
}
