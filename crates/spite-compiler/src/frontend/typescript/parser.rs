//! TypeScript parser using tree-sitter.

use std::path::Path;
use tree_sitter::{Node, Parser};

use crate::diagnostic::{CompilerError, Span};
use super::ast::*;

/// TypeScript parser.
pub struct TypeScriptParser {
    parser: Parser,
}

impl TypeScriptParser {
    /// Creates a new TypeScript parser.
    pub fn new() -> Result<Self, CompilerError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .map_err(|_| CompilerError::ParserInitFailed)?;
        Ok(Self { parser })
    }

    /// Parses a TypeScript source file.
    pub fn parse(&mut self, source: &str, path: &Path) -> Result<ParsedFile, CompilerError> {
        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| CompilerError::ParseFailed { path: path.to_path_buf() })?;

        let root = tree.root_node();
        let mut visitor = Visitor::new(source, path);
        visitor.visit_program(root)?;

        Ok(ParsedFile {
            path: path.to_path_buf(),
            imports: visitor.imports,
            type_aliases: visitor.type_aliases,
            classes: visitor.classes,
        })
    }
}

/// AST visitor that extracts declarations from tree-sitter nodes.
struct Visitor<'a> {
    source: &'a str,
    path: &'a Path,
    imports: Vec<ImportDecl>,
    type_aliases: Vec<TypeAlias>,
    classes: Vec<ClassDecl>,
}

impl<'a> Visitor<'a> {
    fn new(source: &'a str, path: &'a Path) -> Self {
        Self {
            source,
            path,
            imports: Vec::new(),
            type_aliases: Vec::new(),
            classes: Vec::new(),
        }
    }

    fn span(&self, node: Node) -> Span {
        Span::new(
            self.path.to_path_buf(),
            node.start_position().row,
            node.start_position().column,
            node.end_position().row,
            node.end_position().column,
        )
    }

    fn node_text(&self, node: Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn visit_program(&mut self, node: Node) -> Result<(), CompilerError> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_statement" => self.visit_import(child)?,
                "export_statement" => self.visit_export(child)?,
                "type_alias_declaration" => {
                    if let Some(alias) = self.visit_type_alias(child, false)? {
                        self.type_aliases.push(alias);
                    }
                }
                "class_declaration" => {
                    if let Some(class) = self.visit_class(child, false)? {
                        self.classes.push(class);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn visit_import(&mut self, node: Node) -> Result<(), CompilerError> {
        let mut source = String::new();
        let mut specifiers = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_clause" => {
                    specifiers = self.visit_import_clause(child)?;
                }
                "string" => {
                    source = self.extract_string_value(child);
                }
                _ => {}
            }
        }

        self.imports.push(ImportDecl {
            specifiers,
            source,
            span: self.span(node),
        });

        Ok(())
    }

