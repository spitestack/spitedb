//! Structure validation for domain types.
//!
//! Validates that aggregates have all required members and
//! that types are correctly structured.

use crate::diagnostic::CompilerError;
use crate::ir::{AggregateIR, DomainIR};

/// Validates the structure of the domain IR.
pub fn validate_structure(domain: &DomainIR) -> Result<(), CompilerError> {
    for aggregate in &domain.aggregates {
        validate_aggregate_structure(aggregate)?;
    }
    Ok(())
}

/// Validates an aggregate has all required components.
fn validate_aggregate_structure(aggregate: &AggregateIR) -> Result<(), CompilerError> {
    // Check that state has at least one field
    if aggregate.state.fields.is_empty() {
        return Err(CompilerError::InvalidStateType {
            type_name: format!("{}State", aggregate.name),
        });
    }

    // Check that events have at least one variant
    if aggregate.events.variants.is_empty() {
        return Err(CompilerError::InvalidEventType {
            type_name: aggregate.events.name.clone(),
        });
    }

    // Check that each event variant has a valid name
    for variant in &aggregate.events.variants {
        if variant.name.is_empty() {
            return Err(CompilerError::InvalidEventType {
                type_name: format!("{} variant", aggregate.events.name),
            });
        }
    }

    Ok(())
}
