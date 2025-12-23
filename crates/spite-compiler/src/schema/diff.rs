//! Schema diff algorithm for change detection.
//!
//! This module compares event schemas between the lock file and current code
//! to detect changes. Changes are classified as:
//!
//! - **Non-breaking**: Can be auto-upcasted (e.g., adding optional fields)
//! - **Breaking**: Require new event type (e.g., removing fields, changing types)

use std::collections::HashMap;

use super::lock::{EventSchema, FieldSchema};

/// A change to a single field.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldChange {
    /// A new field was added.
    Added {
        name: String,
        schema: FieldSchema,
    },

    /// A field was removed.
    Removed {
        name: String,
        schema: FieldSchema,
    },

    /// A field's type changed.
    TypeChanged {
        name: String,
        old_type: String,
        new_type: String,
    },

    /// A field's required status changed.
    RequiredChanged {
        name: String,
        /// Was optional, now required (breaking) or vice versa
        was_optional: bool,
    },
}

impl FieldChange {
    /// Whether this change is breaking.
    pub fn is_breaking(&self) -> bool {
        match self {
            FieldChange::Added { schema, .. } => {
                // Adding a required field is breaking
                schema.required
            }
            FieldChange::Removed { .. } => true,
            FieldChange::TypeChanged { .. } => true,
            FieldChange::RequiredChanged { was_optional, .. } => {
                // Optional -> Required is breaking
                *was_optional
            }
        }
    }
}

/// Type of change for summary purposes.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    /// Non-breaking change that can be auto-upcasted.
    NonBreaking,
    /// Breaking change that requires a new event type.
    Breaking,
}

/// Diff result for a single event.
#[derive(Debug, Clone)]
pub struct SchemaDiff {
    /// Aggregate name.
    pub aggregate: String,
    /// Event name.
    pub event: String,
    /// List of field changes.
    pub changes: Vec<FieldChange>,
}

impl SchemaDiff {
    /// Whether this diff contains any breaking changes.
    pub fn is_breaking(&self) -> bool {
        self.changes.iter().any(|c| c.is_breaking())
    }

    /// Whether this diff can be auto-upcasted.
    pub fn can_auto_upcast(&self) -> bool {
        !self.is_breaking() && !self.changes.is_empty()
    }

    /// Get all added optional fields (for auto-upcast).
    pub fn added_optional_fields(&self) -> Vec<(&str, &FieldSchema)> {
        self.changes
            .iter()
            .filter_map(|c| match c {
                FieldChange::Added { name, schema } if !schema.required => {
                    Some((name.as_str(), schema))
                }
                _ => None,
            })
            .collect()
    }

    /// Format the diff for display.
    pub fn format_changes(&self) -> String {
        let mut lines = Vec::new();

        for change in &self.changes {
            let (desc, breaking) = match change {
                FieldChange::Added { name, schema } => {
                    let typ = &schema.typ;
                    let opt = if schema.required { "" } else { "?" };
                    let default = schema
                        .default
                        .as_ref()
                        .map(|d| format!(" = {}", d))
                        .unwrap_or_default();
                    (
                        format!("+ Field '{}': {}{}{}", name, typ, opt, default),
                        schema.required,
                    )
                }
                FieldChange::Removed { name, .. } => {
                    (format!("- Field '{}' removed", name), true)
                }
                FieldChange::TypeChanged {
                    name,
                    old_type,
                    new_type,
                } => (
                    format!("~ Field '{}' type changed: {} -> {}", name, old_type, new_type),
                    true,
                ),
                FieldChange::RequiredChanged { name, was_optional } => {
                    let desc = if *was_optional {
                        format!("~ Field '{}' changed from optional to required", name)
                    } else {
                        format!("~ Field '{}' changed from required to optional", name)
                    };
                    (desc, *was_optional)
                }
            };

            let marker = if breaking { "(BREAKING)" } else { "(OK)" };
            lines.push(format!("  {} {}", desc, marker));
        }

        lines.join("\n")
    }
}

/// Compare two event schemas and return the differences.
pub fn diff_event_schemas(
    locked: &EventSchema,
    current_fields: &HashMap<String, FieldSchema>,
) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    // Check for removed or changed fields
    for (name, locked_schema) in &locked.fields {
        match current_fields.get(name) {
            None => {
                // Field was removed
                changes.push(FieldChange::Removed {
                    name: name.clone(),
                    schema: locked_schema.clone(),
                });
            }
            Some(current_schema) => {
                // Check for type change
                if locked_schema.typ != current_schema.typ {
                    changes.push(FieldChange::TypeChanged {
                        name: name.clone(),
                        old_type: locked_schema.typ.clone(),
                        new_type: current_schema.typ.clone(),
                    });
                }
                // Check for required status change
                else if locked_schema.required != current_schema.required {
                    changes.push(FieldChange::RequiredChanged {
                        name: name.clone(),
                        was_optional: !locked_schema.required,
                    });
                }
            }
        }
    }

    // Check for added fields
    for (name, current_schema) in current_fields {
        if !locked.fields.contains_key(name) {
            changes.push(FieldChange::Added {
                name: name.clone(),
                schema: current_schema.clone(),
            });
        }
    }

    changes
}

