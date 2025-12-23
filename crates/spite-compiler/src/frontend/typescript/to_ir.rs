//! Convert TypeScript AST to language-agnostic IR.

use std::path::{Path, PathBuf};
use crate::diagnostic::CompilerError;
use crate::ir::{
    AggregateIR, CommandIR, DomainIR, DomainType, EventTypeIR, EventVariant, EventField,
    FieldDef, InitialValue, ObjectType, ParameterIR,
    StatementIR, ExpressionIR, BinaryOp, UnaryOp,
    // Projection types
    ProjectionIR, ProjectionKind, ProjectionSchema, QueryMethodIR,
    SubscribedEvent, ColumnDef, IndexDef, SqlType, StateShape, TimeSeriesSignals,
    is_time_related_name, is_range_param,
    TIMESTAMP_FIELDS, TIME_STRING_METHODS,
};
use super::ast::*;

/// Converts parsed TypeScript files to domain IR.
pub fn to_ir(files: &[ParsedFile], source_dir: PathBuf) -> Result<DomainIR, CompilerError> {
    let mut domain = DomainIR::new(source_dir);

    // Collect all event types and state types from all files
    let all_event_types: Vec<_> = files
        .iter()
        .flat_map(|f| f.type_aliases.iter())
        .filter(|t| t.name.ends_with("Event"))
        .collect();

    let all_state_types: Vec<_> = files
        .iter()
        .flat_map(|f| f.type_aliases.iter())
        .filter(|t| t.name.ends_with("State"))
        .collect();

    // Find aggregate and projection classes across all files
    for file in files {
        for class in &file.classes {
            if is_aggregate(class) {
                let aggregate = convert_aggregate(class, &all_event_types, &all_state_types, &file.path)?;
                domain.aggregates.push(aggregate);
            } else if is_projection(class) {
                let projection = convert_projection(class, &all_event_types, &file.path)?;
                domain.projections.push(projection);
            }
        }
    }

    // Only require aggregates if no projections are found either
    if domain.aggregates.is_empty() && domain.projections.is_empty() {
        return Err(CompilerError::NoAggregates);
    }

    Ok(domain)
}

/// Checks if a class is an aggregate (has initialState, events, emit, apply).
fn is_aggregate(class: &ClassDecl) -> bool {
    let has_initial_state = class.properties.iter().any(|p| p.name == "initialState" && p.is_static);
    let has_events = class.properties.iter().any(|p| p.name == "events");
    let has_emit = class.methods.iter().any(|m| m.name == "emit");
    let has_apply = class.methods.iter().any(|m| m.name == "apply");

    has_initial_state && has_events && has_emit && has_apply
}

/// Converts a class declaration to an AggregateIR.
fn convert_aggregate(
    class: &ClassDecl,
    event_types: &[&TypeAlias],
    state_types: &[&TypeAlias],
    source_path: &Path,
) -> Result<AggregateIR, CompilerError> {
    let name = class.name.trim_end_matches("Aggregate").to_string();

    // Find matching event type
    let event_type_name = format!("{}Event", name);
    let event_type = event_types
        .iter()
        .find(|t| t.name == event_type_name)
        .ok_or_else(|| CompilerError::MissingMember {
            member: event_type_name.clone(),
            aggregate: class.name.clone(),
        })?;

    // Find matching state type
    let state_type_name = format!("{}State", name);
    let state_type = state_types
        .iter()
        .find(|t| t.name == state_type_name)
        .ok_or_else(|| CompilerError::MissingMember {
            member: state_type_name.clone(),
            aggregate: class.name.clone(),
        })?;

    // Convert event type
    let events = convert_event_type(event_type)?;

    // Convert state type
    let state = convert_state_type(state_type)?;

    // Extract initial state values
    let initial_state = extract_initial_state(class);

    // Extract commands (public methods that aren't emit, apply, constructor, or getters/setters)
    let commands = class
        .methods
        .iter()
        .filter(|m| {
            m.visibility == Visibility::Public
                && !matches!(m.name.as_str(), "emit" | "apply" | "constructor")
                && !m.name.starts_with("get_")
                && !m.name.starts_with("set_")
        })
        .map(convert_command)
        .collect::<Result<Vec<_>, _>>()?;

    // Extract raw apply body for TS→TS pass-through
    let raw_apply_body = class
        .methods
        .iter()
        .find(|m| m.name == "apply")
        .and_then(|m| m.raw_body.clone());

    Ok(AggregateIR {
        name,
        source_path: source_path.to_path_buf(),
        state,
        initial_state,
        events,
        commands,
        raw_apply_body,
    })
}

