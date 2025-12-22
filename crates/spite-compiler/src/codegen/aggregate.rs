//! Aggregate class code generation for TypeScript.

use crate::ir::{AggregateIR, StatementIR, ExpressionIR, BinaryOp, UnaryOp, DomainType};
use super::ts_types::{to_ts_type, to_snake_case};

/// Generates TypeScript code for an aggregate class.
pub fn generate_aggregate(aggregate: &AggregateIR) -> String {
    let class_name = format!("{}Aggregate", aggregate.name);
    let state_type = format!("{}State", aggregate.name);
    let event_type = format!("{}Event", aggregate.name);
    let initial_state = format!("initial{}State", aggregate.name);

    let mut output = String::new();

    // Imports
    output.push_str(&format!(
        "import type {{ {} }} from '../events/{}.events';\n",
        event_type,
        to_snake_case(&aggregate.name)
    ));
    output.push_str(&format!(
        "import type {{ {} }} from '../state/{}.state';\n",
        state_type,
        to_snake_case(&aggregate.name)
    ));
    output.push_str(&format!(
        "import {{ {} }} from '../state/{}.state';\n\n",
        initial_state,
        to_snake_case(&aggregate.name)
    ));

    // Class definition
    output.push_str(&format!("export class {} {{\n", class_name));
    output.push_str(&format!("  private state: {} = {{ ...{} }};\n", state_type, initial_state));
    output.push_str(&format!("  private pendingEvents: {}[] = [];\n\n", event_type));

    // Static factory method
    output.push_str(&format!("  static create(events: {}[]): {} {{\n", event_type, class_name));
    output.push_str(&format!("    const agg = new {}();\n", class_name));
    output.push_str("    for (const event of events) {\n");
    output.push_str("      agg.applyEvent(event);\n");
    output.push_str("    }\n");
    output.push_str("    return agg;\n");
    output.push_str("  }\n\n");

    // Getters
    output.push_str(&format!("  getState(): Readonly<{}> {{\n", state_type));
    output.push_str("    return this.state;\n");
    output.push_str("  }\n\n");

    output.push_str(&format!("  getPendingEvents(): {}[] {{\n", event_type));
    output.push_str("    return [...this.pendingEvents];\n");
    output.push_str("  }\n\n");

    output.push_str("  clearPendingEvents(): void {\n");
    output.push_str("    this.pendingEvents = [];\n");
    output.push_str("  }\n\n");

    // Emit method
    output.push_str(&format!("  private emit(event: {}): void {{\n", event_type));
    output.push_str("    this.pendingEvents.push(event);\n");
    output.push_str("    this.applyEvent(event);\n");
    output.push_str("  }\n\n");

    // Apply method - use raw body if available (preserves user's apply logic)
    output.push_str(&format!("  private applyEvent(event: {}): void ", event_type));

    if let Some(raw_body) = &aggregate.raw_apply_body {
        // Use the raw apply body from source (preserves user's custom logic)
        output.push_str(raw_body);
        output.push_str("\n\n");
    } else {
        // Auto-generate apply logic based on field matching
        output.push_str("{\n");
        output.push_str("    switch (event.type) {\n");

        for variant in &aggregate.events.variants {
            output.push_str(&format!("      case \"{}\":\n", variant.name));

            // Generate state assignments
            for event_field in &variant.fields {
                // Find matching state field
                if let Some(state_field) = aggregate.state.fields.iter().find(|sf| sf.name == event_field.name) {
                    let needs_some = matches!(&state_field.typ, DomainType::Option(inner)
                        if !matches!(&event_field.typ, DomainType::Option(_))
                        && **inner == event_field.typ);

                    if needs_some {
                        // State field is Option<T>, event field is T
                        output.push_str(&format!(
                            "        this.state.{} = event.{};\n",
                            event_field.name, event_field.name
                        ));
                    } else {
                        output.push_str(&format!(
                            "        this.state.{} = event.{};\n",
                            event_field.name, event_field.name
                        ));
                    }
                }
            }

            output.push_str("        break;\n");
        }

        output.push_str("    }\n");
        output.push_str("  }\n\n");
    }

    // Command methods
    for cmd in &aggregate.commands {
        let params: Vec<String> = cmd
            .parameters
            .iter()
            .map(|p| format!("{}: {}", p.name, to_ts_type(&p.typ)))
            .collect();

        output.push_str(&format!("  {}({}): void {{\n", cmd.name, params.join(", ")));

        for stmt in &cmd.body {
            output.push_str(&generate_statement(stmt, 2));
        }

        output.push_str("  }\n\n");
    }

    output.push_str("}\n");
    output
}

