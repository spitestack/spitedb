//! Upcast code generation for schema evolution.
//!
//! When event schemas change in non-breaking ways (e.g., adding optional fields),
//! this module generates TypeScript upcast functions that transform old event
//! formats to new ones at read-time.

use std::collections::HashMap;

use super::diff::SchemaDiff;

/// Strategy for upcasting events from one version to another.
#[derive(Debug, Clone)]
pub enum UpcastStrategy {
    /// Auto-generated upcast that adds optional fields with defaults.
    Auto {
        /// Fields to add: (name, default_value_json)
        added_fields: Vec<(String, String)>,
    },

    /// Custom upcast function provided by the user.
    Custom {
        /// Name of the custom upcast function.
        function_name: String,
    },
}

/// Generator for upcast TypeScript code.
pub struct UpcastGenerator;

impl UpcastGenerator {
    /// Generate TypeScript upcast code for an aggregate.
    ///
    /// Returns the content of the upcast module file.
    pub fn generate_upcast_module(
        aggregate_name: &str,
        diffs: &[SchemaDiff],
        current_versions: &HashMap<String, u32>,
    ) -> String {
        let mut code = String::new();

        // Module header
        code.push_str(&format!(
            r#"/**
 * Auto-generated event upcasters for {} aggregate.
 * DO NOT EDIT - regenerate with `spitestack compile`
 */

"#,
            aggregate_name
        ));

        // Import type (assuming events are defined in the aggregate's events.ts)
        let lower_name = to_snake_case(aggregate_name);
        code.push_str(&format!(
            "import type {{ {}Event }} from '../../../../domain/{}/events';\n\n",
            aggregate_name, aggregate_name
        ));

        // Generate upcast functions for each event with changes
        let mut registry_entries = Vec::new();

        for diff in diffs {
            if diff.aggregate != aggregate_name || !diff.can_auto_upcast() {
                continue;
            }

            let current_version = current_versions.get(&diff.event).copied().unwrap_or(1);
            let from_version = current_version - 1;

            // Generate the upcast function
            let func_name = format!(
                "upcast{}_{}_v{}_to_v{}",
                aggregate_name, diff.event, from_version, current_version
            );

            let added_fields = diff.added_optional_fields();
            let field_assignments: Vec<String> = added_fields
                .iter()
                .map(|(name, schema)| {
                    let default = schema
                        .default
                        .as_ref()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "null".to_string());
                    format!("    {}: {},  // Added in v{}", name, default, current_version)
                })
                .collect();

            code.push_str(&format!(
                r#"// Version {} -> Version {} upcast for {}
export function {}(event: Record<string, unknown>): {}Event {{
  return {{
    ...event,
{}
  }} as {}Event;
}}

"#,
                from_version,
                current_version,
                diff.event,
                func_name,
                aggregate_name,
                field_assignments.join("\n"),
                aggregate_name
            ));

            registry_entries.push((diff.event.clone(), from_version, func_name));
        }

        // Generate the upcast registry
        code.push_str("// Event upcast registry\n");
        code.push_str(&format!(
            "export const {}UpcastRegistry: Record<string, Record<number, (event: Record<string, unknown>) => {}Event>> = {{\n",
            lower_name, aggregate_name
        ));

        // Group by event name
        let mut events: HashMap<String, Vec<(u32, String)>> = HashMap::new();
        for (event_name, version, func_name) in registry_entries {
            events
                .entry(event_name)
                .or_default()
                .push((version, func_name));
        }

        for (event_name, versions) in events {
            code.push_str(&format!("  {}: {{\n", event_name));
            for (version, func_name) in versions {
                code.push_str(&format!("    {}: {},\n", version, func_name));
            }
            code.push_str("  },\n");
        }

        code.push_str("};\n\n");

