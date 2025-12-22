//! Bun.serve router code generation for TypeScript.

use crate::ir::DomainIR;
use super::ts_types::{to_snake_case, to_pascal_case};

/// Generates the main router that wires up all handlers.
pub fn generate_router(domain: &DomainIR) -> String {
    let mut output = String::new();

    // Imports
    output.push_str("import type { SpiteDbNapi, TelemetryDbNapi } from '@spitestack/db';\n");
    output.push_str(
        "import { emitTelemetry, finishSpan, logError, metricCounter, metricHistogram, startSpan } from './runtime/telemetry';\n",
    );

    // Import handlers for each aggregate
    for aggregate in &domain.aggregates {
        let snake_name = to_snake_case(&aggregate.name);

        let handler_names: Vec<String> = aggregate
            .commands
            .iter()
            .map(|cmd| format!("handle{}{}", aggregate.name, to_pascal_case(&cmd.name)))
            .chain(std::iter::once(format!("handle{}Get", aggregate.name)))
            .collect();

        output.push_str(&format!(
            "import {{ {} }} from './handlers/{}.handlers';\n",
            handler_names.join(", "),
            snake_name
        ));
    }

    output.push('\n');

    // Router context type
    output.push_str("export type RouterContext = {\n");
    output.push_str("  db: SpiteDbNapi;\n");
    output.push_str("  telemetry: TelemetryDbNapi;\n");
    output.push_str("  tenant: string;\n");
    output.push_str("};\n\n");

    // Router function
    output.push_str("export function createRouter(ctx: RouterContext) {\n");
    output.push_str("  return async (req: Request): Promise<Response> => {\n");
    output.push_str("    const url = new URL(req.url);\n");
    output.push_str("    const path = url.pathname;\n");
    output.push_str("    const method = req.method;\n\n");
    output.push_str("    const traceId = crypto.randomUUID();\n");
    output.push_str("    const requestSpan = startSpan(ctx.tenant, traceId, 'http.request', undefined, { method, path });\n\n");
    output.push_str("    const finalize = (response: Response, err?: unknown): Response => {\n");
    output.push_str("      const endMs = Date.now();\n");
    output.push_str("      const durationMs = Math.max(0, endMs - requestSpan.startMs);\n");
    output.push_str("      const attrs = { method, path, status: response.status };\n");
    output.push_str("      const records = [\n");
    output.push_str("        finishSpan(requestSpan, response.status >= 500 ? 'Error' : 'Ok', endMs, attrs),\n");
    output.push_str("        metricCounter(ctx.tenant, 'http.request.count', 1, attrs, traceId, requestSpan.spanId),\n");
    output.push_str("        metricHistogram(ctx.tenant, 'http.request.duration_ms', durationMs, attrs, traceId, requestSpan.spanId),\n");
    output.push_str("      ];\n");
    output.push_str("      if (err || response.status >= 500) {\n");
    output.push_str("        const message = err instanceof Error ? err.message : 'request failed';\n");
    output.push_str("        records.push(logError(ctx.tenant, message, attrs, traceId, requestSpan.spanId));\n");
    output.push_str("      }\n");
    output.push_str("      emitTelemetry(ctx.telemetry, records);\n");
    output.push_str("      return response;\n");
    output.push_str("    };\n\n");

    // Generate route matching for each aggregate
    output.push_str("    try {\n");
    for aggregate in &domain.aggregates {
        let snake_name = to_snake_case(&aggregate.name);

        output.push_str(&format!(
            "      // {} routes\n",
            aggregate.name
        ));
        output.push_str(&format!(
            "      const {}Match = path.match(/^\\/{}\\/([^/]+)(?:\\/([^/]+))?$/);\n",
            snake_name, snake_name
        ));
        output.push_str(&format!("      if ({}Match) {{\n", snake_name));
        output.push_str(&format!("        const streamId = {}Match[1];\n", snake_name));
        output.push_str(&format!("        const action = {}Match[2];\n\n", snake_name));

        // GET handler
        output.push_str("        if (method === 'GET' && !action) {\n");
        output.push_str(&format!(
            "          const response = await handle{}Get(ctx, streamId, traceId, requestSpan.spanId);\n",
            aggregate.name
        ));
        output.push_str("          return finalize(response);\n");
        output.push_str("        }\n");

        // Command handlers
        for cmd in &aggregate.commands {
            output.push_str(&format!(
                "        if (method === 'POST' && action === '{}') {{\n",
                cmd.name
            ));
            output.push_str("          const body = await req.json();\n");
            output.push_str(&format!(
                "          const response = await handle{}{}(ctx, streamId, body, traceId, requestSpan.spanId);\n",
                aggregate.name,
                to_pascal_case(&cmd.name)
            ));
            output.push_str("          return finalize(response);\n");
            output.push_str("        }\n");
        }

        output.push_str("      }\n\n");
    }

    // 404 fallback
    output.push_str("      const response = new Response('Not Found', { status: 404 });\n");
    output.push_str("      return finalize(response);\n");
    output.push_str("    } catch (err) {\n");
    output.push_str("      const response = new Response('Internal Server Error', { status: 500 });\n");
    output.push_str("      return finalize(response, err);\n");
    output.push_str("    }\n");
    output.push_str("  };\n");
    output.push_str("}\n");

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AggregateIR, CommandIR, DomainIR, EventTypeIR, ObjectType};
    use std::path::PathBuf;

    fn make_test_aggregate(name: &str, commands: Vec<CommandIR>) -> AggregateIR {
        AggregateIR {
            name: name.to_string(),
            source_path: PathBuf::new(),
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

    #[test]
    fn emits_telemetry_without_flush() {
        let mut domain = DomainIR::new(PathBuf::new());
        domain.aggregates.push(make_test_aggregate("Todo", vec![]));

        let code = generate_router(&domain);

        assert!(code.contains("emitTelemetry(ctx.telemetry, records);"));
        assert!(code.contains("const finalize = (response: Response, err?: unknown): Response => {"));
        assert!(!code.contains("flushTelemetry"));
        assert!(!code.contains("const finalize = async"));
    }
}
