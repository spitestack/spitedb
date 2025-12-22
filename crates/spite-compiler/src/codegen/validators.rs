//! Pure TypeScript validator code generation.

use crate::ir::{AggregateIR, CommandIR, DomainType, ParameterIR};
use super::ts_types::{to_ts_type, to_pascal_case};

/// Generates TypeScript validators for all commands in an aggregate.
/// 
/// `_domain_import_path` is unused here but kept for API consistency.
pub fn generate_validators(aggregate: &AggregateIR, _domain_import_path: &str) -> String {
    let mut output = String::new();

    // Common types
    output.push_str("export type ValidationError = { field: string; message: string };\n\n");
    output.push_str("export type ValidationResult<T> =\n");
    output.push_str("  | { ok: true; value: T }\n");
    output.push_str("  | { ok: false; errors: ValidationError[] };\n\n");

    // Generate input types and validators for each command
    for cmd in &aggregate.commands {
        output.push_str(&generate_command_input_type(cmd, &aggregate.name));
        output.push('\n');
        output.push_str(&generate_command_validator(cmd, &aggregate.name));
        output.push('\n');
    }

    output
}

/// Generates the input type for a command.
fn generate_command_input_type(cmd: &CommandIR, aggregate_name: &str) -> String {
    let type_name = format!("{}{}Input", aggregate_name, to_pascal_case(&cmd.name));

    if cmd.parameters.is_empty() {
        return format!("export type {} = Record<string, never>;\n", type_name);
    }

    let mut output = format!("export type {} = {{\n", type_name);

    for param in &cmd.parameters {
        output.push_str(&format!("  {}: {};\n", param.name, to_ts_type(&param.typ)));
    }

    output.push_str("};\n");
    output
}

/// Generates a validator function for a command.
fn generate_command_validator(cmd: &CommandIR, aggregate_name: &str) -> String {
    let type_name = format!("{}{}Input", aggregate_name, to_pascal_case(&cmd.name));
    let fn_name = format!("validate{}{}Input", aggregate_name, to_pascal_case(&cmd.name));

    let mut output = format!(
        "export function {}(input: unknown): ValidationResult<{}> {{\n",
        fn_name, type_name
    );
    output.push_str("  const errors: ValidationError[] = [];\n\n");

    // Check if input is an object
    output.push_str("  if (typeof input !== 'object' || input === null) {\n");
    output.push_str("    return { ok: false, errors: [{ field: '_root', message: 'Expected object' }] };\n");
    output.push_str("  }\n\n");

    if cmd.parameters.is_empty() {
        output.push_str(&format!("  return {{ ok: true, value: {{}} as {} }};\n", type_name));
        output.push_str("}\n");
        return output;
    }

    output.push_str("  const obj = input as Record<string, unknown>;\n\n");

    // Generate validation for each parameter
    for param in &cmd.parameters {
        output.push_str(&generate_field_validation(param));
    }

    output.push_str("  if (errors.length > 0) {\n");
    output.push_str("    return { ok: false, errors };\n");
    output.push_str("  }\n\n");

    // Build the validated object
    let field_names: Vec<&str> = cmd.parameters.iter().map(|p| p.name.as_str()).collect();
    output.push_str(&format!(
        "  return {{ ok: true, value: {{ {} }} as {} }};\n",
        field_names.iter().map(|f| format!("{}: obj.{}", f, f)).collect::<Vec<_>>().join(", "),
        type_name
    ));
    output.push_str("}\n");

    output
}

/// Generates validation code for a single field.
fn generate_field_validation(param: &ParameterIR) -> String {
    let field = &param.name;
    generate_type_validation(field, &format!("obj.{}", field), &param.typ, 2)
}

/// Generates type validation code for a given path and type.
fn generate_type_validation(field: &str, path: &str, typ: &DomainType, indent: usize) -> String {
    let spaces = "  ".repeat(indent);
    let mut output = String::new();

    match typ {
        DomainType::String => {
            output.push_str(&format!(
                "{}if (typeof {} !== 'string') {{\n",
                spaces, path
            ));
            output.push_str(&format!(
                "{}  errors.push({{ field: '{}', message: 'Expected string' }});\n",
                spaces, field
            ));
            output.push_str(&format!("{}}}\n", spaces));
        }
        DomainType::Number => {
            output.push_str(&format!(
                "{}if (typeof {} !== 'number' || Number.isNaN({})) {{\n",
                spaces, path, path
            ));
            output.push_str(&format!(
                "{}  errors.push({{ field: '{}', message: 'Expected number' }});\n",
                spaces, field
            ));
            output.push_str(&format!("{}}}\n", spaces));
        }
        DomainType::Boolean => {
            output.push_str(&format!(
                "{}if (typeof {} !== 'boolean') {{\n",
                spaces, path
            ));
            output.push_str(&format!(
                "{}  errors.push({{ field: '{}', message: 'Expected boolean' }});\n",
                spaces, field
            ));
            output.push_str(&format!("{}}}\n", spaces));
        }
        DomainType::Array(inner) => {
            output.push_str(&format!(
                "{}if (!Array.isArray({})) {{\n",
                spaces, path
            ));
            output.push_str(&format!(
                "{}  errors.push({{ field: '{}', message: 'Expected array' }});\n",
                spaces, field
            ));
            output.push_str(&format!("{}}} else {{\n", spaces));
            output.push_str(&format!(
                "{}  for (let i = 0; i < {}.length; i++) {{\n",
                spaces, path
            ));
            output.push_str(&generate_type_validation(
                &format!("{}[i]", field),
                &format!("{}[i]", path),
                inner,
                indent + 2,
            ));
            output.push_str(&format!("{}  }}\n", spaces));
            output.push_str(&format!("{}}}\n", spaces));
        }
        DomainType::Option(inner) => {
            output.push_str(&format!(
                "{}if ({} !== undefined && {} !== null) {{\n",
                spaces, path, path
            ));
            output.push_str(&generate_type_validation(field, path, inner, indent + 1));
            output.push_str(&format!("{}}}\n", spaces));
        }
        DomainType::Object(obj) => {
            output.push_str(&format!(
                "{}if (typeof {} !== 'object' || {} === null) {{\n",
                spaces, path, path
            ));
            output.push_str(&format!(
                "{}  errors.push({{ field: '{}', message: 'Expected object' }});\n",
                spaces, field
            ));
            output.push_str(&format!("{}}} else {{\n", spaces));

            for f in &obj.fields {
                let nested_path = format!("({} as Record<string, unknown>).{}", path, f.name);
                let nested_field = format!("{}.{}", field, f.name);

                if f.optional {
                    output.push_str(&format!(
                        "{}  if ({} !== undefined) {{\n",
                        spaces, nested_path
                    ));
                    output.push_str(&generate_type_validation(
                        &nested_field,
                        &nested_path,
                        &f.typ,
                        indent + 2,
                    ));
                    output.push_str(&format!("{}  }}\n", spaces));
                } else {
                    output.push_str(&generate_type_validation(
                        &nested_field,
                        &nested_path,
                        &f.typ,
                        indent + 1,
                    ));
                }
            }

            output.push_str(&format!("{}}}\n", spaces));
        }
        DomainType::Reference(_) => {
            // For references, we just check it's an object (can't validate deeper without context)
            output.push_str(&format!(
                "{}if (typeof {} !== 'object' || {} === null) {{\n",
                spaces, path, path
            ));
            output.push_str(&format!(
                "{}  errors.push({{ field: '{}', message: 'Expected object' }});\n",
                spaces, field
            ));
            output.push_str(&format!("{}}}\n", spaces));
        }
    }

    output
}
