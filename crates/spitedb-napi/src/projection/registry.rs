//! Projection registry - manages all projection consumers.
//!
//! The registry provides a central point for:
//! - Registering projections
//! - Getting projection instances for direct operations

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use spitedb::SpiteDB;

use super::consumer::{ProjectionConsumer, ProjectionConsumerConfig};
use super::error::ProjectionError;
use super::instance::ProjectionInstance;
use super::operation::ProjectionOp;
use super::schema::ProjectionSchema;
use tokio::sync::Mutex;

/// Registry for all projection consumers.
///
/// Each projection has:
/// - Its own SQLite database file (`{db_dir}/{name}.db`)
/// - Its own consumer for event processing
/// - Independent checkpoint tracking
pub struct ProjectionRegistry {
    /// Base directory for projection databases.
    db_dir: PathBuf,

    /// Reference to the event store.
    event_store: Arc<SpiteDB>,

    /// Registered consumers.
    consumers: HashMap<String, ProjectionConsumer>,
}

impl ProjectionRegistry {
    /// Creates a new projection registry.
    ///
    /// # Arguments
    ///
    /// * `db_dir` - Directory where projection databases will be stored
    /// * `event_store` - Reference to the SpiteDB event store
    pub fn new(db_dir: PathBuf, event_store: Arc<SpiteDB>) -> Result<Self, ProjectionError> {
        // Ensure the directory exists
        std::fs::create_dir_all(&db_dir)?;

        Ok(Self {
            db_dir,
            event_store,
            consumers: HashMap::new(),
        })
    }

    /// Returns the database directory.
    pub fn db_dir(&self) -> &PathBuf {
        &self.db_dir
    }

    /// Returns the number of registered projections.
    pub fn len(&self) -> usize {
        self.consumers.len()
    }

    /// Returns true if no projections are registered.
    pub fn is_empty(&self) -> bool {
        self.consumers.is_empty()
    }