/// Converts a type alias to an EventTypeIR.
fn convert_event_type(type_alias: &TypeAlias) -> Result<EventTypeIR, CompilerError> {
    let variants = match &type_alias.type_node {
        TypeNode::Union(members) => {
            members
                .iter()
                .map(convert_event_variant)
                .collect::<Result<Vec<_>, _>>()?
        }
        TypeNode::ObjectLiteral(props) => {
            // Single event type
            vec![convert_object_to_variant(props)?]
        }
        _ => {
            return Err(CompilerError::InvalidEventType {
                type_name: type_alias.name.clone(),
            });
        }
    };

    Ok(EventTypeIR {
        name: type_alias.name.clone(),
        variants,
    })
}

/// Converts a union member to an event variant.
fn convert_event_variant(type_node: &TypeNode) -> Result<EventVariant, CompilerError> {
    match type_node {
        TypeNode::ObjectLiteral(props) => convert_object_to_variant(props),
        _ => Err(CompilerError::InvalidEventType {
            type_name: "union member".to_string(),
        }),
    }
}

/// Converts an object literal to an event variant.
fn convert_object_to_variant(props: &[ObjectProperty]) -> Result<EventVariant, CompilerError> {
    // Find the "type" discriminant field
    let type_prop = props
        .iter()
        .find(|p| p.name == "type")
        .ok_or_else(|| CompilerError::InvalidEventType {
            type_name: "missing type discriminant".to_string(),
        })?;

    // Extract variant name from literal type
    let variant_name = match &type_prop.type_node {
        TypeNode::Primitive(s) => {
            // Remove quotes if present
            s.trim_matches('"').trim_matches('\'').to_string()
        }
        _ => {
            return Err(CompilerError::InvalidEventType {
                type_name: "type must be a string literal".to_string(),
            });
        }
    };

    // Convert other fields
    let fields = props
        .iter()
        .filter(|p| p.name != "type")
        .map(|p| EventField {
            name: p.name.clone(),
            typ: convert_type_node(&p.type_node),
        })
        .collect();

    Ok(EventVariant {
        name: variant_name,
        fields,
    })
}

/// Converts a type alias to an ObjectType for state.
fn convert_state_type(type_alias: &TypeAlias) -> Result<ObjectType, CompilerError> {
    match &type_alias.type_node {
        TypeNode::ObjectLiteral(props) => {
            let fields = props
                .iter()
                .map(|p| FieldDef {
                    name: p.name.clone(),
                    typ: convert_type_node(&p.type_node),
                    optional: p.optional,
                })
                .collect();
            Ok(ObjectType { fields })
        }
        _ => Err(CompilerError::InvalidStateType {
            type_name: type_alias.name.clone(),
        }),
    }
}

/// Converts a TypeNode to a DomainType.
fn convert_type_node(node: &TypeNode) -> DomainType {
    match node {
        TypeNode::Primitive(name) => match name.as_str() {
            "string" => DomainType::String,
            "number" => DomainType::Number,
            "boolean" => DomainType::Boolean,
            _ => DomainType::String, // Default for literals
        },
        TypeNode::Array(inner) => DomainType::Array(Box::new(convert_type_node(inner))),
        TypeNode::Optional(inner) => DomainType::Option(Box::new(convert_type_node(inner))),
        TypeNode::Reference(name) => DomainType::Reference(name.clone()),
        TypeNode::ObjectLiteral(props) => {
            let fields = props
                .iter()
                .map(|p| FieldDef {
                    name: p.name.clone(),
                    typ: convert_type_node(&p.type_node),
                    optional: p.optional,
                })
                .collect();
            DomainType::Object(ObjectType { fields })
        }
        TypeNode::IndexSignature { value_type, .. } => {
            // For index signatures, represent as an object with the value type
            // This is a simplification - the key is handled separately in projection detection
            convert_type_node(value_type)
        }
        TypeNode::Union(members) => {
            // Check if it's T | undefined (optional)
            let non_undefined: Vec<_> = members
                .iter()
                .filter(|m| {
                    !matches!(m, TypeNode::Primitive(s) if s == "undefined")
                })
                .collect();

            if non_undefined.len() == 1 {
                DomainType::Option(Box::new(convert_type_node(non_undefined[0])))
            } else {
                // Complex union - just use first type for now
                convert_type_node(&members[0])
            }
        }
    }
}