/// Compare schemas from lock file against current code.
///
/// Returns a list of all diffs (empty if schemas match).
pub fn diff_schemas(
    locked: &HashMap<String, super::lock::AggregateLock>,
    current: &crate::ir::DomainIR,
) -> Vec<SchemaDiff> {
    let mut diffs = Vec::new();

    for aggregate in &current.aggregates {
        let locked_aggregate = match locked.get(&aggregate.name) {
            Some(a) => a,
            None => {
                // New aggregate, not in lock file - this is OK for new events
                continue;
            }
        };

        for variant in &aggregate.events.variants {
            let locked_event = match locked_aggregate.events.get(&variant.name) {
                Some(e) => e,
                None => {
                    // New event, not in lock file - this is OK
                    continue;
                }
            };

            // Convert current fields to FieldSchema map
            let current_fields: HashMap<String, FieldSchema> = variant
                .fields
                .iter()
                .map(|f| {
                    // Check if the type is optional by looking for DomainType::Option wrapper
                    let (typ, is_optional) = match &f.typ {
                        crate::ir::DomainType::Option(inner) => {
                            (super::lock::domain_type_to_string_pub(inner), true)
                        }
                        other => (super::lock::domain_type_to_string_pub(other), false),
                    };

                    (
                        f.name.clone(),
                        FieldSchema {
                            typ,
                            required: !is_optional,
                            default: if is_optional {
                                Some(serde_json::Value::Null)
                            } else {
                                None
                            },
                        },
                    )
                })
                .collect();

            let changes = diff_event_schemas(locked_event, &current_fields);

            if !changes.is_empty() {
                diffs.push(SchemaDiff {
                    aggregate: aggregate.name.clone(),
                    event: variant.name.clone(),
                    changes,
                });
            }
        }

        // Check for removed events
        for (event_name, _) in &locked_aggregate.events {
            let exists_in_current = aggregate
                .events
                .variants
                .iter()
                .any(|v| &v.name == event_name);

            if !exists_in_current {
                diffs.push(SchemaDiff {
                    aggregate: aggregate.name.clone(),
                    event: event_name.clone(),
                    changes: vec![FieldChange::Removed {
                        name: event_name.clone(),
                        schema: FieldSchema {
                            typ: "event".to_string(),
                            required: true,
                            default: None,
                        },
                    }],
                });
            }
        }
    }

    diffs
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_no_changes() {
        let locked = EventSchema {
            version: 1,
            previous_version: None,
            fields: {
                let mut f = HashMap::new();
                f.insert("id".to_string(), make_field("string", true));
                f
            },
            upcast_from: HashMap::new(),
            hash: "test".to_string(),
        };

        let current: HashMap<String, FieldSchema> = {
            let mut f = HashMap::new();
            f.insert("id".to_string(), make_field("string", true));
            f
        };

        let changes = diff_event_schemas(&locked, &current);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_added_optional_field() {
        let locked = EventSchema {
            version: 1,
            previous_version: None,
            fields: {
                let mut f = HashMap::new();
                f.insert("id".to_string(), make_field("string", true));
                f
            },
            upcast_from: HashMap::new(),
            hash: "test".to_string(),
        };

        let current: HashMap<String, FieldSchema> = {
            let mut f = HashMap::new();
            f.insert("id".to_string(), make_field("string", true));
            f.insert("description".to_string(), make_field("string", false));
            f
        };

        let changes = diff_event_schemas(&locked, &current);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FieldChange::Added { name, schema }
            if name == "description" && !schema.required));
        assert!(!changes[0].is_breaking());
    }

    #[test]
    fn test_added_required_field_is_breaking() {
        let locked = EventSchema {
            version: 1,
            previous_version: None,
            fields: {
                let mut f = HashMap::new();
                f.insert("id".to_string(), make_field("string", true));
                f
            },
            upcast_from: HashMap::new(),
            hash: "test".to_string(),
        };

        let current: HashMap<String, FieldSchema> = {
            let mut f = HashMap::new();
            f.insert("id".to_string(), make_field("string", true));
            f.insert("title".to_string(), make_field("string", true));
            f
        };

        let changes = diff_event_schemas(&locked, &current);
        assert_eq!(changes.len(), 1);
        assert!(changes[0].is_breaking());
    }

    #[test]
    fn test_removed_field_is_breaking() {
        let locked = EventSchema {
            version: 1,
            previous_version: None,
            fields: {
                let mut f = HashMap::new();
                f.insert("id".to_string(), make_field("string", true));
                f.insert("title".to_string(), make_field("string", true));
                f
            },
            upcast_from: HashMap::new(),
            hash: "test".to_string(),
        };

        let current: HashMap<String, FieldSchema> = {
            let mut f = HashMap::new();
            f.insert("id".to_string(), make_field("string", true));
            f
        };

        let changes = diff_event_schemas(&locked, &current);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FieldChange::Removed { name, .. } if name == "title"));
        assert!(changes[0].is_breaking());
    }

    #[test]
    fn test_type_change_is_breaking() {
        let locked = EventSchema {
            version: 1,
            previous_version: None,
            fields: {
                let mut f = HashMap::new();
                f.insert("count".to_string(), make_field("string", true));
                f
            },
            upcast_from: HashMap::new(),
            hash: "test".to_string(),
        };

        let current: HashMap<String, FieldSchema> = {
            let mut f = HashMap::new();
            f.insert("count".to_string(), make_field("number", true));
            f
        };

        let changes = diff_event_schemas(&locked, &current);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FieldChange::TypeChanged { .. }));
        assert!(changes[0].is_breaking());
    }

    #[test]
    fn test_schema_diff_methods() {
        let diff = SchemaDiff {
            aggregate: "Todo".to_string(),
            event: "Created".to_string(),
            changes: vec![
                FieldChange::Added {
                    name: "description".to_string(),
                    schema: make_field("string", false),
                },
            ],
        };

        assert!(!diff.is_breaking());
        assert!(diff.can_auto_upcast());

        let optional_fields = diff.added_optional_fields();
        assert_eq!(optional_fields.len(), 1);
        assert_eq!(optional_fields[0].0, "description");
    }
}