    /// Checks if a projection is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.consumers.contains_key(name)
    }

    /// Returns a list of registered projection names.
    pub fn projection_names(&self) -> Vec<&str> {
        self.consumers.keys().map(|s| s.as_str()).collect()
    }

    // =========================================================================
    // Registration
    // =========================================================================

    /// Registers a new projection.
    ///
    /// Creates the database file and consumer but doesn't start processing.
    pub fn register(
        &mut self,
        name: &str,
        schema: ProjectionSchema,
    ) -> Result<(), ProjectionError> {
        if self.consumers.contains_key(name) {
            return Err(ProjectionError::AlreadyExists(name.to_string()));
        }

        let config =
            ProjectionConsumerConfig::new(name, self.db_dir.clone(), schema);

        let consumer = ProjectionConsumer::new(config, Arc::clone(&self.event_store))?;

        self.consumers.insert(name.to_string(), consumer);

        Ok(())
    }

    /// Unregisters a projection.
    ///
    /// Removes it from the registry. Does NOT delete the database file.
    pub fn unregister(&mut self, name: &str) -> Result<(), ProjectionError> {
        self.consumers
            .remove(name)
            .ok_or_else(|| ProjectionError::NotFound(name.to_string()))?;

        Ok(())
    }

    // =========================================================================
    // JS-Driven Mode Operations
    // =========================================================================

    /// Gets the checkpoint for a projection.
    pub async fn get_checkpoint(&self, name: &str) -> Result<Option<i64>, ProjectionError> {
        let consumer = self
            .consumers
            .get(name)
            .ok_or_else(|| ProjectionError::NotFound(name.to_string()))?;

        consumer.get_checkpoint().await
    }

    /// Reads a row from a projection by tenant_id and primary key.
    ///
    /// Tenant isolation is enforced - only rows matching the tenant_id are returned.
    pub async fn read_row(
        &self,
        name: &str,
        tenant_id: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>, ProjectionError> {
        let consumer = self
            .consumers
            .get(name)
            .ok_or_else(|| ProjectionError::NotFound(name.to_string()))?;

        consumer.read_row(tenant_id, key).await
    }

    /// Gets events for JS processing.
    pub async fn get_events(
        &self,
        name: &str,
        batch_size: usize,
    ) -> Result<Option<(Vec<spitedb::Event>, i64)>, ProjectionError> {
        let consumer = self
            .consumers
            .get(name)
            .ok_or_else(|| ProjectionError::NotFound(name.to_string()))?;

        consumer.get_events(batch_size).await
    }

    /// Applies a batch of operations for JS-driven mode.
    pub async fn apply_batch(
        &self,
        name: &str,
        operations: Vec<ProjectionOp>,
        checkpoint: i64,
    ) -> Result<(), ProjectionError> {
        let consumer = self
            .consumers
            .get(name)
            .ok_or_else(|| ProjectionError::NotFound(name.to_string()))?;

        consumer.apply_batch(operations, checkpoint).await
    }

    /// Gets a projection instance for direct operations.
    ///
    /// Returns None if the projection doesn't exist.
    pub fn get_instance(&self, name: &str) -> Option<Arc<Mutex<ProjectionInstance>>> {
        self.consumers.get(name).map(|c| c.instance())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::operation::OpType;
    use crate::projection::schema::{ColumnDef, ColumnType};
    use spitedb::crypto::BatchCryptor;
    use tempfile::TempDir;

    fn test_cryptor() -> BatchCryptor {
        BatchCryptor::from_env().unwrap()
    }

    fn create_test_schema(name: &str) -> ProjectionSchema {
        // Use new_with_tenant to automatically add tenant_id
        ProjectionSchema::new_with_tenant(
            name.to_string(),
            vec![
                ColumnDef {
                    name: "id".to_string(),
                    col_type: ColumnType::Text,
                    primary_key: true,
                    nullable: false,
                    default_value: None,
                },
                ColumnDef {
                    name: "value".to_string(),
                    col_type: ColumnType::Integer,
                    primary_key: false,
                    nullable: false,
                    default_value: None,
                },
            ],
        )
    }

    #[tokio::test]
    async fn test_registry_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("events.db");
        let event_store = Arc::new(SpiteDB::open_with_cryptor(&db_path, test_cryptor()).await.unwrap());

        let mut registry =
            ProjectionRegistry::new(temp_dir.path().to_path_buf(), event_store).unwrap();

        // Register projections
        registry
            .register("proj_a", create_test_schema("proj_a"))
            .unwrap();
        registry
            .register("proj_b", create_test_schema("proj_b"))
            .unwrap();

        assert_eq!(registry.len(), 2);
        assert!(registry.contains("proj_a"));
        assert!(registry.contains("proj_b"));

        // Check database files created
        assert!(temp_dir.path().join("proj_a.db").exists());
        assert!(temp_dir.path().join("proj_b.db").exists());

        // Unregister
        registry.unregister("proj_a").unwrap();
        assert_eq!(registry.len(), 1);
        assert!(!registry.contains("proj_a"));

        // Database file still exists (we don't delete it)
        assert!(temp_dir.path().join("proj_a.db").exists());
    }

    #[tokio::test]
    async fn test_registry_js_driven_mode_with_tenant() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("events.db");
        let event_store = Arc::new(SpiteDB::open_with_cryptor(&db_path, test_cryptor()).await.unwrap());

        let mut registry =
            ProjectionRegistry::new(temp_dir.path().to_path_buf(), event_store).unwrap();

        registry
            .register("test", create_test_schema("test"))
            .unwrap();

        let tenant_id = "tenant-123";

        // Apply batch with tenant_id
        let ops = vec![ProjectionOp {
            op_type: OpType::Upsert,
            tenant_id: tenant_id.to_string(),
            key: "key1".to_string(),
            value: Some(serde_json::json!({"value": 123})),
        }];
        registry.apply_batch("test", ops, 50).await.unwrap();

        // Check checkpoint
        let checkpoint = registry.get_checkpoint("test").await.unwrap();
        assert_eq!(checkpoint, Some(50));

        // Read row with correct tenant
        let row = registry.read_row("test", tenant_id, "key1").await.unwrap();
        assert!(row.is_some());
        assert_eq!(row.unwrap()["value"], 123);

        // Read with wrong tenant - should NOT find it
        let wrong_row = registry.read_row("test", "other-tenant", "key1").await.unwrap();
        assert!(wrong_row.is_none());
    }

    #[tokio::test]
    async fn test_registry_tenant_isolation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("events.db");
        let event_store = Arc::new(SpiteDB::open_with_cryptor(&db_path, test_cryptor()).await.unwrap());

        let mut registry =
            ProjectionRegistry::new(temp_dir.path().to_path_buf(), event_store).unwrap();

        registry
            .register("test", create_test_schema("test"))
            .unwrap();

        // Apply data for two tenants
        let ops = vec![
            ProjectionOp {
                op_type: OpType::Upsert,
                tenant_id: "tenant-a".to_string(),
                key: "user-1".to_string(),
                value: Some(serde_json::json!({"value": 100})),
            },
            ProjectionOp {
                op_type: OpType::Upsert,
                tenant_id: "tenant-b".to_string(),
                key: "user-1".to_string(),
                value: Some(serde_json::json!({"value": 200})),
            },
        ];
        registry.apply_batch("test", ops, 100).await.unwrap();

        // Each tenant sees their own data
        let row_a = registry.read_row("test", "tenant-a", "user-1").await.unwrap().unwrap();
        assert_eq!(row_a["value"], 100);

        let row_b = registry.read_row("test", "tenant-b", "user-1").await.unwrap().unwrap();
        assert_eq!(row_b["value"], 200);
    }
}
