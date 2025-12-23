//! Parser for App registration configuration from index.ts.
//!
//! This module parses the FastAPI-style App registration to extract
//! access configuration for aggregates and orchestrators.
//!
//! # Example
//!
//! ```typescript
//! // index.ts
//! const app = new App();
//!
//! app.register(OrderAggregate, {
//!   access: 'private',
//!   roles: ['user'],
//!   methods: {
//!     create: { access: 'public' },
//!     cancel: { access: 'internal', roles: ['admin'] }
//!   }
//! });
//!
//! app.start();
//! ```

use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Node, Parser};

use crate::diagnostic::CompilerError;
use crate::ir::{AccessLevel, AppConfig, AppMode, EntityAccessConfig, MethodAccessConfig};

/// Parses App configuration from index.ts in the given source directory.
///
/// Returns `None` if no index.ts exists or no App registration is found.
pub fn parse_app_config(source_dir: &Path) -> Result<Option<AppConfig>, CompilerError> {
    let index_path = source_dir.join("index.ts");
    if !index_path.exists() {
        return Ok(None);
    }

    let source = std::fs::read_to_string(&index_path).map_err(|e| CompilerError::IoError {
        path: index_path.clone(),
        message: e.to_string(),
    })?;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .map_err(|_| CompilerError::ParserInitFailed)?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| CompilerError::ParseFailed {
            path: index_path.clone(),
        })?;

    let root = tree.root_node();
    let mut extractor = AppConfigExtractor::new(&source);
    extractor.visit_program(root);

    // Return config if we found an App registration (even without entity registrations)
    if extractor.app_var.is_some() || !extractor.entities.is_empty() {
        Ok(Some(AppConfig {
            mode: extractor.mode,
            api_versioning: extractor.api_versioning,
            entities: extractor.entities,
        }))
    } else {
        Ok(None)
    }
}

/// Extracts App configuration from tree-sitter AST.
struct AppConfigExtractor<'a> {
    source: &'a str,
    entities: HashMap<String, EntityAccessConfig>,
    /// Variable name holding the App instance (e.g., "app")
    app_var: Option<String>,
    /// Application mode (greenfield or production)
    mode: AppMode,
    /// Whether API versioning is enabled
    api_versioning: bool,
}

