//! Bun handler code generation for TypeScript.
//!
//! Generates TypeScript handlers directly without templates.

use crate::ir::{AggregateIR, CommandIR};
use super::ts_types::{to_snake_case, to_pascal_case};

/// Generates TypeScript handlers for an aggregate.
///
/// `domain_import_path` is the relative path from the handlers directory to the domain source.
pub fn generate_handlers(aggregate: &AggregateIR, domain_import_path: &str) -> String {
    let name = &aggregate.name;
    let snake_name = to_snake_case(name);

    let mut code = String::new();

    // Imports
    code.push_str(&format!(
        r#"import type {{ SpiteDbNapi }} from '@spitestack/db';
import {{ {name}Aggregate }} from '{domain_import_path}/{name}/aggregate';
import type {{ {name}Event }} from '{domain_import_path}/{name}/events';
"#
    ));

    // Validator imports
    if !aggregate.commands.is_empty() {
        let validator_imports: Vec<String> = aggregate
            .commands
            .iter()
            .map(|cmd| format!("validate{}{}Input", name, to_pascal_case(&cmd.name)))
            .collect();
        code.push_str(&format!(
            "import {{ {} }} from '../validators/{}.validator';\n",
            validator_imports.join(", "),
            snake_name
        ));
    }

    // HandlerContext type
    code.push_str(
        r#"
export type HandlerContext = {
  db: SpiteDbNapi;
  tenant: string;
};
"#,
    );

    // Generate GET handler
    code.push_str(&generate_get_handler(aggregate));

    // Generate command handlers
    for cmd in &aggregate.commands {
        code.push_str(&generate_command_handler(aggregate, cmd));
    }

    code
}

/// Generates the GET handler for reading aggregate state.
fn generate_get_handler(aggregate: &AggregateIR) -> String {
    let name = &aggregate.name;
    format!(
        r#"
export async function handle{name}Get(
  ctx: HandlerContext,
  streamId: string
): Promise<Response> {{
  const storedEvents = await ctx.db.readStream(streamId, 0, 10000, ctx.tenant);
  const aggregate = new {name}Aggregate();
  for (const e of storedEvents) {{
    aggregate.apply(JSON.parse(e.data.toString()) as {name}Event);
  }}

  return new Response(JSON.stringify({{
    streamId,
    state: aggregate.currentState,
  }}), {{
    status: 200,
    headers: {{ 'Content-Type': 'application/json' }},
  }});
}}
"#
    )
}