/// Extracts initial state values from the class.
fn extract_initial_state(class: &ClassDecl) -> Vec<(String, InitialValue)> {
    let initial_state_prop = class
        .properties
        .iter()
        .find(|p| p.name == "initialState" && p.is_static);

    let mut values = Vec::new();

    if let Some(prop) = initial_state_prop {
        if let Some(init) = &prop.initializer {
            // Parse the initializer to extract field values
            // This is a simplified extraction - a full implementation would
            // parse the object literal properly
            values = parse_initial_state_object(init);
        }
    }

    values
}

/// Parses an initial state object literal string into field values.
fn parse_initial_state_object(init: &str) -> Vec<(String, InitialValue)> {
    let mut values = Vec::new();

    // Simple parsing of object literal
    // Remove outer braces and split by commas
    let content = init.trim().trim_start_matches('{').trim_end_matches('}');

    for pair in content.split(',') {
        let parts: Vec<_> = pair.splitn(2, ':').collect();
        if parts.len() == 2 {
            let key = parts[0].trim().to_string();
            let value = parts[1].trim();

            let init_value = if value == "\"\"" || value == "''" {
                InitialValue::String(String::new())
            } else if value.starts_with('"') || value.starts_with('\'') {
                InitialValue::String(value.trim_matches('"').trim_matches('\'').to_string())
            } else if value == "false" {
                InitialValue::Boolean(false)
            } else if value == "true" {
                InitialValue::Boolean(true)
            } else if value == "null" || value == "undefined" {
                InitialValue::Null
            } else if value == "[]" {
                InitialValue::EmptyArray
            } else if value == "{}" {
                InitialValue::EmptyObject
            } else if let Ok(n) = value.parse::<f64>() {
                InitialValue::Number(n)
            } else {
                InitialValue::Null
            };

            values.push((key, init_value));
        }
    }

    values
}

/// Converts a method declaration to a CommandIR.
fn convert_command(method: &MethodDecl) -> Result<CommandIR, CompilerError> {
    let parameters = method
        .parameters
        .iter()
        .map(|p| ParameterIR {
            name: p.name.clone(),
            typ: p
                .type_node
                .as_ref()
                .map(convert_type_node)
                .unwrap_or(DomainType::String),
        })
        .collect();

    let body = method
        .body
        .iter()
        .filter_map(|s| convert_statement(s).ok())
        .collect();

    Ok(CommandIR {
        name: method.name.clone(),
        parameters,
        body,
        // Default to Internal - access config will be applied later
        access: crate::ir::AccessLevel::Internal,
        roles: Vec::new(),
    })
}

/// Converts a statement to StatementIR.
fn convert_statement(stmt: &Statement) -> Result<StatementIR, CompilerError> {
    match stmt {
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => Ok(StatementIR::If {
            condition: convert_expression(condition)?,
            then_branch: then_branch
                .iter()
                .filter_map(|s| convert_statement(s).ok())
                .collect(),
            else_branch: else_branch.as_ref().map(|stmts| {
                stmts
                    .iter()
                    .filter_map(|s| convert_statement(s).ok())
                    .collect()
            }),
        }),
        Statement::Throw { argument, .. } => {
            // Extract error message from new Error("message")
            let message = extract_error_message(argument);
            Ok(StatementIR::Throw { message })
        }
        Statement::Expression { expression, .. } => {
            // Check if this is a this.emit() call
            if let Some(emit) = try_convert_emit(expression) {
                return Ok(emit);
            }
            Ok(StatementIR::Expression(convert_expression(expression)?))
        }
        Statement::VariableDecl {
            name, initializer, ..
        } => Ok(StatementIR::Let {
            name: name.clone(),
            value: initializer
                .as_ref()
                .map(convert_expression)
                .transpose()?
                .unwrap_or(ExpressionIR::Identifier("undefined".to_string())),
        }),
        Statement::Return { value, .. } => Ok(StatementIR::Return(
            value.as_ref().map(convert_expression).transpose()?,
        )),
        Statement::Block { statements, .. } => {
            // Flatten block into statements
            let stmts: Vec<_> = statements
                .iter()
                .filter_map(|s| convert_statement(s).ok())
                .collect();
            if stmts.len() == 1 {
                Ok(stmts.into_iter().next().unwrap())
            } else {
                // Return first statement or empty expression
                Ok(stmts
                    .into_iter()
                    .next()
                    .unwrap_or(StatementIR::Expression(ExpressionIR::Identifier(
                        "()".to_string(),
                    ))))
            }
        }
        Statement::Switch { .. } => {
            // Switch statements in apply() are handled differently
            // For now, return a placeholder
            Ok(StatementIR::Expression(ExpressionIR::Identifier(
                "match".to_string(),
            )))
        }
    }
}