        // Generate the main upcast function
        let max_version = current_versions.values().max().copied().unwrap_or(1);
        code.push_str(&format!(
            r#"const CURRENT_VERSION = {};

/**
 * Apply upcasts to bring an event to the current schema version.
 *
 * @param event - The raw event data from storage
 * @param storedVersion - The schema version the event was stored with
 * @returns The event transformed to current schema version
 */
export function upcast{}Event(event: Record<string, unknown>, storedVersion: number): {}Event {{
  const eventType = event.type as string;
  const upcasters = {}UpcastRegistry[eventType];

  if (!upcasters) {{
    // No upcasters for this event type, return as-is
    return event as {}Event;
  }}

  let current = event;
  for (let v = storedVersion; v < CURRENT_VERSION; v++) {{
    const upcast = upcasters[v];
    if (upcast) {{
      current = upcast(current);
    }}
  }}

  return current as {}Event;
}}
"#,
            max_version,
            aggregate_name,
            aggregate_name,
            lower_name,
            aggregate_name,
            aggregate_name
        ));

        code
    }

    /// Generate upcast code for a single event from a diff.
    pub fn generate_upcast_from_diff(
        aggregate: &str,
        diff: &SchemaDiff,
        from_version: u32,
        to_version: u32,
    ) -> Option<String> {
        if !diff.can_auto_upcast() {
            return None;
        }

        let added_fields = diff.added_optional_fields();
        if added_fields.is_empty() {
            return None;
        }

        let func_name = format!(
            "upcast{}_{}_v{}_to_v{}",
            aggregate, diff.event, from_version, to_version
        );

        let field_assignments: Vec<String> = added_fields
            .iter()
            .map(|(name, schema)| {
                let default = schema
                    .default
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "null".to_string());
                format!("    {}: {},", name, default)
            })
            .collect();

        Some(format!(
            r#"export function {}(event: Record<string, unknown>): unknown {{
  return {{
    ...event,
{}
  }};
}}"#,
            func_name,
            field_assignments.join("\n")
        ))
    }
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::diff::FieldChange;
    use crate::schema::lock::FieldSchema;

    fn make_field(typ: &str, required: bool) -> FieldSchema {
        FieldSchema {
            typ: typ.to_string(),
            required,
            default: if required {
                None
            } else {
                Some(serde_json::Value::Null)
            },
        }
    }

    #[test]
    fn test_generate_upcast_from_diff() {
        let diff = SchemaDiff {
            aggregate: "Todo".to_string(),
            event: "Created".to_string(),
            changes: vec![FieldChange::Added {
                name: "description".to_string(),
                schema: make_field("string", false),
            }],
        };

        let code = UpcastGenerator::generate_upcast_from_diff("Todo", &diff, 1, 2);
        assert!(code.is_some());

        let code = code.unwrap();
        assert!(code.contains("upcastTodo_Created_v1_to_v2"));
        assert!(code.contains("description: null"));
    }

    #[test]
    fn test_no_upcast_for_breaking_changes() {
        let diff = SchemaDiff {
            aggregate: "Todo".to_string(),
            event: "Created".to_string(),
            changes: vec![FieldChange::Removed {
                name: "title".to_string(),
                schema: make_field("string", true),
            }],
        };

        let code = UpcastGenerator::generate_upcast_from_diff("Todo", &diff, 1, 2);
        assert!(code.is_none());
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Todo"), "todo");
        assert_eq!(to_snake_case("OrderAggregate"), "order_aggregate");
        assert_eq!(to_snake_case("HTTPRequest"), "h_t_t_p_request");
    }

    #[test]
    fn test_generate_upcast_module() {
        let diffs = vec![SchemaDiff {
            aggregate: "Todo".to_string(),
            event: "Created".to_string(),
            changes: vec![FieldChange::Added {
                name: "description".to_string(),
                schema: make_field("string", false),
            }],
        }];

        let mut versions = HashMap::new();
        versions.insert("Created".to_string(), 2);

        let code = UpcastGenerator::generate_upcast_module("Todo", &diffs, &versions);

        assert!(code.contains("Auto-generated event upcasters for Todo"));
        assert!(code.contains("upcastTodoEvent"));
        assert!(code.contains("todoUpcastRegistry"));
    }
}