/// Generates TypeScript code for a statement.
fn generate_statement(stmt: &StatementIR, indent: usize) -> String {
    let spaces = "  ".repeat(indent);

    match stmt {
        StatementIR::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = generate_expression(condition);
            let mut output = format!("{}if ({}) {{\n", spaces, cond);

            for s in then_branch {
                output.push_str(&generate_statement(s, indent + 1));
            }

            if let Some(else_stmts) = else_branch {
                output.push_str(&format!("{}}} else {{\n", spaces));
                for s in else_stmts {
                    output.push_str(&generate_statement(s, indent + 1));
                }
            }

            output.push_str(&format!("{}}}\n", spaces));
            output
        }
        StatementIR::Throw { message } => {
            format!("{}throw new Error(\"{}\");\n", spaces, message)
        }
        StatementIR::Emit { event_type, fields } => {
            let field_strs: Vec<String> = fields
                .iter()
                .map(|(name, expr)| format!("{}: {}", name, generate_expression(expr)))
                .collect();

            format!(
                "{}this.emit({{ type: \"{}\", {} }});\n",
                spaces,
                event_type,
                field_strs.join(", ")
            )
        }
        StatementIR::Let { name, value } => {
            format!(
                "{}const {} = {};\n",
                spaces,
                name,
                generate_expression(value)
            )
        }
        StatementIR::Expression(expr) => {
            format!("{}{};\n", spaces, generate_expression(expr))
        }
        StatementIR::Return(Some(expr)) => {
            format!("{}return {};\n", spaces, generate_expression(expr))
        }
        StatementIR::Return(None) => {
            format!("{}return;\n", spaces)
        }
    }
}

/// Generates TypeScript code for an expression.
fn generate_expression(expr: &ExpressionIR) -> String {
    match expr {
        ExpressionIR::StringLiteral(s) => format!("\"{}\"", s.replace('\"', "\\\"")),
        ExpressionIR::NumberLiteral(n) => n.to_string(),
        ExpressionIR::BooleanLiteral(b) => b.to_string(),
        ExpressionIR::Identifier(name) => name.clone(),
        ExpressionIR::StateAccess(field) => format!("this.state.{}", field),
        ExpressionIR::PropertyAccess { object, property } => {
            format!("{}.{}", generate_expression(object), property)
        }
        ExpressionIR::MethodCall {
            object,
            method,
            arguments,
        } => {
            let obj = generate_expression(object);
            let args: Vec<String> = arguments.iter().map(|a| generate_expression(a)).collect();
            format!("{}.{}({})", obj, method, args.join(", "))
        }
        ExpressionIR::Call { callee, arguments } => {
            let args: Vec<String> = arguments.iter().map(|a| generate_expression(a)).collect();
            format!("{}({})", callee, args.join(", "))
        }
        ExpressionIR::New { callee, arguments } => {
            let args: Vec<String> = arguments.iter().map(|a| generate_expression(a)).collect();
            format!("new {}({})", callee, args.join(", "))
        }
        ExpressionIR::Binary {
            left,
            operator,
            right,
        } => {
            let l = generate_expression(left);
            let r = generate_expression(right);
            let op = match operator {
                BinaryOp::Eq => "===",
                BinaryOp::NotEq => "!==",
                BinaryOp::Lt => "<",
                BinaryOp::LtEq => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::GtEq => ">=",
                BinaryOp::And => "&&",
                BinaryOp::Or => "||",
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
            };
            format!("({} {} {})", l, op, r)
        }
        ExpressionIR::Unary { operator, operand } => {
            let arg = generate_expression(operand);
            match operator {
                UnaryOp::Not => format!("!{}", arg),
                UnaryOp::Neg => format!("-{}", arg),
            }
        }
        ExpressionIR::Object(fields) => {
            let entries: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}: {}", k, generate_expression(v)))
                .collect();
            format!("{{ {} }}", entries.join(", "))
        }
        ExpressionIR::Array(elements) => {
            let elems: Vec<String> = elements.iter().map(|e| generate_expression(e)).collect();
            format!("[{}]", elems.join(", "))
        }
    }
}