impl<'a> AppConfigExtractor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            entities: HashMap::new(),
            app_var: None,
            mode: AppMode::Greenfield,
            api_versioning: false,
        }
    }

    fn node_text(&self, node: Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn visit_program(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "lexical_declaration" | "variable_declaration" => {
                    self.visit_variable_declaration(child);
                }
                "expression_statement" => {
                    self.visit_expression_statement(child);
                }
                _ => {}
            }
        }
    }

    /// Look for: const app = new App()
    fn visit_variable_declaration(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                self.visit_variable_declarator(child);
            }
        }
    }

    fn visit_variable_declarator(&mut self, node: Node) {
        let name_node = node.child_by_field_name("name");
        let value_node = node.child_by_field_name("value");

        if let (Some(name), Some(value)) = (name_node, value_node) {
            if value.kind() == "new_expression" {
                // Check if this is new App()
                if let Some(constructor) = value.child_by_field_name("constructor") {
                    let constructor_name = self.node_text(constructor);
                    if constructor_name == "App" {
                        self.app_var = Some(self.node_text(name).to_string());

                        // Parse constructor arguments: new App({ mode: '...', apiVersioning: ... })
                        if let Some(args) = value.child_by_field_name("arguments") {
                            self.parse_app_constructor_args(args);
                        }
                    }
                }
            }
        }
    }

    /// Parse App constructor arguments: new App({ mode: 'production', apiVersioning: true })
    fn parse_app_constructor_args(&mut self, args_node: Node) {
        let mut cursor = args_node.walk();
        for child in args_node.children(&mut cursor) {
            if child.kind() == "object" {
                self.parse_app_config_object(child);
            }
        }
    }

    /// Parse the config object passed to App constructor
    fn parse_app_config_object(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "pair" {
                let key_node = child.child_by_field_name("key");
                let value_node = child.child_by_field_name("value");

                if let (Some(key), Some(value)) = (key_node, value_node) {
                    let key_name = self.node_text(key).trim_matches(|c| c == '"' || c == '\'');

                    match key_name {
                        "mode" => {
                            self.mode = self.parse_app_mode(value);
                        }
                        "apiVersioning" => {
                            self.api_versioning = self.parse_boolean(value);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Parse: 'greenfield' | 'production'
    fn parse_app_mode(&self, node: Node) -> AppMode {
        let text = self.node_text(node).trim_matches(|c| c == '"' || c == '\'');
        AppMode::from_str(text).unwrap_or(AppMode::Greenfield)
    }

    /// Parse a boolean literal: true | false
    fn parse_boolean(&self, node: Node) -> bool {
        let text = self.node_text(node);
        text == "true"
    }

    /// Look for: app.register(EntityClass, { ... })
    fn visit_expression_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "call_expression" {
                self.visit_call_expression(child);
            }
        }
    }

    fn visit_call_expression(&mut self, node: Node) {
        let function_node = node.child_by_field_name("function");
        let arguments_node = node.child_by_field_name("arguments");

        if let Some(func) = function_node {
            if func.kind() == "member_expression" {
                let object = func.child_by_field_name("object");
                let property = func.child_by_field_name("property");

                if let (Some(obj), Some(prop)) = (object, property) {
                    let obj_name = self.node_text(obj);
                    let prop_name = self.node_text(prop);

                    // Check if this is app.register(...)
                    if self.app_var.as_deref() == Some(obj_name) && prop_name == "register" {
                        if let Some(args) = arguments_node {
                            self.parse_register_call(args);
                        }
                    }
                }
            }
        }
    }

    /// Parse: register(EntityClass, { access: '...', roles: [...], methods: { ... } })
    fn parse_register_call(&mut self, args_node: Node) {
        let mut cursor = args_node.walk();
        let children: Vec<Node> = args_node.children(&mut cursor).collect();

        // First argument: entity class name
        let entity_name = children
            .iter()
            .find(|n| n.kind() == "identifier")
            .map(|n| self.node_text(*n).to_string());

        // Second argument: config object (optional)
        let config_obj = children.iter().find(|n| n.kind() == "object");

        if let Some(name) = entity_name {
            let config = if let Some(obj) = config_obj {
                self.parse_entity_config(*obj)
            } else {
                EntityAccessConfig::default()
            };
            self.entities.insert(name, config);
        }
    }

    /// Parse: { access: '...', roles: [...], methods: { ... } }
    fn parse_entity_config(&mut self, node: Node) -> EntityAccessConfig {
        let mut config = EntityAccessConfig::default();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "pair" {
                let key_node = child.child_by_field_name("key");
                let value_node = child.child_by_field_name("value");

                if let (Some(key), Some(value)) = (key_node, value_node) {
                    let key_name = self.node_text(key).trim_matches(|c| c == '"' || c == '\'');

                    match key_name {
                        "access" => {
                            config.access = self.parse_access_level(value);
                        }
                        "roles" => {
                            config.roles = self.parse_string_array(value);
                        }
                        "methods" => {
                            config.methods = self.parse_methods_config(value);
                        }
                        _ => {}
                    }
                }
            }
        }

        config
    }

    /// Parse: 'public' | 'internal' | 'private'
    fn parse_access_level(&self, node: Node) -> AccessLevel {
        let text = self.node_text(node).trim_matches(|c| c == '"' || c == '\'');
        AccessLevel::from_str(text).unwrap_or(AccessLevel::Internal)
    }

    /// Parse: ['role1', 'role2']
    fn parse_string_array(&self, node: Node) -> Vec<String> {
        let mut result = Vec::new();

        if node.kind() == "array" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "string" {
                    let text = self.node_text(child);
                    // Remove quotes
                    let value = text.trim_matches(|c| c == '"' || c == '\'');
                    if !value.is_empty() {
                        result.push(value.to_string());
                    }
                }
            }
        }

        result
    }

    /// Parse: { methodName: { access: '...', roles: [...] }, ... }
    fn parse_methods_config(&mut self, node: Node) -> HashMap<String, MethodAccessConfig> {
        let mut methods = HashMap::new();

        if node.kind() == "object" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "pair" {
                    let key_node = child.child_by_field_name("key");
                    let value_node = child.child_by_field_name("value");

                    if let (Some(key), Some(value)) = (key_node, value_node) {
                        let method_name = self.node_text(key).trim_matches(|c| c == '"' || c == '\'');
                        let config = self.parse_method_config(value);
                        methods.insert(method_name.to_string(), config);
                    }
                }
            }
        }

        methods
    }

    /// Parse: { access: '...', roles: [...] }
    fn parse_method_config(&self, node: Node) -> MethodAccessConfig {
        let mut config = MethodAccessConfig::default();

        if node.kind() == "object" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "pair" {
                    let key_node = child.child_by_field_name("key");
                    let value_node = child.child_by_field_name("value");

                    if let (Some(key), Some(value)) = (key_node, value_node) {
                        let key_name = self.node_text(key).trim_matches(|c| c == '"' || c == '\'');

                        match key_name {
                            "access" => {
                                config.access = self.parse_access_level(value);
                            }
                            "roles" => {
                                config.roles = self.parse_string_array(value);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_dir(content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let index_path = dir.path().join("index.ts");
        fs::write(&index_path, content).unwrap();
        dir
    }

    #[test]
    fn test_parse_basic_app_config() {
        let source = r#"
            const app = new App();
            app.register(OrderAggregate, {
                access: 'private',
                roles: ['user']
            });
        "#;

        let dir = setup_test_dir(source);
        let config = parse_app_config(dir.path()).unwrap().unwrap();

        // Default mode and apiVersioning
        assert_eq!(config.mode, AppMode::Greenfield);
        assert!(!config.api_versioning);

        assert!(config.entities.contains_key("OrderAggregate"));
        let order_config = &config.entities["OrderAggregate"];
        assert_eq!(order_config.access, AccessLevel::Private);
        assert_eq!(order_config.roles, vec!["user"]);
    }

    #[test]
    fn test_parse_production_mode() {
        let source = r#"
            const app = new App({ mode: 'production' });
            app.register(OrderAggregate);
        "#;

        let dir = setup_test_dir(source);
        let config = parse_app_config(dir.path()).unwrap().unwrap();

        assert_eq!(config.mode, AppMode::Production);
        assert!(!config.api_versioning);
    }

    #[test]
    fn test_parse_api_versioning() {
        let source = r#"
            const app = new App({ mode: 'production', apiVersioning: true });
            app.register(OrderAggregate);
        "#;

        let dir = setup_test_dir(source);
        let config = parse_app_config(dir.path()).unwrap().unwrap();

        assert_eq!(config.mode, AppMode::Production);
        assert!(config.api_versioning);
    }

    #[test]
    fn test_parse_greenfield_mode_explicit() {
        let source = r#"
            const app = new App({ mode: 'greenfield' });
            app.register(OrderAggregate);
        "#;

        let dir = setup_test_dir(source);
        let config = parse_app_config(dir.path()).unwrap().unwrap();

        assert_eq!(config.mode, AppMode::Greenfield);
    }

    #[test]
    fn test_parse_method_overrides() {
        let source = r#"
            const app = new App();
            app.register(OrderAggregate, {
                access: 'private',
                methods: {
                    create: { access: 'public' },
                    cancel: { access: 'internal', roles: ['admin'] }
                }
            });
        "#;

        let dir = setup_test_dir(source);
        let config = parse_app_config(dir.path()).unwrap().unwrap();

        let order_config = &config.entities["OrderAggregate"];
        assert_eq!(order_config.methods["create"].access, AccessLevel::Public);
        assert_eq!(order_config.methods["cancel"].access, AccessLevel::Internal);
        assert_eq!(order_config.methods["cancel"].roles, vec!["admin"]);
    }

    #[test]
    fn test_no_index_file() {
        let dir = TempDir::new().unwrap();
        let config = parse_app_config(dir.path()).unwrap();
        assert!(config.is_none());
    }

    #[test]
    fn test_no_app_registration() {
        let source = r#"
            // Just some code, no App
            const x = 5;
        "#;

        let dir = setup_test_dir(source);
        let config = parse_app_config(dir.path()).unwrap();
        assert!(config.is_none());
    }

}
