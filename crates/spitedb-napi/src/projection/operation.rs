//! Projection operation types.
//!
//! All operations are tenant-scoped for complete isolation.

use serde_json::Value as JsonValue;

use crate::{BatchResultNapi, ProjectionOpNapi};

/// Operation type for projection updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpType {
    Upsert,
    Delete,
}

/// A single projection operation.
///
/// All operations include tenant_id for framework-enforced tenant isolation.
#[derive(Debug, Clone)]
pub struct ProjectionOp {
    pub op_type: OpType,
    /// Tenant ID for isolation
    pub tenant_id: String,
    /// User-defined primary key
    pub key: String,
    pub value: Option<JsonValue>,
}

impl ProjectionOp {
    /// Creates a new projection operation from NAPI with tenant_id.
    pub fn from_napi_with_tenant(napi: ProjectionOpNapi, tenant_id: String) -> Self {
        Self {
            op_type: match napi.op_type.as_str() {
                "delete" => OpType::Delete,
                _ => OpType::Upsert,
            },
            tenant_id,
            key: napi.key,
            value: napi.value.and_then(|s| serde_json::from_str(&s).ok()),
        }
    }
}

/// Result of processing a batch - operations to apply.
///
/// All operations in a batch share the same tenant_id.
#[derive(Debug)]
pub struct BatchResult {
    pub projection_name: String,
    /// Tenant ID for all operations in this batch
    pub tenant_id: String,
    pub operations: Vec<ProjectionOp>,
    pub last_global_pos: i64,
}

impl BatchResult {
    /// Creates from NAPI batch result with tenant_id.
    pub fn from_napi(napi: BatchResultNapi) -> Self {
        let tenant_id = napi.tenant_id.clone();
        Self {
            projection_name: napi.projection_name,
            tenant_id: tenant_id.clone(),
            operations: napi
                .operations
                .into_iter()
                .map(|op| ProjectionOp::from_napi_with_tenant(op, tenant_id.clone()))
                .collect(),
            last_global_pos: napi.last_global_pos,
        }
    }
}
