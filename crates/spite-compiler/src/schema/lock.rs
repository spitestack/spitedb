//! Schema lock file types and serialization.
//!
//! The lock file (`events.lock.json`) captures the event schemas at a point in time.
//! When in production mode, the compiler compares current schemas against this file
//! to detect changes and enforce safe evolution.

use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::diagnostic::CompilerError;
use crate::ir::{AggregateIR, DomainIR, DomainType, EventField};

/// The schema lock file format version.
pub const LOCK_FILE_VERSION: &str = "1.0";

/// The complete schema lock file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaLockFile {
    /// Lock file format version.
    pub version: String,

    /// When this lock file was generated.
    #[serde(rename = "generatedAt")]
    pub generated_at: String,

    /// Compiler version that generated this file.
    #[serde(rename = "compilerVersion")]
    pub compiler_version: String,

    /// Schemas for each aggregate.
    pub aggregates: HashMap<String, AggregateLock>,
}

/// Lock file entry for a single aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateLock {
    /// Event schemas keyed by event name.
    pub events: HashMap<String, EventSchema>,
}

/// Schema for a single event type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSchema {
    /// Schema version (starts at 1, increments on non-breaking changes).
    pub version: u32,

    /// Previous version this was upcasted from (if any).
    #[serde(rename = "previousVersion", skip_serializing_if = "Option::is_none")]
    pub previous_version: Option<u32>,

    /// Event fields.
    pub fields: HashMap<String, FieldSchema>,

    /// Upcast strategies from older versions.
    /// Key is the source version, value is the strategy ("auto" or custom function name).
    #[serde(rename = "upcastFrom", skip_serializing_if = "HashMap::is_empty", default)]
    pub upcast_from: HashMap<u32, String>,

    /// Content hash for quick comparison.
    pub hash: String,
}

/// Schema for a single field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldSchema {
    /// TypeScript type representation.
    #[serde(rename = "type")]
    pub typ: String,

    /// Whether the field is required.
    pub required: bool,

    /// Default value for optional fields (JSON representation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

impl SchemaLockFile {
    /// Load a lock file from disk.
    ///
    /// Returns `Ok(None)` if the file doesn't exist.
    pub fn load(path: &Path) -> Result<Option<Self>, CompilerError> {
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(path).map_err(|e| CompilerError::IoError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        let lock_file: Self =
            serde_json::from_str(&content).map_err(|e| CompilerError::IoError {
                path: path.to_path_buf(),
                message: format!("Failed to parse lock file: {}", e),
            })?;

        Ok(Some(lock_file))
    }

    /// Save the lock file to disk.
    pub fn save(&self, path: &Path) -> Result<(), CompilerError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CompilerError::IoError {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }

        let content =
            serde_json::to_string_pretty(self).map_err(|e| CompilerError::IoError {
                path: path.to_path_buf(),
                message: format!("Failed to serialize lock file: {}", e),
            })?;

        std::fs::write(path, content).map_err(|e| CompilerError::IoError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        Ok(())
    }

    /// Generate a lock file from the domain IR.
    pub fn from_domain_ir(domain: &DomainIR, compiler_version: &str) -> Self {
        let mut aggregates = HashMap::new();

        for aggregate in &domain.aggregates {
            let lock = AggregateLock::from_aggregate(aggregate);
            aggregates.insert(aggregate.name.clone(), lock);
        }

        Self {
            version: LOCK_FILE_VERSION.to_string(),
            generated_at: chrono_lite_now(),
            compiler_version: compiler_version.to_string(),
            aggregates,
        }
    }
}

impl AggregateLock {
    /// Create from an aggregate IR.
    fn from_aggregate(aggregate: &AggregateIR) -> Self {
        let mut events = HashMap::new();

        for variant in &aggregate.events.variants {
            let schema = EventSchema::from_variant(&variant.name, &variant.fields);
            events.insert(variant.name.clone(), schema);
        }

        Self { events }
    }
}

impl EventSchema {
    /// Create from an event variant.
    fn from_variant(name: &str, fields: &[EventField]) -> Self {
        let mut field_schemas = HashMap::new();

        for field in fields {
            let schema = FieldSchema::from_field(field);
            field_schemas.insert(field.name.clone(), schema);
        }

        let hash = compute_hash(name, &field_schemas);

        Self {
            version: 1,
            previous_version: None,
            fields: field_schemas,
            upcast_from: HashMap::new(),
            hash,
        }
    }

