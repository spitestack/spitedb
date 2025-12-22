//! Orchestrator code generation for TypeScript.

use crate::ir::OrchestratorIR;
use super::ts_types::{to_snake_case, to_ts_type};

/// Generates TypeScript code for an orchestrator.
pub fn generate_orchestrator(orchestrator: &OrchestratorIR) -> String {
    let mut output = String::new();
    let fn_name = format!("execute{}", orchestrator.name);

    // Imports
    output.push_str("import type { SpiteDbNapi } from '@spitestack/db';\n");

    // Import aggregate dependencies
    for dep in &orchestrator.dependencies {
        let snake_name = to_snake_case(&dep.typ);
        output.push_str(&format!(
            "import {{ {}Aggregate }} from '../aggregates/{}.aggregate';\n",
            dep.typ, snake_name
        ));
        output.push_str(&format!(
            "import type {{ {}Event }} from '../events/{}.events';\n",
            dep.typ, snake_name
        ));
    }

    output.push('\n');

    // Generate input type
    let input_type = format!("{}Input", orchestrator.name);
    output.push_str(&format!("export type {} = {{\n", input_type));

    for param in &orchestrator.parameters {
        output.push_str(&format!("  {}: {};\n", param.name, to_ts_type(&param.typ)));
    }

    output.push_str("};\n\n");

    // Generate result type
    let result_type = format!("{}Result", orchestrator.name);
    output.push_str(&format!("export type {} = {{\n", result_type));
    output.push_str("  success: boolean;\n");
    output.push_str("  error?: string;\n");
    output.push_str("};\n\n");

    // Handler context type
    output.push_str("export type OrchestratorContext = {\n");
    output.push_str("  db: SpiteDbNapi;\n");
    output.push_str("  tenant: string;\n");
    output.push_str("};\n\n");

    // Generate orchestrator function
    output.push_str(&format!(
        "export async function {}(\n",
        fn_name
    ));
    output.push_str("  ctx: OrchestratorContext,\n");
    output.push_str(&format!("  input: {}\n", input_type));
    output.push_str(&format!("): Promise<{}> {{\n", result_type));

    output.push_str("  try {\n");

    // Load each aggregate dependency
    output.push_str("    // Load aggregates\n");
    for dep in &orchestrator.dependencies {
        let var_name = to_snake_case(&dep.name);
        let stream_id_param = format!("input.{}StreamId", to_snake_case(&dep.name));

        output.push_str(&format!(
            "    const {}Events = await ctx.db.readStream({}, 0, 10000, ctx.tenant);\n",
            var_name, stream_id_param
        ));
        output.push_str(&format!(
            "    const {} = {}Aggregate.create(\n",
            var_name, dep.typ
        ));
        output.push_str(&format!(
            "      {}Events.map(e => JSON.parse(e.data.toString()) as {}Event)\n",
            var_name, dep.typ
        ));
        output.push_str("    );\n");
        output.push_str(&format!(
            "    const {}Rev = {}Events.length > 0 ? {}Events[{}Events.length - 1].streamRev : 0;\n\n",
            var_name, var_name, var_name, var_name
        ));
    }

    output.push_str("    // Execute orchestrated workflow\n");
    output.push_str("    // TODO: Add workflow logic based on orchestrator body\n\n");

    // Persist all changes
    output.push_str("    // Persist all changes\n");
    output.push_str("    const commandId = crypto.randomUUID();\n");

    for dep in &orchestrator.dependencies {
        let var_name = to_snake_case(&dep.name);
        let stream_id_param = format!("input.{}StreamId", to_snake_case(&dep.name));

        output.push_str(&format!(
            "    const {}NewEvents = {}.getPendingEvents();\n",
            var_name, var_name
        ));
        output.push_str(&format!(
            "    if ({}NewEvents.length > 0) {{\n",
            var_name
        ));
        output.push_str(&format!(
            "      const {}Buffers = {}NewEvents.map(e => Buffer.from(JSON.stringify(e)));\n",
            var_name, var_name
        ));
        output.push_str(&format!(
            "      await ctx.db.append({}, commandId, {}Rev, {}Buffers, ctx.tenant);\n",
            stream_id_param, var_name, var_name
        ));
        output.push_str("    }\n\n");
    }

    output.push_str("    return { success: true };\n");
    output.push_str("  } catch (err) {\n");
    output.push_str("    return { success: false, error: (err as Error).message };\n");
    output.push_str("  }\n");
    output.push_str("}\n");

    output
}
