//! Schema evolution and lock file management.
//!
//! This module handles event schema versioning, change detection, and upcasting
//! for production mode. It ensures safe schema evolution by:
//!
//! 1. Generating lock files that capture event schemas
//! 2. Detecting changes between code and lock file
//! 3. Auto-generating upcasts for non-breaking changes
//! 4. Rejecting breaking changes with helpful errors

pub mod lock;
pub mod diff;
pub mod upcast;

pub use lock::{SchemaLockFile, AggregateLock, EventSchema, FieldSchema, domain_type_to_string_pub};
pub use diff::{SchemaDiff, FieldChange, ChangeType, diff_schemas};
pub use upcast::{UpcastGenerator, UpcastStrategy};