/// Tries to convert a call expression to an emit statement.
fn try_convert_emit(expr: &Expression) -> Option<StatementIR> {
    if let Expression::Call { callee, arguments, .. } = expr {
        if let Expression::MemberAccess { object, property, .. } = callee.as_ref() {
            if let Expression::This { .. } = object.as_ref() {
                if property == "emit" && arguments.len() == 1 {
                    if let Expression::ObjectLiteral { properties, .. } = &arguments[0] {
                        // Find the type field
                        let event_type = properties
                            .iter()
                            .find(|(k, _)| k == "type")
                            .and_then(|(_, v)| {
                                if let Expression::StringLiteral { value, .. } = v {
                                    Some(value.clone())
                                } else {
                                    None
                                }
                            })?;

                        // Convert other fields
                        let fields: Vec<_> = properties
                            .iter()
                            .filter(|(k, _)| k != "type")
                            .filter_map(|(k, v)| {
                                convert_expression(v).ok().map(|e| (k.clone(), e))
                            })
                            .collect();

                        return Some(StatementIR::Emit { event_type, fields });
                    }
                }
            }
        }
    }
    None
}

/// Extracts error message from throw new Error("message").
fn extract_error_message(expr: &Expression) -> String {
    if let Expression::New { arguments, .. } = expr {
        if let Some(Expression::StringLiteral { value, .. }) = arguments.first() {
            return value.clone();
        }
    }
    "Unknown error".to_string()
}

