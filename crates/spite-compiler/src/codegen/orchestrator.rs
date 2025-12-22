//! Orchestrator code generation for TypeScript.

use crate::ir::OrchestratorIR;
use super::ts_types::{to_snake_case, to_ts_type};

/// Generates TypeScript code for an orchestrator.
pub fn generate_orchestrator(orchestrator: &OrchestratorIR) -> String {
    let mut output = String::new();
    let fn_name = format!("execute{}", orchestrator.name);

    // Imports
    output.push_str("import type { SpiteDbNapi, TelemetryDbNapi } from '@spitestack/db';\n");
    output.push_str("import { emitTelemetry, finishSpan, logError, metricCounter, metricHistogram, startSpan } from '../runtime/telemetry';\n");

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
    output.push_str("  telemetry: TelemetryDbNapi;\n");
    output.push_str("  tenant: string;\n");
    output.push_str("};\n\n");

    // Generate orchestrator function
    output.push_str(&format!(
        "export async function {}(\n",
        fn_name
    ));
    output.push_str("  ctx: OrchestratorContext,\n");
    output.push_str(&format!("  input: {},\n", input_type));
    output.push_str("  traceId?: string,\n");
    output.push_str("  parentSpanId?: string\n");
    output.push_str(&format!("): Promise<{}> {{\n", result_type));

    output.push_str("  const resolvedTraceId = traceId ?? crypto.randomUUID();\n");
    output.push_str(&format!(
        "  const span = startSpan(ctx.tenant, resolvedTraceId, 'orchestrator.{}', parentSpanId, {{ orchestrator: '{}' }});\n",
        orchestrator.name, orchestrator.name
    ));
    output.push_str("  const startMs = Date.now();\n");
    output.push_str("  const records = [];\n\n");
    output.push_str(&format!(
        "  const finalize = (result: {}, status: 'Ok' | 'Error', err?: unknown) => {{\n",
        result_type
    ));
    output.push_str("    const endMs = Date.now();\n");
    output.push_str("    records.push(\n");
    output.push_str("      finishSpan(span, status, endMs, {\n");
    output.push_str(&format!("        orchestrator: '{}',\n", orchestrator.name));
    output.push_str("        duration_ms: Math.max(0, endMs - startMs),\n");
    output.push_str("      })\n");
    output.push_str("    );\n");
    output.push_str("    records.push(\n");
    output.push_str("      metricCounter(ctx.tenant, 'orchestrator.invocations', 1, {\n");
    output.push_str(&format!("        orchestrator: '{}',\n", orchestrator.name));
    output.push_str("        status,\n");
    output.push_str("      }, resolvedTraceId, span.spanId, span.commandId)\n");
    output.push_str("    );\n");
    output.push_str("    records.push(\n");
    output.push_str("      metricHistogram(ctx.tenant, 'orchestrator.duration_ms', Math.max(0, endMs - startMs), {\n");
    output.push_str(&format!("        orchestrator: '{}',\n", orchestrator.name));
    output.push_str("        status,\n");
    output.push_str("      }, resolvedTraceId, span.spanId, span.commandId)\n");
    output.push_str("    );\n");
    output.push_str("    if (err) {\n");
    output.push_str("      const message = err instanceof Error ? err.message : 'orchestrator failed';\n");
    output.push_str(&format!(
        "      records.push(logError(ctx.tenant, message, {{ orchestrator: '{}' }}, resolvedTraceId, span.spanId, span.commandId));\n",
        orchestrator.name
    ));
    output.push_str("    }\n");
    output.push_str("    emitTelemetry(ctx.telemetry, records);\n");
    output.push_str("    return result;\n");
    output.push_str("  };\n\n");

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
    output.push_str("    span.commandId = commandId;\n");
    output.push_str("    let totalEvents = 0;\n");
    output.push_str("    let totalBytes = 0;\n");

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
            "      totalEvents += {}NewEvents.length;\n",
            var_name
        ));
        output.push_str(&format!(
            "      totalBytes += {}Buffers.reduce((sum, buf) => sum + buf.byteLength, 0);\n",
            var_name
        ));
        output.push_str(&format!(
            "      await ctx.db.append({}, commandId, {}Rev, {}Buffers, ctx.tenant);\n",
            stream_id_param, var_name, var_name
        ));
        output.push_str("    }\n\n");
    }

    output.push_str("    if (totalEvents > 0) {\n");
    output.push_str("      records.push(metricCounter(ctx.tenant, 'events.appended', totalEvents, {\n");
    output.push_str(&format!("        orchestrator: '{}',\n", orchestrator.name));
    output.push_str("      }, resolvedTraceId, span.spanId, commandId));\n");
    output.push_str("      records.push(metricHistogram(ctx.tenant, 'events.payload_bytes', totalBytes, {\n");
    output.push_str(&format!("        orchestrator: '{}',\n", orchestrator.name));
    output.push_str("      }, resolvedTraceId, span.spanId, commandId));\n");
    output.push_str("    }\n");
    output.push_str("    return finalize({ success: true }, 'Ok');\n");
    output.push_str("  } catch (err) {\n");
    output.push_str("    return finalize({ success: false, error: (err as Error).message }, 'Error', err);\n");
    output.push_str("  }\n");
    output.push_str("}\n");

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::OrchestratorDependency;
    use std::path::PathBuf;

    fn make_test_orchestrator(name: &str) -> OrchestratorIR {
        OrchestratorIR {
            name: name.to_string(),
            source_path: PathBuf::new(),
            dependencies: vec![OrchestratorDependency {
                name: "order".to_string(),
                typ: "Order".to_string(),
                optional: false,
            }],
            parameters: vec![],
            is_async: false,
        }
    }

    #[test]
    fn emits_telemetry_without_flush() {
        let orchestrator = make_test_orchestrator("ProcessOrder");
        let code = generate_orchestrator(&orchestrator);

        assert!(code.contains("emitTelemetry(ctx.telemetry, records);"));
        assert!(code.contains("const finalize = (result: ProcessOrderResult, status: 'Ok' | 'Error', err?: unknown) => {"));
        assert!(!code.contains("flushTelemetry"));
        assert!(!code.contains("const finalize = async"));
    }
}