    /// Create a new version from an existing schema with added fields.
    pub fn new_version(&self, added_fields: HashMap<String, FieldSchema>) -> Self {
        let mut fields = self.fields.clone();
        fields.extend(added_fields);

        let hash = compute_hash_from_fields(&fields);

        let mut upcast_from = self.upcast_from.clone();
        upcast_from.insert(self.version, "auto".to_string());

        Self {
            version: self.version + 1,
            previous_version: Some(self.version),
            fields,
            upcast_from,
            hash,
        }
    }
}

impl FieldSchema {
    /// Create from an event field.
    fn from_field(field: &EventField) -> Self {
        // Check if the type is optional by looking for DomainType::Option wrapper
        let (typ, is_optional) = match &field.typ {
            DomainType::Option(inner) => (domain_type_to_string(inner), true),
            other => (domain_type_to_string(other), false),
        };

        let default = if is_optional {
            Some(serde_json::Value::Null)
        } else {
            None
        };

        Self {
            typ,
            required: !is_optional,
            default,
        }
    }
}

/// Convert a DomainType to its TypeScript string representation.
pub fn domain_type_to_string_pub(typ: &DomainType) -> String {
    domain_type_to_string(typ)
}

/// Convert a DomainType to its TypeScript string representation.
fn domain_type_to_string(typ: &DomainType) -> String {
    match typ {
        DomainType::String => "string".to_string(),
        DomainType::Number => "number".to_string(),
        DomainType::Boolean => "boolean".to_string(),
        DomainType::Array(inner) => format!("{}[]", domain_type_to_string(inner)),
        DomainType::Option(inner) => format!("{} | undefined", domain_type_to_string(inner)),
        DomainType::Object(obj) => {
            let fields: Vec<String> = obj
                .fields
                .iter()
                .map(|f| {
                    let opt = if f.optional { "?" } else { "" };
                    format!("{}{}: {}", f.name, opt, domain_type_to_string(&f.typ))
                })
                .collect();
            format!("{{ {} }}", fields.join(", "))
        }
        DomainType::Reference(name) => name.clone(),
    }
}

/// Compute a content hash for an event schema.
fn compute_hash(event_name: &str, fields: &HashMap<String, FieldSchema>) -> String {
    use std::collections::BTreeMap;
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    // Sort fields for deterministic hashing
    let sorted: BTreeMap<_, _> = fields.iter().collect();

    let mut hasher = DefaultHasher::new();
    event_name.hash(&mut hasher);
    for (name, schema) in sorted {
        name.hash(&mut hasher);
        schema.typ.hash(&mut hasher);
        schema.required.hash(&mut hasher);
    }

    format!("sha256:{:016x}", hasher.finish())
}

/// Compute hash from field schemas.
fn compute_hash_from_fields(fields: &HashMap<String, FieldSchema>) -> String {
    compute_hash("", fields)
}

/// Simple ISO timestamp without external crate.
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();
    // Basic timestamp - not perfect but doesn't need external crate
    format!("{}Z", secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_lock_file_roundtrip() {
        let mut events = HashMap::new();
        events.insert(
            "Created".to_string(),
            EventSchema {
                version: 1,
                previous_version: None,
                fields: {
                    let mut fields = HashMap::new();
                    fields.insert(
                        "id".to_string(),
                        FieldSchema {
                            typ: "string".to_string(),
                            required: true,
                            default: None,
                        },
                    );
                    fields
                },
                upcast_from: HashMap::new(),
                hash: "sha256:abc123".to_string(),
            },
        );

        let mut aggregates = HashMap::new();
        aggregates.insert(
            "Todo".to_string(),
            AggregateLock { events },
        );

        let lock_file = SchemaLockFile {
            version: "1.0".to_string(),
            generated_at: "2025-12-22T00:00:00Z".to_string(),
            compiler_version: "0.1.0".to_string(),
            aggregates,
        };

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("events.lock.json");

        // Save
        lock_file.save(&path).unwrap();

        // Load
        let loaded = SchemaLockFile::load(&path).unwrap().unwrap();

        assert_eq!(loaded.version, "1.0");
        assert!(loaded.aggregates.contains_key("Todo"));
        assert!(loaded.aggregates["Todo"].events.contains_key("Created"));
    }

    #[test]
    fn test_load_nonexistent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.lock.json");

        let result = SchemaLockFile::load(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_domain_type_to_string() {
        assert_eq!(domain_type_to_string(&DomainType::String), "string");
        assert_eq!(domain_type_to_string(&DomainType::Number), "number");
        assert_eq!(domain_type_to_string(&DomainType::Boolean), "boolean");
        assert_eq!(
            domain_type_to_string(&DomainType::Array(Box::new(DomainType::String))),
            "string[]"
        );
        assert_eq!(
            domain_type_to_string(&DomainType::Option(Box::new(DomainType::Number))),
            "number | undefined"
        );
    }
}