/// Converts an expression to ExpressionIR.
fn convert_expression(expr: &Expression) -> Result<ExpressionIR, CompilerError> {
    match expr {
        Expression::StringLiteral { value, .. } => Ok(ExpressionIR::StringLiteral(value.clone())),
        Expression::NumberLiteral { value, .. } => Ok(ExpressionIR::NumberLiteral(*value)),
        Expression::BooleanLiteral { value, .. } => Ok(ExpressionIR::BooleanLiteral(*value)),
        Expression::Identifier { name, .. } => Ok(ExpressionIR::Identifier(name.clone())),
        Expression::This { .. } => Ok(ExpressionIR::Identifier("self".to_string())),
        Expression::MemberAccess { object, property, .. } => {
            // Check for this.state.field pattern
            if let Expression::MemberAccess {
                object: inner_obj,
                property: inner_prop,
                ..
            } = object.as_ref()
            {
                if let Expression::This { .. } = inner_obj.as_ref() {
                    if inner_prop == "state" {
                        return Ok(ExpressionIR::StateAccess(property.clone()));
                    }
                }
            }

            Ok(ExpressionIR::PropertyAccess {
                object: Box::new(convert_expression(object)?),
                property: property.clone(),
            })
        }
        Expression::Call { callee, arguments, .. } => {
            // Check if it's a method call
            if let Expression::MemberAccess { object, property, .. } = callee.as_ref() {
                let args: Vec<_> = arguments
                    .iter()
                    .filter_map(|a| convert_expression(a).ok())
                    .collect();
                return Ok(ExpressionIR::MethodCall {
                    object: Box::new(convert_expression(object)?),
                    method: property.clone(),
                    arguments: args,
                });
            }

            // Regular function call
            let callee_name = if let Expression::Identifier { name, .. } = callee.as_ref() {
                name.clone()
            } else {
                "unknown".to_string()
            };

            let args: Vec<_> = arguments
                .iter()
                .filter_map(|a| convert_expression(a).ok())
                .collect();

            Ok(ExpressionIR::Call {
                callee: callee_name,
                arguments: args,
            })
        }
        Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            let op = match operator.as_str() {
                "===" | "==" => BinaryOp::Eq,
                "!==" | "!=" => BinaryOp::NotEq,
                "<" => BinaryOp::Lt,
                "<=" => BinaryOp::LtEq,
                ">" => BinaryOp::Gt,
                ">=" => BinaryOp::GtEq,
                "&&" => BinaryOp::And,
                "||" => BinaryOp::Or,
                "+" => BinaryOp::Add,
                "-" => BinaryOp::Sub,
                "*" => BinaryOp::Mul,
                "/" => BinaryOp::Div,
                _ => BinaryOp::Eq,
            };

            Ok(ExpressionIR::Binary {
                left: Box::new(convert_expression(left)?),
                operator: op,
                right: Box::new(convert_expression(right)?),
            })
        }
        Expression::Unary {
            operator,
            argument,
            ..
        } => {
            let op = match operator.as_str() {
                "!" => UnaryOp::Not,
                "-" => UnaryOp::Neg,
                _ => UnaryOp::Not,
            };

            Ok(ExpressionIR::Unary {
                operator: op,
                operand: Box::new(convert_expression(argument)?),
            })
        }
        Expression::ObjectLiteral { properties, .. } => {
            let props: Vec<_> = properties
                .iter()
                .filter_map(|(k, v)| convert_expression(v).ok().map(|e| (k.clone(), e)))
                .collect();
            Ok(ExpressionIR::Object(props))
        }
        Expression::ArrayLiteral { elements, .. } => {
            let elems: Vec<_> = elements
                .iter()
                .filter_map(|e| convert_expression(e).ok())
                .collect();
            Ok(ExpressionIR::Array(elems))
        }
        Expression::NullLiteral { .. } => Ok(ExpressionIR::Identifier("None".to_string())),
        Expression::New { callee, arguments, .. } => {
            // Extract callee name
            let callee_name = if let Expression::Identifier { name, .. } = callee.as_ref() {
                name.clone()
            } else {
                "unknown".to_string()
            };

            let args: Vec<_> = arguments
                .iter()
                .filter_map(|a| convert_expression(a).ok())
                .collect();

            Ok(ExpressionIR::New {
                callee: callee_name,
                arguments: args,
            })
        }
        _ => Ok(ExpressionIR::Identifier("unknown".to_string())),
    }
}

/// Applies access configuration from App registration to the domain IR.
///
/// This function merges access configurations parsed from index.ts with
/// the aggregate commands. Each command gets its access level and roles based on:
/// 1. Method-specific configuration (if present)
/// 2. Entity-level defaults (if present)
/// 3. System default (Internal, no roles)
pub fn apply_access_config(domain: &mut DomainIR, app_config: &crate::ir::AppConfig) {
    for aggregate in &mut domain.aggregates {
        // Look for config by aggregate name (with or without "Aggregate" suffix)
        let entity_config = app_config
            .entities
            .get(&aggregate.name)
            .or_else(|| app_config.entities.get(&format!("{}Aggregate", aggregate.name)))
            .cloned()
            .unwrap_or_default();

        // Apply access config to each command
        for cmd in &mut aggregate.commands {
            let method_config = entity_config.resolve_method(&cmd.name);
            cmd.access = method_config.access;
            cmd.roles = method_config.roles;
        }
    }
}

// ============================================================================
// Projection Detection and Conversion
// ============================================================================

/// Checks if a class is a projection.
/// A projection has a build() method and a state property with index signature or object type.
fn is_projection(class: &ClassDecl) -> bool {
    let has_build = class.methods.iter().any(|m| m.name == "build");
    let has_state_property = class.properties.iter().any(|p| {
        // Look for properties with index signature or object type annotation
        if let Some(ref type_node) = p.type_node {
            matches!(type_node, TypeNode::IndexSignature { .. } | TypeNode::ObjectLiteral(_))
        } else {
            false
        }
    });

    has_build && has_state_property
}