    fn visit_import_clause(&self, node: Node) -> Result<Vec<ImportSpecifier>, CompilerError> {
        let mut specifiers = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    specifiers.push(ImportSpecifier {
                        name: self.node_text(child).to_string(),
                        alias: None,
                    });
                }
                "named_imports" => {
                    let mut inner_cursor = child.walk();
                    for import_spec in child.children(&mut inner_cursor) {
                        if import_spec.kind() == "import_specifier" {
                            let spec = self.visit_import_specifier(import_spec)?;
                            specifiers.push(spec);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(specifiers)
    }

    fn visit_import_specifier(&self, node: Node) -> Result<ImportSpecifier, CompilerError> {
        let mut name = String::new();
        let mut alias = None;

        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        if !children.is_empty() {
            name = self.node_text(children[0]).to_string();
        }
        if children.len() >= 3 {
            // name as alias
            alias = Some(self.node_text(children[2]).to_string());
            std::mem::swap(&mut name, alias.as_mut().unwrap());
        }

        Ok(ImportSpecifier { name, alias })
    }

    fn visit_export(&mut self, node: Node) -> Result<(), CompilerError> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_alias_declaration" => {
                    if let Some(alias) = self.visit_type_alias(child, true)? {
                        self.type_aliases.push(alias);
                    }
                }
                "class_declaration" => {
                    if let Some(class) = self.visit_class(child, true)? {
                        self.classes.push(class);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn visit_type_alias(&self, node: Node, exported: bool) -> Result<Option<TypeAlias>, CompilerError> {
        let mut name = String::new();
        let mut type_node = TypeNode::Primitive("unknown".to_string());

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => {
                    // Only set name if not already set (first type_identifier is the name)
                    if name.is_empty() {
                        name = self.node_text(child).to_string();
                    }
                }
                // These are type node kinds we should parse
                "union_type" | "object_type" | "array_type" | "literal_type"
                | "predefined_type" | "parenthesized_type" | "intersection_type"
                | "function_type" | "generic_type" | "tuple_type" | "conditional_type" => {
                    type_node = self.visit_type_node(child)?;
                }
                // Skip punctuation and keywords
                "type" | "=" | ";" | "," | ":" | "export" => {}
                _ => {}
            }
        }

        if name.is_empty() {
            return Ok(None);
        }

        Ok(Some(TypeAlias {
            name,
            type_node,
            exported,
            span: self.span(node),
        }))
    }

    fn visit_type_node(&self, node: Node) -> Result<TypeNode, CompilerError> {
        match node.kind() {
            "predefined_type" => {
                Ok(TypeNode::Primitive(self.node_text(node).to_string()))
            }
            "type_identifier" => {
                let name = self.node_text(node);
                match name {
                    "string" | "number" | "boolean" | "void" | "null" | "undefined" => {
                        Ok(TypeNode::Primitive(name.to_string()))
                    }
                    _ => Ok(TypeNode::Reference(name.to_string()))
                }
            }
            "array_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "[" && child.kind() != "]" {
                        let inner = self.visit_type_node(child)?;
                        return Ok(TypeNode::Array(Box::new(inner)));
                    }
                }
                Ok(TypeNode::Array(Box::new(TypeNode::Primitive("unknown".to_string()))))
            }
            "union_type" => {
                let mut variants = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "|" {
                        let variant = self.visit_type_node(child)?;
                        // Flatten nested unions
                        if let TypeNode::Union(nested) = variant {
                            for v in nested {
                                if let TypeNode::Primitive(ref p) = v {
                                    if p == "undefined" {
                                        continue;
                                    }
                                }
                                variants.push(v);
                            }
                        } else {
                            // Check for T | undefined -> Optional
                            if let TypeNode::Primitive(ref p) = variant {
                                if p == "undefined" {
                                    continue; // Will be handled as optional
                                }
                            }
                            variants.push(variant);
                        }
                    }
                }

                // Check if this is T | undefined
                let has_undefined = node.children(&mut node.walk())
                    .any(|c| self.node_text(c) == "undefined");

