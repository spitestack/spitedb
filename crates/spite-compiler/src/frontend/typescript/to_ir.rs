//! Convert TypeScript AST to language-agnostic IR.

use std::path::PathBuf;
use crate::diagnostic::CompilerError;
use crate::ir::{
    AggregateIR, CommandIR, DomainIR, DomainType, EventTypeIR, EventVariant, EventField,
    FieldDef, InitialValue, ObjectType, ParameterIR,
    StatementIR, ExpressionIR, BinaryOp, UnaryOp,
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

    // Find aggregate classes across all files
    for file in files {
        for class in &file.classes {
            if is_aggregate(class) {
                let aggregate = convert_aggregate(class, &all_event_types, &all_state_types, &file.path)?;
                domain.aggregates.push(aggregate);
            }
        }
    }

    if domain.aggregates.is_empty() {
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
    source_path: &PathBuf,
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
        .map(|m| convert_command(m))
        .collect::<Result<Vec<_>, _>>()?;

    // Extract raw apply body for TS→TS pass-through
    let raw_apply_body = class
        .methods
        .iter()
        .find(|m| m.name == "apply")
        .and_then(|m| m.raw_body.clone());

    Ok(AggregateIR {
        name,
        source_path: source_path.clone(),
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
                .map(|m| convert_event_variant(m))
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