/// Converts a class declaration to a ProjectionIR.
fn convert_projection(
    class: &ClassDecl,
    event_types: &[&TypeAlias],
    source_path: &Path,
) -> Result<ProjectionIR, CompilerError> {
    let name = class.name.clone();

    // Find the state property (first property with index signature or object type)
    let state_prop = class.properties.iter().find(|p| {
        if let Some(ref type_node) = p.type_node {
            matches!(type_node, TypeNode::IndexSignature { .. } | TypeNode::ObjectLiteral(_))
        } else {
            false
        }
    }).ok_or_else(|| CompilerError::InvalidProjection {
        name: name.clone(),
        reason: "Missing state property with type annotation".to_string(),
    })?;

    // Analyze state shape and determine projection kind
    let state_type = state_prop.type_node.as_ref().unwrap();
    let state_shape = analyze_state_shape(state_type);
    let time_signals = detect_time_series_signals(class, &state_shape);
    let kind = determine_projection_kind(&state_shape, &time_signals);

    // Extract subscribed events from build method parameter
    let subscribed_events = extract_subscribed_events(class, event_types);

    // Extract schema from state type
    let schema = extract_projection_schema(state_prop, state_type, class)?;

    // Extract query methods (public methods except build, constructor)
    let queries = extract_query_methods(class);

    // Get raw build body
    let raw_build_body = class
        .methods
        .iter()
        .find(|m| m.name == "build")
        .and_then(|m| m.raw_body.clone());

    Ok(ProjectionIR {
        name,
        source_path: source_path.to_path_buf(),
        kind,
        subscribed_events,
        schema,
        queries,
        raw_build_body,
        access: crate::ir::AccessLevel::Internal,
        roles: Vec::new(),
    })
}

/// Analyzes the state type to determine its shape.
fn analyze_state_shape(type_node: &TypeNode) -> StateShape {
    match type_node {
        TypeNode::IndexSignature { key_name, value_type, .. } => {
            // Check if value is an object or a number
            match value_type.as_ref() {
                TypeNode::Primitive(p) if p == "number" => {
                    StateShape::IndexedNumber {
                        key_name: key_name.clone(),
                    }
                }
                TypeNode::ObjectLiteral(_) | TypeNode::Reference(_) => {
                    StateShape::IndexedObject {
                        key_name: key_name.clone(),
                        value_type: convert_type_node(value_type),
                    }
                }
                _ => {
                    // Default to indexed object for other types
                    StateShape::IndexedObject {
                        key_name: key_name.clone(),
                        value_type: convert_type_node(value_type),
                    }
                }
            }
        }
        TypeNode::ObjectLiteral(props) => {
            let fields = props
                .iter()
                .map(|p| (p.name.clone(), convert_type_node(&p.type_node)))
                .collect();
            StateShape::NamedFields { fields }
        }
        _ => {
            // Fallback to named fields with empty list
            StateShape::NamedFields { fields: Vec::new() }
        }
    }
}

/// Detects time-series signals from the class.
fn detect_time_series_signals(class: &ClassDecl, state_shape: &StateShape) -> TimeSeriesSignals {
    let mut signals = TimeSeriesSignals::default();

    // Signal 1: Key derived from timestamp in build() method
    signals.has_timestamp_derivation = has_timestamp_derived_key(class);

    // Signal 2: Key name is time-related (only for indexed number shapes)
    if let StateShape::IndexedNumber { key_name } = state_shape {
        signals.has_time_related_key_name = is_time_related_name(key_name);
    }

    // Signal 3: Has range query methods
    signals.has_range_query_methods = has_range_query_methods(class);

    signals
}

/// Determines the projection kind based on state shape and time signals.
fn determine_projection_kind(state_shape: &StateShape, time_signals: &TimeSeriesSignals) -> ProjectionKind {
    match state_shape {
        StateShape::IndexedObject { .. } => ProjectionKind::DenormalizedView,
        StateShape::IndexedNumber { .. } => {
            if time_signals.any() {
                ProjectionKind::TimeSeries
            } else {
                ProjectionKind::Aggregator
            }
        }
        StateShape::NamedFields { .. } => ProjectionKind::Aggregator,
    }
}

/// Detects if the key in build() is derived from a timestamp field.
fn has_timestamp_derived_key(class: &ClassDecl) -> bool {
    let build_method = match class.methods.iter().find(|m| m.name == "build") {
        Some(m) => m,
        None => return false,
    };

    // Look for variable declarations with timestamp derivation patterns
    for stmt in &build_method.body {
        if let Statement::VariableDecl { initializer: Some(init), .. } = stmt {
            if is_timestamp_derivation(init) {
                return true;
            }
        }
    }
    false
}

