//! Event type code generation for TypeScript.

use crate::ir::EventTypeIR;
use super::ts_types::to_ts_type;

/// Generates TypeScript code for an event discriminated union type.
pub fn generate_event_type(events: &EventTypeIR) -> String {
    let mut output = String::new();

    // Generate the discriminated union type
    output.push_str(&format!("export type {} =\n", events.name));

    let variants: Vec<String> = events
        .variants
        .iter()
        .map(|variant| {
            let fields: Vec<String> = variant
                .fields
                .iter()
                .map(|field| format!("{}: {}", field.name, to_ts_type(&field.typ)))
                .collect();

            let fields_str = if fields.is_empty() {
                String::new()
            } else {
                format!("; {}", fields.join("; "))
            };

            format!("  | {{ type: \"{}\"{} }}", variant.name, fields_str)
        })
        .collect();

    output.push_str(&variants.join("\n"));
    output.push_str(";\n");

    // Generate event type guard helpers
    output.push('\n');
    for variant in &events.variants {
        output.push_str(&format!(
            "export function is{}(event: {}): event is {{ type: \"{}\" }} & {} {{\n",
            variant.name,
            events.name,
            variant.name,
            events.name
        ));
        output.push_str(&format!(
            "  return event.type === \"{}\";\n",
            variant.name
        ));
        output.push_str("}\n\n");
    }

    output
}