                if has_undefined && variants.len() == 1 {
                    Ok(TypeNode::Optional(Box::new(variants.remove(0))))
                } else {
                    Ok(TypeNode::Union(variants))
                }
            }
            "object_type" => {
                let mut properties = Vec::new();
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    if child.kind() == "property_signature" {
                        if let Some(prop) = self.visit_property_signature(child)? {
                            properties.push(prop);
                        }
                    }
                }

                Ok(TypeNode::ObjectLiteral(properties))
            }
            "literal_type" => {
                // String literal types like "Created"
                let text = self.node_text(node);
                Ok(TypeNode::Primitive(text.to_string()))
            }
            "parenthesized_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "(" && child.kind() != ")" {
                        return self.visit_type_node(child);
                    }
                }
                Ok(TypeNode::Primitive("unknown".to_string()))
            }
            _ => {
                // Try to get text as primitive
                Ok(TypeNode::Primitive(self.node_text(node).to_string()))
            }
        }
    }

    fn visit_property_signature(&self, node: Node) -> Result<Option<ObjectProperty>, CompilerError> {
        let mut name = String::new();
        let mut type_node = TypeNode::Primitive("unknown".to_string());
        let mut optional = false;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "property_identifier" => {
                    name = self.node_text(child).to_string();
                }
                "?" => {
                    optional = true;
                }
                "type_annotation" => {
                    let mut inner_cursor = child.walk();
                    for inner_child in child.children(&mut inner_cursor) {
                        if inner_child.kind() != ":" {
                            type_node = self.visit_type_node(inner_child)?;
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        if name.is_empty() {
            return Ok(None);
        }

        Ok(Some(ObjectProperty {
            name,
            type_node,
            optional,
        }))
    }

    fn visit_class(&mut self, node: Node, exported: bool) -> Result<Option<ClassDecl>, CompilerError> {
        let mut name = String::new();
        let mut properties = Vec::new();
        let mut methods = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => {
                    name = self.node_text(child).to_string();
                }
                "class_body" => {
                    let (props, meths) = self.visit_class_body(child)?;
                    properties = props;
                    methods = meths;
                }
                _ => {}
            }
        }

        if name.is_empty() {
            return Ok(None);
        }

        Ok(Some(ClassDecl {
            name,
            properties,
            methods,
            exported,
            span: self.span(node),
        }))
    }

    fn visit_class_body(&mut self, node: Node) -> Result<(Vec<PropertyDecl>, Vec<MethodDecl>), CompilerError> {
        let mut properties = Vec::new();
        let mut methods = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "public_field_definition" | "property_definition" => {
                    if let Some(prop) = self.visit_property_decl(child)? {
                        properties.push(prop);
                    }
                }
                "method_definition" => {
                    if let Some(method) = self.visit_method_decl(child)? {
                        methods.push(method);
                    }
                }
                _ => {}
            }
        }

        Ok((properties, methods))
    }

    fn visit_property_decl(&self, node: Node) -> Result<Option<PropertyDecl>, CompilerError> {
        let mut name = String::new();
        let mut type_node = None;
        let mut is_static = false;
        let mut is_readonly = false;
        let mut initializer = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "static" => is_static = true,
                "readonly" => is_readonly = true,
                "property_identifier" => {
                    name = self.node_text(child).to_string();
                }
                "type_annotation" => {
                    let mut inner_cursor = child.walk();
                    for inner_child in child.children(&mut inner_cursor) {
                        if inner_child.kind() != ":" {
                            type_node = Some(self.visit_type_node(inner_child)?);
                            break;
                        }
                    }
                }
                _ => {
                    // Capture initializer as raw text
                    if !name.is_empty() && initializer.is_none() && child.kind() != "=" {
                        initializer = Some(self.node_text(child).to_string());
                    }
                }
            }
        }

        if name.is_empty() {
            return Ok(None);
        }

        Ok(Some(PropertyDecl {
            name,
            type_node,
            is_static,
            is_readonly,
            initializer,
            span: self.span(node),
        }))
    }

    fn visit_method_decl(&self, node: Node) -> Result<Option<MethodDecl>, CompilerError> {
        let mut name = String::new();
        let mut parameters = Vec::new();
        let mut return_type = None;
        let mut body = Vec::new();
        let mut raw_body = None;
        let mut is_async = false;
        let mut visibility = Visibility::Public;
        let mut is_getter = false;
        let mut is_setter = false;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "async" => is_async = true,
                "accessibility_modifier" => {
                    visibility = match self.node_text(child) {
                        "private" => Visibility::Private,
                        "protected" => Visibility::Protected,
                        _ => Visibility::Public,
                    };
                }
                "get" => {
                    is_getter = true;
                }
                "set" => {
                    is_setter = true;
                }
                "property_identifier" => {
                    // For getters/setters, this is the actual method name
                    // For regular methods, this is also the method name
                    name = self.node_text(child).to_string();
                }
                "formal_parameters" => {
                    parameters = self.visit_parameters(child)?;
                }
                "type_annotation" => {
                    let mut inner_cursor = child.walk();
                    for inner_child in child.children(&mut inner_cursor) {
                        if inner_child.kind() != ":" {
                            return_type = Some(self.visit_type_node(inner_child)?);
                            break;
                        }
                    }
                }
                "statement_block" => {
                    body = self.visit_statement_block(child)?;
                    // Store raw body text for pass-through (useful for apply methods in TS→TS)
                    raw_body = Some(self.node_text(child).to_string());
                }
                _ => {}
            }
        }

        if name.is_empty() {
            return Ok(None);
        }

        // Prefix getter/setter names so they can be filtered
        if is_getter {
            name = format!("get_{}", name);
        } else if is_setter {
            name = format!("set_{}", name);
        }

        Ok(Some(MethodDecl {
            name,
            parameters,
            return_type,
            body,
            raw_body,
            is_async,
            visibility,
            span: self.span(node),
        }))
    }

    fn visit_parameters(&self, node: Node) -> Result<Vec<Parameter>, CompilerError> {
        let mut params = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "required_parameter" || child.kind() == "optional_parameter" {
                if let Some(param) = self.visit_parameter(child)? {
                    params.push(param);
                }
            }
        }

        Ok(params)
    }

    fn visit_parameter(&self, node: Node) -> Result<Option<Parameter>, CompilerError> {
        let mut name = String::new();
        let mut type_node = None;
        let optional = node.kind() == "optional_parameter";
        let mut default_value = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    name = self.node_text(child).to_string();
                }
                "type_annotation" => {
                    let mut inner_cursor = child.walk();
                    for inner_child in child.children(&mut inner_cursor) {
                        if inner_child.kind() != ":" {
                            type_node = Some(self.visit_type_node(inner_child)?);
                            break;
                        }
                    }
                }
                _ => {
                    if child.kind() != "?" && child.kind() != "=" && default_value.is_none() && !name.is_empty() {
                        default_value = Some(self.node_text(child).to_string());
                    }
                }
            }
        }

        if name.is_empty() {
            return Ok(None);
        }

        Ok(Some(Parameter {
            name,
            type_node,
            optional,
            default_value,
        }))
    }

    fn visit_statement_block(&self, node: Node) -> Result<Vec<Statement>, CompilerError> {
        let mut statements = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() != "{" && child.kind() != "}" {
                if let Some(stmt) = self.visit_statement(child)? {
                    statements.push(stmt);
                }
            }
        }

        Ok(statements)
    }

    fn visit_statement(&self, node: Node) -> Result<Option<Statement>, CompilerError> {
        match node.kind() {
            "if_statement" => self.visit_if_statement(node),
            "switch_statement" => self.visit_switch_statement(node),
            "return_statement" => self.visit_return_statement(node),
            "throw_statement" => self.visit_throw_statement(node),
            "expression_statement" => self.visit_expression_statement(node),
            "lexical_declaration" | "variable_declaration" => self.visit_variable_declaration(node),
            "statement_block" => {
                let stmts = self.visit_statement_block(node)?;
                Ok(Some(Statement::Block {
                    statements: stmts,
                    span: self.span(node),
                }))
            }
            _ => Ok(None),
        }
    }

    fn visit_if_statement(&self, node: Node) -> Result<Option<Statement>, CompilerError> {
        let mut condition = None;
        let mut then_branch = Vec::new();
        let mut else_branch = None;

        let mut cursor = node.walk();
        let mut found_condition = false;
        let mut found_then = false;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "parenthesized_expression" => {
                    condition = self.visit_expression(child).ok();
                    found_condition = true;
                }
                "statement_block" | "if_statement" | "return_statement" | "throw_statement" | "expression_statement" => {
                    if !found_then {
                        if child.kind() == "statement_block" {
                            then_branch = self.visit_statement_block(child)?;
                        } else if let Some(stmt) = self.visit_statement(child)? {
                            then_branch = vec![stmt];
                        }
                        found_then = true;
                    } else {
                        // This is the else branch
                        if child.kind() == "statement_block" {
                            else_branch = Some(self.visit_statement_block(child)?);
                        } else if let Some(stmt) = self.visit_statement(child)? {
                            else_branch = Some(vec![stmt]);
                        }
                    }
                }
                "else_clause" => {
                    let mut inner_cursor = child.walk();
                    for inner_child in child.children(&mut inner_cursor) {
                        if inner_child.kind() == "statement_block" {
                            else_branch = Some(self.visit_statement_block(inner_child)?);
                        } else if let Some(stmt) = self.visit_statement(inner_child)? {
                            else_branch = Some(vec![stmt]);
                        }
                    }
                }
                _ => {
                    if !found_condition {
                        condition = self.visit_expression(child).ok();
                        if condition.is_some() {
                            found_condition = true;
                        }
                    }
                }
            }
        }

        let condition = condition.ok_or_else(|| CompilerError::SyntaxError {
            message: "Missing condition in if statement".to_string(),
            file: self.path.to_path_buf(),
            line: node.start_position().row,
            column: node.start_position().column,
        })?;

        Ok(Some(Statement::If {
            condition,
            then_branch,
            else_branch,
            span: self.span(node),
        }))
    }

    fn visit_switch_statement(&self, node: Node) -> Result<Option<Statement>, CompilerError> {
        let mut discriminant = None;
        let mut cases = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "parenthesized_expression" => {
                    discriminant = self.visit_expression(child).ok();
                }
                "switch_body" => {
                    cases = self.visit_switch_body(child)?;
                }
                _ => {}
            }
        }

        let discriminant = discriminant.ok_or_else(|| CompilerError::SyntaxError {
            message: "Missing discriminant in switch statement".to_string(),
            file: self.path.to_path_buf(),
            line: node.start_position().row,
            column: node.start_position().column,
        })?;

        Ok(Some(Statement::Switch {
            discriminant,
            cases,
            span: self.span(node),
        }))
    }

    fn visit_switch_body(&self, node: Node) -> Result<Vec<SwitchCase>, CompilerError> {
        let mut cases = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "switch_case" => {
                    cases.push(self.visit_switch_case(child)?);
                }
                "switch_default" => {
                    cases.push(self.visit_switch_default(child)?);
                }
                _ => {}
            }
        }

        Ok(cases)
    }

    fn visit_switch_case(&self, node: Node) -> Result<SwitchCase, CompilerError> {
        let mut test = None;
        let mut consequent = Vec::new();

        let mut cursor = node.walk();
        let mut past_colon = false;

        for child in node.children(&mut cursor) {
            if child.kind() == ":" {
                past_colon = true;
                continue;
            }

            if !past_colon && child.kind() != "case" {
                test = self.visit_expression(child).ok();
            } else if past_colon {
                if let Some(stmt) = self.visit_statement(child)? {
                    consequent.push(stmt);
                }
            }
        }

        Ok(SwitchCase { test, consequent })
    }

    fn visit_switch_default(&self, node: Node) -> Result<SwitchCase, CompilerError> {
        let mut consequent = Vec::new();
        let mut cursor = node.walk();
        let mut past_colon = false;

        for child in node.children(&mut cursor) {
            if child.kind() == ":" {
                past_colon = true;
                continue;
            }

            if past_colon {
                if let Some(stmt) = self.visit_statement(child)? {
                    consequent.push(stmt);
                }
            }
        }

        Ok(SwitchCase {
            test: None,
            consequent,
        })
    }

    fn visit_return_statement(&self, node: Node) -> Result<Option<Statement>, CompilerError> {
        let mut value = None;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() != "return" && child.kind() != ";" {
                value = self.visit_expression(child).ok();
            }
        }

        Ok(Some(Statement::Return {
            value,
            span: self.span(node),
        }))
    }

    fn visit_throw_statement(&self, node: Node) -> Result<Option<Statement>, CompilerError> {
        let mut argument = None;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() != "throw" && child.kind() != ";" {
                argument = self.visit_expression(child).ok();
            }
        }

        let argument = argument.ok_or_else(|| CompilerError::SyntaxError {
            message: "Missing argument in throw statement".to_string(),
            file: self.path.to_path_buf(),
            line: node.start_position().row,
            column: node.start_position().column,
        })?;

        Ok(Some(Statement::Throw {
            argument,
            span: self.span(node),
        }))
    }

    fn visit_expression_statement(&self, node: Node) -> Result<Option<Statement>, CompilerError> {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() != ";" {
                if let Ok(expr) = self.visit_expression(child) {
                    return Ok(Some(Statement::Expression {
                        expression: expr,
                        span: self.span(node),
                    }));
                }
            }
        }

        Ok(None)
    }

    fn visit_variable_declaration(&self, node: Node) -> Result<Option<Statement>, CompilerError> {
        let mut kind = VarKind::Let;
        let mut name = String::new();
        let mut type_node = None;
        let mut initializer = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "const" => kind = VarKind::Const,
                "let" => kind = VarKind::Let,
                "var" => kind = VarKind::Var,
                "variable_declarator" => {
                    let mut inner_cursor = child.walk();
                    for inner_child in child.children(&mut inner_cursor) {
                        match inner_child.kind() {
                            "identifier" => {
                                name = self.node_text(inner_child).to_string();
                            }
                            "type_annotation" => {
                                let mut type_cursor = inner_child.walk();
                                for type_child in inner_child.children(&mut type_cursor) {
                                    if type_child.kind() != ":" {
                                        type_node = self.visit_type_node(type_child).ok();
                                        break;
                                    }
                                }
                            }
                            _ => {
                                if inner_child.kind() != "=" && initializer.is_none() && !name.is_empty() {
                                    initializer = self.visit_expression(inner_child).ok();
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if name.is_empty() {
            return Ok(None);
        }

        Ok(Some(Statement::VariableDecl {
            kind,
            name,
            type_node,
            initializer,
            span: self.span(node),
        }))
    }

    fn visit_expression(&self, node: Node) -> Result<Expression, CompilerError> {
        match node.kind() {
            "identifier" => Ok(Expression::Identifier {
                name: self.node_text(node).to_string(),
                span: self.span(node),
            }),
            "string" | "template_string" => Ok(Expression::StringLiteral {
                value: self.extract_string_value(node),
                span: self.span(node),
            }),
            "number" => Ok(Expression::NumberLiteral {
                value: self.node_text(node).parse().unwrap_or(0.0),
                span: self.span(node),
            }),
            "true" => Ok(Expression::BooleanLiteral {
                value: true,
                span: self.span(node),
            }),
            "false" => Ok(Expression::BooleanLiteral {
                value: false,
                span: self.span(node),
            }),
            "null" => Ok(Expression::NullLiteral {
                span: self.span(node),
            }),
            "undefined" => Ok(Expression::Identifier {
                name: "undefined".to_string(),
                span: self.span(node),
            }),
            "this" => Ok(Expression::This {
                span: self.span(node),
            }),
            "array" => {
                let mut elements = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "[" && child.kind() != "]" && child.kind() != "," {
                        if let Ok(expr) = self.visit_expression(child) {
                            elements.push(expr);
                        }
                    }
                }
                Ok(Expression::ArrayLiteral {
                    elements,
                    span: self.span(node),
                })
            }
            "object" => {
                let mut properties = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "pair" || child.kind() == "shorthand_property_identifier" {
                        if let Some((key, value)) = self.visit_object_property(child)? {
                            properties.push((key, value));
                        }
                    }
                }
                Ok(Expression::ObjectLiteral {
                    properties,
                    span: self.span(node),
                })
            }
            "member_expression" => {
                let mut object = None;
                let mut property = String::new();
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "property_identifier" => {
                            property = self.node_text(child).to_string();
                        }
                        "." => {}
                        _ => {
                            object = self.visit_expression(child).ok();
                        }
                    }
                }

                let object = object.ok_or_else(|| CompilerError::SyntaxError {
                    message: "Missing object in member expression".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })?;

                Ok(Expression::MemberAccess {
                    object: Box::new(object),
                    property,
                    span: self.span(node),
                })
            }
            "call_expression" => {
                let mut callee = None;
                let mut arguments = Vec::new();
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "arguments" => {
                            let mut arg_cursor = child.walk();
                            for arg in child.children(&mut arg_cursor) {
                                if arg.kind() != "(" && arg.kind() != ")" && arg.kind() != "," {
                                    if let Ok(expr) = self.visit_expression(arg) {
                                        arguments.push(expr);
                                    }
                                }
                            }
                        }
                        _ => {
                            if callee.is_none() {
                                callee = self.visit_expression(child).ok();
                            }
                        }
                    }
                }

                let callee = callee.ok_or_else(|| CompilerError::SyntaxError {
                    message: "Missing callee in call expression".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })?;

                Ok(Expression::Call {
                    callee: Box::new(callee),
                    arguments,
                    span: self.span(node),
                })
            }
            "new_expression" => {
                let mut callee = None;
                let mut arguments = Vec::new();
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "new" => {}
                        "arguments" => {
                            let mut arg_cursor = child.walk();
                            for arg in child.children(&mut arg_cursor) {
                                if arg.kind() != "(" && arg.kind() != ")" && arg.kind() != "," {
                                    if let Ok(expr) = self.visit_expression(arg) {
                                        arguments.push(expr);
                                    }
                                }
                            }
                        }
                        _ => {
                            if callee.is_none() {
                                callee = self.visit_expression(child).ok();
                            }
                        }
                    }
                }

                let callee = callee.ok_or_else(|| CompilerError::SyntaxError {
                    message: "Missing callee in new expression".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })?;

                Ok(Expression::New {
                    callee: Box::new(callee),
                    arguments,
                    span: self.span(node),
                })
            }
            "binary_expression" => {
                let mut left = None;
                let mut operator = String::new();
                let mut right = None;
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    let text = self.node_text(child);
                    if left.is_none() {
                        left = self.visit_expression(child).ok();
                    } else if operator.is_empty() && is_binary_operator(text) {
                        operator = text.to_string();
                    } else if right.is_none() {
                        right = self.visit_expression(child).ok();
                    }
                }

                let left = left.ok_or_else(|| CompilerError::SyntaxError {
                    message: "Missing left operand".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })?;

                let right = right.ok_or_else(|| CompilerError::SyntaxError {
                    message: "Missing right operand".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })?;

                Ok(Expression::Binary {
                    left: Box::new(left),
                    operator,
                    right: Box::new(right),
                    span: self.span(node),
                })
            }
            "unary_expression" => {
                let mut operator = String::new();
                let mut argument = None;
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    let text = self.node_text(child);
                    if is_unary_operator(text) {
                        operator = text.to_string();
                    } else {
                        argument = self.visit_expression(child).ok();
                    }
                }

                let argument = argument.ok_or_else(|| CompilerError::SyntaxError {
                    message: "Missing argument in unary expression".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })?;

                Ok(Expression::Unary {
                    operator,
                    argument: Box::new(argument),
                    prefix: true,
                    span: self.span(node),
                })
            }
            "await_expression" => {
                let mut argument = None;
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    if child.kind() != "await" {
                        argument = self.visit_expression(child).ok();
                    }
                }

                let argument = argument.ok_or_else(|| CompilerError::SyntaxError {
                    message: "Missing argument in await expression".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })?;

                Ok(Expression::Await {
                    argument: Box::new(argument),
                    span: self.span(node),
                })
            }
            "parenthesized_expression" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "(" && child.kind() != ")" {
                        return self.visit_expression(child);
                    }
                }
                Err(CompilerError::SyntaxError {
                    message: "Empty parenthesized expression".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })
            }
            "spread_element" => {
                let mut argument = None;
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    if child.kind() != "..." {
                        argument = self.visit_expression(child).ok();
                    }
                }

                let argument = argument.ok_or_else(|| CompilerError::SyntaxError {
                    message: "Missing argument in spread element".to_string(),
                    file: self.path.to_path_buf(),
                    line: node.start_position().row,
                    column: node.start_position().column,
                })?;

                Ok(Expression::Spread {
                    argument: Box::new(argument),
                    span: self.span(node),
                })
            }
            _ => {
                // Fallback: treat as identifier
                Ok(Expression::Identifier {
                    name: self.node_text(node).to_string(),
                    span: self.span(node),
                })
            }
        }
    }

    fn visit_object_property(&self, node: Node) -> Result<Option<(String, Expression)>, CompilerError> {
        if node.kind() == "shorthand_property_identifier" {
            let name = self.node_text(node).to_string();
            return Ok(Some((
                name.clone(),
                Expression::Identifier {
                    name,
                    span: self.span(node),
                },
            )));
        }

        let mut key = String::new();
        let mut value = None;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "property_identifier" => {
                    // Property identifier is always the key
                    if key.is_empty() {
                        key = self.node_text(child).to_string();
                    }
                }
                ":" => {}
                _ => {
                    // Everything else (including string literals) is parsed as expression value
                    if key.is_empty() {
                        // If we haven't found a key yet, this might be a computed key
                        // For now, try to use it as the key
                        key = self.node_text(child).to_string();
                    } else if value.is_none() {
                        value = self.visit_expression(child).ok();
                    }
                }
            }
        }

        if key.is_empty() {
            return Ok(None);
        }

        let value = value.unwrap_or(Expression::Identifier {
            name: key.clone(),
            span: self.span(node),
        });

        Ok(Some((key, value)))
    }

    fn extract_string_value(&self, node: Node) -> String {
        let text = self.node_text(node);
        // Remove quotes
        if (text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\''))
            || (text.starts_with('`') && text.ends_with('`'))
        {
            text[1..text.len() - 1].to_string()
        } else {
            text.to_string()
        }
    }
}

fn is_binary_operator(s: &str) -> bool {
    matches!(
        s,
        "+" | "-" | "*" | "/" | "%" | "==" | "===" | "!=" | "!==" | "<" | "<=" | ">" | ">=" | "&&" | "||"
    )
}

fn is_unary_operator(s: &str) -> bool {
    matches!(s, "!" | "-" | "+" | "~" | "typeof" | "void" | "delete")
}