/// Checks if an expression is a timestamp derivation pattern.
fn is_timestamp_derivation(expr: &Expression) -> bool {
    // Pattern: event.timestamp.slice(...) or event.date.toISOString()
    if let Expression::Call { callee, .. } = expr {
        if let Expression::MemberAccess { object, property, .. } = callee.as_ref() {
            // Check if it's a string method (slice, substring, toISOString, etc.)
            let is_string_method = TIME_STRING_METHODS.iter()
                .any(|m| property.to_lowercase().contains(&m.to_lowercase()));

            if is_string_method {
                // Check if the object is accessing a timestamp field
                if let Expression::MemberAccess { property: inner_prop, .. } = object.as_ref() {
                    return TIMESTAMP_FIELDS.iter()
                        .any(|f| inner_prop.to_lowercase().contains(&f.to_lowercase()));
                }
                // Could be chained like event.date.toISOString().slice(...)
                if let Expression::Call { callee: inner_callee, .. } = object.as_ref() {
                    if let Expression::MemberAccess { object: inner_obj, property: inner_prop, .. } = inner_callee.as_ref() {
                        let is_iso_string = inner_prop.to_lowercase() == "toisostring";
                        if is_iso_string {
                            if let Expression::MemberAccess { property: field_prop, .. } = inner_obj.as_ref() {
                                return TIMESTAMP_FIELDS.iter()
                                    .any(|f| field_prop.to_lowercase().contains(&f.to_lowercase()));
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Checks if the class has range query methods.
fn has_range_query_methods(class: &ClassDecl) -> bool {
    class.methods.iter().any(|m| {
        m.visibility == Visibility::Public &&
        m.name != "build" &&
        m.name != "constructor" &&
        m.parameters.iter().any(|p| is_range_param(&p.name))
    })
}

/// Extracts subscribed events from build method parameter.
fn extract_subscribed_events(class: &ClassDecl, _event_types: &[&TypeAlias]) -> Vec<SubscribedEvent> {
    let build_method = match class.methods.iter().find(|m| m.name == "build") {
        Some(m) => m,
        None => return Vec::new(),
    };

    // Get the first parameter's type (the event union)
    let event_param = match build_method.parameters.first() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let type_node = match &event_param.type_node {
        Some(t) => t,
        None => return Vec::new(),
    };

    // Extract event names from union type
    let event_names = extract_event_names_from_type(type_node);

    event_names
        .into_iter()
        .map(|name| {
            // Try to infer aggregate from event name (e.g., "UserCreated" -> "User")
            let aggregate = infer_aggregate_from_event(&name);
            SubscribedEvent {
                event_name: name,
                aggregate,
            }
        })
        .collect()
}

/// Extracts event type names from a type node.
fn extract_event_names_from_type(type_node: &TypeNode) -> Vec<String> {
    match type_node {
        TypeNode::Union(members) => {
            members
                .iter()
                .flat_map(|m| extract_event_names_from_type(m))
                .collect()
        }
        TypeNode::Reference(name) => vec![name.clone()],
        TypeNode::ObjectLiteral(props) => {
            // Try to extract event name from type field
            props
                .iter()
                .find(|p| p.name == "type")
                .map(|p| {
                    if let TypeNode::Primitive(s) = &p.type_node {
                        s.trim_matches('"').trim_matches('\'').to_string()
                    } else {
                        String::new()
                    }
                })
                .filter(|s| !s.is_empty())
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Infers the aggregate name from an event name.
fn infer_aggregate_from_event(event_name: &str) -> Option<String> {
    // Common event name patterns: UserCreated, OrderCompleted, etc.
    // Try to extract the entity name
    let suffixes = ["Created", "Updated", "Deleted", "Completed", "Cancelled", "Added", "Removed", "Changed"];
    for suffix in suffixes {
        if event_name.ends_with(suffix) {
            return Some(event_name[..event_name.len() - suffix.len()].to_string());
        }
    }
    None
}

/// Extracts projection schema from state property.
fn extract_projection_schema(
    state_prop: &PropertyDecl,
    state_type: &TypeNode,
    class: &ClassDecl,
) -> Result<ProjectionSchema, CompilerError> {
    let state_property_name = state_prop.name.clone();

    let (primary_keys, columns) = match state_type {
        TypeNode::IndexSignature { key_name, value_type, .. } => {
            // Primary key is the index key
            let pk = ColumnDef {
                name: to_snake_case(key_name),
                sql_type: SqlType::Text,
                nullable: false,
                default: None,
            };

            // Columns from value type
            let cols = extract_columns_from_type(value_type);

            (vec![pk], cols)
        }
        TypeNode::ObjectLiteral(props) => {
            // For aggregators with named fields, use a singleton key
            let pk = ColumnDef {
                name: "id".to_string(),
                sql_type: SqlType::Text,
                nullable: false,
                default: Some("'singleton'".to_string()),
            };

            let cols: Vec<ColumnDef> = props
                .iter()
                .map(|p| ColumnDef {
                    name: to_snake_case(&p.name),
                    sql_type: SqlType::from_domain_type(&convert_type_node(&p.type_node)),
                    nullable: p.optional,
                    default: None,
                })
                .collect();

            (vec![pk], cols)
        }
        _ => {
            return Err(CompilerError::InvalidProjection {
                name: state_prop.name.clone(),
                reason: "Unsupported state type shape".to_string(),
            });
        }
    };

    // Derive indexes from query method parameters
    let indexes = derive_indexes_from_queries(class, &columns);

    Ok(ProjectionSchema {
        state_property_name,
        primary_keys,
        columns,
        indexes,
    })
}

/// Extracts column definitions from a type node.
fn extract_columns_from_type(type_node: &TypeNode) -> Vec<ColumnDef> {
    match type_node {
        TypeNode::ObjectLiteral(props) => {
            props
                .iter()
                .map(|p| ColumnDef {
                    name: to_snake_case(&p.name),
                    sql_type: SqlType::from_domain_type(&convert_type_node(&p.type_node)),
                    nullable: p.optional,
                    default: None,
                })
                .collect()
        }
        TypeNode::Primitive(p) if p == "number" => {
            // For { [key]: number } - just a value column
            vec![ColumnDef {
                name: "value".to_string(),
                sql_type: SqlType::Real,
                nullable: false,
                default: None,
            }]
        }
        _ => Vec::new(),
    }
}

/// Derives indexes from query method parameters.
fn derive_indexes_from_queries(class: &ClassDecl, columns: &[ColumnDef]) -> Vec<IndexDef> {
    let mut indexes = Vec::new();
    let column_names: Vec<_> = columns.iter().map(|c| c.name.clone()).collect();

    for method in &class.methods {
        if method.visibility != Visibility::Public
            || method.name == "build"
            || method.name == "constructor"
        {
            continue;
        }

        for param in &method.parameters {
            let param_snake = to_snake_case(&param.name);
            if column_names.contains(&param_snake) {
                // Check if we already have this index
                if !indexes.iter().any(|idx: &IndexDef| idx.columns == vec![param_snake.clone()]) {
                    indexes.push(IndexDef {
                        name: format!("idx_{}", param_snake),
                        columns: vec![param_snake],
                        unique: false,
                    });
                }
            }
        }
    }

    indexes
}

/// Extracts query methods from a class.
fn extract_query_methods(class: &ClassDecl) -> Vec<QueryMethodIR> {
    class
        .methods
        .iter()
        .filter(|m| {
            m.visibility == Visibility::Public
                && m.name != "build"
                && m.name != "constructor"
                && !m.name.starts_with("get_")
                && !m.name.starts_with("set_")
        })
        .map(|m| {
            let parameters = m
                .parameters
                .iter()
                .map(|p| ParameterIR {
                    name: p.name.clone(),
                    typ: p
                        .type_node
                        .as_ref()
                        .map(convert_type_node)
                        .unwrap_or(DomainType::String),
                })
                .collect();

            let return_type = m.return_type.as_ref().map(convert_type_node);

            let indexed_columns: Vec<String> = m
                .parameters
                .iter()
                .map(|p| to_snake_case(&p.name))
                .collect();

            let is_range_query = m.parameters.iter().any(|p| is_range_param(&p.name));

            QueryMethodIR {
                name: m.name.clone(),
                parameters,
                return_type,
                indexed_columns,
                is_range_query,
                raw_body: m.raw_body.clone(),
            }
        })
        .collect()
}

/// Converts a camelCase string to snake_case.
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
