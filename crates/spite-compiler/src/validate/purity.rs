//! Purity validation for domain logic.
//!
//! Domain logic (aggregates) must be pure - no side effects allowed.
//! This includes:
//! - No console.log, fetch, setTimeout, etc.
//! - No Date.now(), Math.random()
//! - No await in aggregates
//! - No external package imports

use std::path::PathBuf;
use crate::diagnostic::CompilerError;
use crate::ir::{AggregateIR, StatementIR, ExpressionIR};

/// Forbidden function/method calls.
const FORBIDDEN_CALLS: &[&str] = &[
    // Console
    "console.log",
    "console.error",
    "console.warn",
    "console.info",
    "console.debug",
    // Network
    "fetch",
    "XMLHttpRequest",
    // Timers
    "setTimeout",
    "setInterval",
    "setImmediate",
    // Non-deterministic
    "Date.now",
    "Math.random",
    // DOM
    "document",
    "window",
    // File system
    "fs.readFileSync",
    "fs.writeFileSync",
];

/// Validates that an aggregate is pure.
pub fn validate_aggregate_purity(aggregate: &AggregateIR) -> Result<(), CompilerError> {
    for command in &aggregate.commands {
        for stmt in &command.body {
            validate_statement(stmt, &aggregate.source_path)?;
        }
    }
    Ok(())
}

/// Validates a statement for purity.
fn validate_statement(stmt: &StatementIR, file: &PathBuf) -> Result<(), CompilerError> {
    match stmt {
        StatementIR::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_expression(condition, file)?;
            for s in then_branch {
                validate_statement(s, file)?;
            }
            if let Some(else_stmts) = else_branch {
                for s in else_stmts {
                    validate_statement(s, file)?;
                }
            }
        }
        StatementIR::Throw { .. } => {
            // Throws are allowed
        }
        StatementIR::Emit { fields, .. } => {
            for (_, expr) in fields {
                validate_expression(expr, file)?;
            }
        }
        StatementIR::Let { value, .. } => {
            validate_expression(value, file)?;
        }
        StatementIR::Expression(expr) => {
            validate_expression(expr, file)?;
        }
        StatementIR::Return(Some(expr)) => {
            validate_expression(expr, file)?;
        }
        StatementIR::Return(None) => {}
    }
    Ok(())
}

/// Validates an expression for purity.
fn validate_expression(expr: &ExpressionIR, file: &PathBuf) -> Result<(), CompilerError> {
    match expr {
        ExpressionIR::Call { callee, arguments } => {
            // Check for forbidden calls
            if FORBIDDEN_CALLS.iter().any(|f| callee.contains(f)) {
                return Err(CompilerError::ForbiddenCall {
                    name: callee.clone(),
                    file: file.clone(),
                    line: 0, // We don't have line info in IR
                });
            }
            for arg in arguments {
                validate_expression(arg, file)?;
            }
        }
        ExpressionIR::MethodCall {
            object,
            method,
            arguments,
        } => {
            // Check for forbidden method calls
            let full_name = format!("{:?}.{}", object, method);
            if FORBIDDEN_CALLS.iter().any(|f| full_name.contains(f)) {
                return Err(CompilerError::ForbiddenCall {
                    name: full_name,
                    file: file.clone(),
                    line: 0,
                });
            }
            validate_expression(object, file)?;
            for arg in arguments {
                validate_expression(arg, file)?;
            }
        }
        ExpressionIR::PropertyAccess { object, .. } => {
            validate_expression(object, file)?;
        }
        ExpressionIR::Binary { left, right, .. } => {
            validate_expression(left, file)?;
            validate_expression(right, file)?;
        }
        ExpressionIR::Unary { operand, .. } => {
            validate_expression(operand, file)?;
        }
        ExpressionIR::Object(fields) => {
            for (_, v) in fields {
                validate_expression(v, file)?;
            }
        }
        ExpressionIR::Array(elements) => {
            for e in elements {
                validate_expression(e, file)?;
            }
        }
        // Literals and identifiers are always pure
        _ => {}
    }
    Ok(())
}
