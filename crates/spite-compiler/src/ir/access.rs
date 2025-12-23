//! Access control configuration for aggregate and orchestrator methods.
//!
//! This module defines the access levels and configuration types used to
//! control how methods are exposed via the generated API.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Application mode controlling schema evolution behavior.
///
/// This determines how the compiler handles changes to event schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AppMode {
    /// Development mode - schemas can change freely without constraints.
    /// Use this when building new features or during early development.
    #[default]
    Greenfield,

    /// Locked mode - event schemas are captured in a lock file.
    /// Breaking changes are rejected, non-breaking changes auto-generate upcasts.
    /// Switch to this before deploying to production.
    Production,
}

impl AppMode {
    /// Parse an app mode from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "greenfield" => Some(AppMode::Greenfield),
            "production" => Some(AppMode::Production),
            _ => None,
        }
    }

    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            AppMode::Greenfield => "greenfield",
            AppMode::Production => "production",
        }
    }
}

/// Access level for an endpoint.
///
/// Determines what authentication/authorization is required to call a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccessLevel {
    /// No authentication required - anyone can call this endpoint.
    Public,

    /// Requires authentication and system-tenant membership.
    /// Used for platform/internal endpoints (dashboards, metrics, admin tools).
    #[default]
    Internal,

    /// Requires authentication and tenant membership.
    /// Standard access level for tenant-scoped operations.
    Private,
}

impl AccessLevel {
    /// Parse an access level from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "public" => Some(AccessLevel::Public),
            "internal" => Some(AccessLevel::Internal),
            "private" => Some(AccessLevel::Private),
            _ => None,
        }
    }

    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessLevel::Public => "public",
            AccessLevel::Internal => "internal",
            AccessLevel::Private => "private",
        }
    }
}

/// Access configuration for a single method.
#[derive(Debug, Clone, Default)]
pub struct MethodAccessConfig {
    /// The access level for this method.
    pub access: AccessLevel,

    /// Required roles to access this method.
    /// Only applicable for `Internal` and `Private` access levels.
    pub roles: Vec<String>,
}

/// Access configuration for an aggregate or orchestrator.
#[derive(Debug, Clone, Default)]
pub struct EntityAccessConfig {
    /// Default access level for all methods on this entity.
    pub access: AccessLevel,

    /// Default required roles for all methods on this entity.
    pub roles: Vec<String>,

    /// Per-method configuration overrides.
    pub methods: HashMap<String, MethodAccessConfig>,
}

impl EntityAccessConfig {
    /// Resolve the effective access configuration for a method.
    ///
    /// Method-level configuration overrides entity-level defaults.
    pub fn resolve_method(&self, method_name: &str) -> MethodAccessConfig {
        if let Some(method_config) = self.methods.get(method_name) {
            // Method config overrides entity defaults
            MethodAccessConfig {
                access: method_config.access,
                roles: if method_config.roles.is_empty() {
                    self.roles.clone()
                } else {
                    method_config.roles.clone()
                },
            }
        } else {
            // Use entity defaults
            MethodAccessConfig {
                access: self.access,
                roles: self.roles.clone(),
            }
        }
    }
}

/// Configuration parsed from App registration in index.ts.
#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    /// Application mode controlling schema evolution.
    pub mode: AppMode,

    /// Whether API versioning is enabled.
    /// When true, routes are prefixed with version and contract changes are locked.
    pub api_versioning: bool,

    /// Access configurations keyed by entity name (aggregate or orchestrator).
    pub entities: HashMap<String, EntityAccessConfig>,
}

impl AppConfig {
    /// Get the access configuration for an entity, or default if not configured.
    pub fn get_entity_config(&self, name: &str) -> EntityAccessConfig {
        self.entities.get(name).cloned().unwrap_or_default()
    }
}