/// Generates a command handler for a specific command.
fn generate_command_handler(aggregate: &AggregateIR, cmd: &CommandIR) -> String {
    let name = &aggregate.name;
    let cmd_pascal = to_pascal_case(&cmd.name);

    // Build the command call with parameters
    let command_call = if cmd.parameters.is_empty() {
        format!("aggregate.{}();", cmd.name)
    } else {
        let args: Vec<String> = cmd
            .parameters
            .iter()
            .map(|p| format!("input.{}", p.name))
            .collect();
        format!("aggregate.{}({});", cmd.name, args.join(", "))
    };

    format!(
        r#"
export async function handle{name}{cmd_pascal}(
  ctx: HandlerContext,
  streamId: string,
  body: unknown
): Promise<Response> {{
  const validation = validate{name}{cmd_pascal}Input(body);
  if (!validation.ok) {{
    return new Response(JSON.stringify({{ errors: validation.errors }}), {{
      status: 400,
      headers: {{ 'Content-Type': 'application/json' }},
    }});
  }}
  const input = validation.value;

  const storedEvents = await ctx.db.readStream(streamId, 0, 10000, ctx.tenant);
  const aggregate = new {name}Aggregate();
  for (const e of storedEvents) {{
    aggregate.apply(JSON.parse(e.data.toString()) as {name}Event);
  }}
  const currentRev = storedEvents.length > 0 ? storedEvents[storedEvents.length - 1].streamRev : 0;

  try {{
    {command_call}
  }} catch (err) {{
    return new Response(JSON.stringify({{ error: (err as Error).message }}), {{
      status: 400,
      headers: {{ 'Content-Type': 'application/json' }},
    }});
  }}

  const newEvents = aggregate.events;
  if (newEvents.length > 0) {{
    const eventBuffers = newEvents.map(e => Buffer.from(JSON.stringify(e)));
    const commandId = crypto.randomUUID();
    await ctx.db.append(streamId, commandId, currentRev, eventBuffers, ctx.tenant);
  }}

  return new Response(JSON.stringify({{
    streamId,
    events: newEvents,
    state: aggregate.currentState,
  }}), {{
    status: 200,
    headers: {{ 'Content-Type': 'application/json' }},
  }});
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{DomainType, ParameterIR, EventTypeIR, ObjectType};

    fn make_test_aggregate(name: &str, commands: Vec<CommandIR>) -> AggregateIR {
        AggregateIR {
            name: name.to_string(),
            source_path: std::path::PathBuf::new(),
            state: ObjectType { fields: vec![] },
            initial_state: vec![],
            events: EventTypeIR {
                name: format!("{}Event", name),
                variants: vec![],
            },
            commands,
            raw_apply_body: None,
        }
    }

    fn make_test_command(name: &str, params: Vec<(&str, DomainType)>) -> CommandIR {
        CommandIR {
            name: name.to_string(),
            parameters: params
                .into_iter()
                .map(|(n, t)| ParameterIR {
                    name: n.to_string(),
                    typ: t,
                })
                .collect(),
            body: vec![],
        }
    }

    #[test]
    fn generates_imports() {
        let agg = make_test_aggregate("Todo", vec![]);
        let code = generate_handlers(&agg, "../../domain");

        assert!(code.contains("import type { SpiteDbNapi } from '@spitestack/db'"));
        assert!(code.contains("import { TodoAggregate } from '../../domain/Todo/aggregate'"));
        assert!(code.contains("import type { TodoEvent } from '../../domain/Todo/events'"));
    }

    #[test]
    fn generates_handler_context_type() {
        let agg = make_test_aggregate("Todo", vec![]);
        let code = generate_handlers(&agg, "../../domain");

        assert!(code.contains("export type HandlerContext = {"));
        assert!(code.contains("db: SpiteDbNapi;"));
        assert!(code.contains("tenant: string;"));
    }

    #[test]
    fn generates_get_handler() {
        let agg = make_test_aggregate("Todo", vec![]);
        let code = generate_handlers(&agg, "../../domain");

        assert!(code.contains("export async function handleTodoGet("));
        assert!(code.contains("ctx.db.readStream(streamId"));
        assert!(code.contains("new TodoAggregate()"));
    }

    #[test]
    fn generates_command_handler_with_params() {
        let agg = make_test_aggregate(
            "Todo",
            vec![make_test_command(
                "create",
                vec![
                    ("id", DomainType::String),
                    ("title", DomainType::String),
                ],
            )],
        );
        let code = generate_handlers(&agg, "../../domain");

        assert!(code.contains("export async function handleTodoCreate("));
        assert!(code.contains("validateTodoCreateInput(body)"));
        assert!(code.contains("aggregate.create(input.id, input.title)"));
    }

    #[test]
    fn generates_command_handler_without_params() {
        let agg = make_test_aggregate(
            "Todo",
            vec![make_test_command("complete", vec![])],
        );
        let code = generate_handlers(&agg, "../../domain");

        assert!(code.contains("export async function handleTodoComplete("));
        assert!(code.contains("aggregate.complete();"));
    }

    #[test]
    fn generates_validator_imports() {
        let agg = make_test_aggregate(
            "Todo",
            vec![
                make_test_command("create", vec![("id", DomainType::String)]),
                make_test_command("complete", vec![]),
            ],
        );
        let code = generate_handlers(&agg, "../../domain");

        assert!(code.contains("import { validateTodoCreateInput, validateTodoCompleteInput }"));
        assert!(code.contains("from '../validators/todo.validator'"));
    }
}
