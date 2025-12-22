//! Bun.serve router code generation for TypeScript.

use crate::ir::DomainIR;
use super::ts_types::{to_snake_case, to_pascal_case};

/// Generates the main router that wires up all handlers.
pub fn generate_router(domain: &DomainIR) -> String {
    let mut output = String::new();

    // Imports
    output.push_str("import type { SpiteDbNapi } from '@spitestack/db';\n");

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
    output.push_str("  tenant: string;\n");
    output.push_str("};\n\n");

    // Router function
    output.push_str("export function createRouter(ctx: RouterContext) {\n");
    output.push_str("  return async (req: Request): Promise<Response> => {\n");
    output.push_str("    const url = new URL(req.url);\n");
    output.push_str("    const path = url.pathname;\n");
    output.push_str("    const method = req.method;\n\n");

    // Generate route matching for each aggregate
    for aggregate in &domain.aggregates {
        let snake_name = to_snake_case(&aggregate.name);

        output.push_str(&format!(
            "    // {} routes\n",
            aggregate.name
        ));
        output.push_str(&format!(
            "    const {}Match = path.match(/^\\/{}\\/([^/]+)(?:\\/([^/]+))?$/);\n",
            snake_name, snake_name
        ));
        output.push_str(&format!("    if ({}Match) {{\n", snake_name));
        output.push_str(&format!("      const streamId = {}Match[1];\n", snake_name));
        output.push_str(&format!("      const action = {}Match[2];\n\n", snake_name));

        // GET handler
        output.push_str("      if (method === 'GET' && !action) {\n");
        output.push_str(&format!(
            "        return handle{}Get(ctx, streamId);\n",
            aggregate.name
        ));
        output.push_str("      }\n");

        // Command handlers
        for cmd in &aggregate.commands {
            output.push_str(&format!(
                "      if (method === 'POST' && action === '{}') {{\n",
                cmd.name
            ));
            output.push_str("        const body = await req.json();\n");
            output.push_str(&format!(
                "        return handle{}{}(ctx, streamId, body);\n",
                aggregate.name,
                to_pascal_case(&cmd.name)
            ));
            output.push_str("      }\n");
        }

        output.push_str("    }\n\n");
    }

    // 404 fallback
    output.push_str("    return new Response('Not Found', { status: 404 });\n");
    output.push_str("  };\n");
    output.push_str("}\n");

    output
}