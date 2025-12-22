//! State type code generation for TypeScript.

use crate::ir::{AggregateIR, DomainType, InitialValue};
use super::ts_types::to_ts_type;

/// Generates TypeScript code for a state type and initial value.
pub fn generate_state_type(aggregate: &AggregateIR) -> String {
    let mut output = String::new();
    let type_name = format!("{}State", aggregate.name);

    // Generate the state type
    output.push_str(&format!("export type {} = {{\n", type_name));

    for field in &aggregate.state.fields {
        let optional_marker = if field.optional { "?" } else { "" };
        output.push_str(&format!(
            "  {}{}: {};\n",
            field.name,
            optional_marker,
            to_ts_type(&field.typ)
        ));
    }

    output.push_str("};\n\n");

    // Generate the initial state constant
    output.push_str(&format!(
        "export const initial{}State: {} = {{\n",
        aggregate.name, type_name
    ));

    for field in &aggregate.state.fields {
        let value = aggregate
            .initial_state
            .iter()
            .find(|(name, _)| name == &field.name)
            .map(|(_, v)| initial_value_to_ts(v))
            .unwrap_or_else(|| default_for_type(&field.typ));

        output.push_str(&format!("  {}: {},\n", field.name, value));
    }

    output.push_str("};\n");

    output
}

/// Converts an InitialValue to a TypeScript expression.
fn initial_value_to_ts(value: &InitialValue) -> String {
    match value {
        InitialValue::String(s) => {
            if s.is_empty() {
                "\"\"".to_string()
            } else {
                format!("\"{}\"", s.replace('\"', "\\\""))
            }
        }
        InitialValue::Number(n) => n.to_string(),
        InitialValue::Boolean(b) => b.to_string(),
        InitialValue::Null => "undefined".to_string(),
        InitialValue::EmptyArray => "[]".to_string(),
        InitialValue::EmptyObject => "{}".to_string(),
    }
}

/// Returns the default TypeScript value for a domain type.
fn default_for_type(typ: &DomainType) -> String {
    match typ {
        DomainType::String => "\"\"".to_string(),
        DomainType::Number => "0".to_string(),
        DomainType::Boolean => "false".to_string(),
        DomainType::Array(_) => "[]".to_string(),
        DomainType::Option(_) => "undefined".to_string(),
        DomainType::Reference(_) => "undefined as any".to_string(),
        DomainType::Object(_) => "{}".to_string(),
    }
}