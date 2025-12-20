import type { AggregateAnalysis, OrchestratorAnalysis, GeneratedFile } from "../types";

/**
 * Helper to capitalize first letter
 */
function capitalize(str: string): string {
  return str.charAt(0).toUpperCase() + str.slice(1);
}

/**
 * Generate the wiring file that connects all handlers
 */
export function generateWiringFile(
  aggregates: AggregateAnalysis[],
  orchestrators: OrchestratorAnalysis[] = [],
  options?: {
    allowedCommands?: Map<string, Set<string>>;
  }
): GeneratedFile {
  const allowedCommands = options?.allowedCommands;
  // Generate imports for aggregate handlers
  const handlerImports = aggregates
    .map((agg) => {
      const aggName = agg.aggregateName;
      const handlers = `${aggName}Handlers`;
      const commandType = `${capitalize(aggName)}Command`;
      return `import { ${handlers}, type ${commandType} } from "./handlers/${aggName}.handler";`;
    })
    .join("\n");

  // Generate imports for orchestrator handlers
  const orchestratorImports = orchestrators
    .map((orch) => {
      const handlerName = `${orch.orchestratorName}Handler`;
      const inputType = `${capitalize(orch.orchestratorName)}Input`;
      return `import { ${handlerName}, type ${inputType} } from "./handlers/${orch.orchestratorName}.orchestrator";`;
    })
    .join("\n");

  // Generate Command union type
  const commandTypes = aggregates
    .map((agg) => {
      const aggName = agg.aggregateName;
      const commandType = `${capitalize(aggName)}Command`;
      const allowed = allowedCommands?.get(aggName);
      if (!allowed) {
        return commandType;
      }
      const allowedTypes = agg.commands
        .filter((cmd) => allowed.has(cmd.methodName))
        .map((cmd) => `"${aggName}.${cmd.methodName}"`)
        .join(" | ");
      if (!allowedTypes) {
        return "never";
      }
      return `Extract<${commandType}, { type: ${allowedTypes} }>`;
    })
    .filter((entry) => entry !== "never")
    .join(" | ");
  const resolvedCommandTypes = commandTypes || "never";

  // Generate switch cases for command routing
  const switchCases = aggregates.flatMap((agg) => {
    const aggName = agg.aggregateName;
    const handlers = `${aggName}Handlers`;
    const allowed = allowedCommands?.get(aggName);

    return agg.commands
      .filter((cmd) => !allowed || allowed.has(cmd.methodName))
      .map((cmd) => {
      const caseType = `${aggName}.${cmd.methodName}`;
      return `    case "${caseType}":
      return ${handlers}.${cmd.methodName}(ctx, command.payload);`;
    });
  });

  // Generate orchestrator input types union
  const orchestratorInputTypes = orchestrators
    .map((orch) => `${capitalize(orch.orchestratorName)}Input`)
    .join(" | ") || "never";

  // Generate orchestrator handler switch cases
  const orchestratorSwitchCases = orchestrators.map((orch) => {
    const handlerName = `${orch.orchestratorName}Handler`;
    return `    case "${orch.orchestratorName}":
      return ${handlerName}(ctx, input as ${capitalize(orch.orchestratorName)}Input);`;
  });

  const content = `/**
 * Auto-generated SpiteDB wiring
 * DO NOT EDIT - regenerate with \`spitestack compile\`
 */

import type { SpiteDbNapi } from "@spitestack/db";
${handlerImports}
${orchestratorImports ? "\n" + orchestratorImports : ""}

/**
 * Union of all command types
 */
export type Command = ${resolvedCommandTypes};

/**
 * Context required for command execution
 */
export interface CommandContext {
  db: SpiteDbNapi;
  commandId: string;
  tenant: string;
  actorId?: string;
}

/**
 * Context required for orchestrator execution
 */
export interface OrchestratorContext {
  db: SpiteDbNapi;
  commandId: string;
  tenant: string;
  actorId?: string;
  adapters: Record<string, unknown>;
}

/**
 * Result of command execution
 */
export interface CommandResult {
  aggregateId: string;
  revision: number;
  events: unknown[];
}

/**
 * Execute a command and persist events to SpiteDB
 */
export async function executeCommand(
  ctx: CommandContext,
  command: Command
): Promise<CommandResult> {
  switch (command.type) {
${switchCases.join("\n\n")}

    default:
      const _exhaustive: never = command;
      throw new Error(\`Unknown command type: \${(command as any).type}\`);
  }
}

${orchestrators.length > 0 ? `/**
 * Union of all orchestrator input types
 */
export type OrchestratorInput = ${orchestratorInputTypes};

/**
 * Available orchestrator names
 */
export type OrchestratorName = ${orchestrators.map((o) => `"${o.orchestratorName}"`).join(" | ") || "never"};

/**
 * Execute an orchestrator
 */
export async function executeOrchestrator(
  ctx: OrchestratorContext,
  name: OrchestratorName,
  input: OrchestratorInput
): Promise<void> {
  switch (name) {
${orchestratorSwitchCases.join("\n\n")}

    default:
      const _exhaustive: never = name;
      throw new Error(\`Unknown orchestrator: \${name}\`);
  }
}
` : ""}
// Re-export handlers for direct access
${aggregates.map((agg) => `export { ${agg.aggregateName}Handlers } from "./handlers/${agg.aggregateName}.handler";`).join("\n")}
${orchestrators.length > 0 ? "\n// Re-export orchestrator handlers\n" + orchestrators.map((orch) => `export { ${orch.orchestratorName}Handler } from "./handlers/${orch.orchestratorName}.orchestrator";`).join("\n") : ""}
`;

  return {
    path: "wiring.ts",
    content,
  };
}

/**
 * Generate the index barrel file
 */
export function generateIndexFile(
  aggregates: AggregateAnalysis[],
  orchestrators: OrchestratorAnalysis[] = [],
  options?: {
    allowedCommands?: Map<string, Set<string>>;
  }
): GeneratedFile {
  const allowedCommands = options?.allowedCommands;
  const handlerExports = aggregates
    .map((agg) => {
      const aggName = agg.aggregateName;
      const handlers = `${aggName}Handlers`;
      const commandType = `${capitalize(aggName)}Command`;
      return `export { ${handlers}, type ${commandType} } from "./handlers/${aggName}.handler";`;
    })
    .join("\n");

  const orchestratorExports = orchestrators
    .map((orch) => {
      const handlerName = `${orch.orchestratorName}Handler`;
      const inputType = `${capitalize(orch.orchestratorName)}Input`;
      return `export { ${handlerName}, type ${inputType} } from "./handlers/${orch.orchestratorName}.orchestrator";`;
    })
    .join("\n");

  const validatorExports = aggregates
    .map((agg) => {
      const aggName = agg.aggregateName;
      const allowed = allowedCommands?.get(aggName);
      const validators = agg.commands
        .filter((cmd) => !allowed || allowed.has(cmd.methodName))
        .map((cmd) => `validate${capitalize(aggName)}${capitalize(cmd.methodName)}`)
        .join(", ");
      if (!validators) {
        return "";
      }
      return `export { ${validators} } from "./validators/${aggName}.validator";`;
    })
    .filter(Boolean)
    .join("\n");

  const orchestratorWiringExports = orchestrators.length > 0
    ? "export { executeOrchestrator, type OrchestratorContext, type OrchestratorName, type OrchestratorInput } from \"./wiring\";"
    : "";

  const content = `/**
 * Auto-generated SpiteStack exports
 * DO NOT EDIT - regenerate with \`spitestack compile\`
 */

// Wiring
export { executeCommand, type Command, type CommandContext, type CommandResult } from "./wiring";
${orchestratorWiringExports}

// Handlers
${handlerExports}

${orchestratorExports ? "// Orchestrators\n" + orchestratorExports : ""}

// Validators
${validatorExports}

// Auth
export { createSpiteStackApp, createSpiteStackAuth } from "./auth";

// Routes
export { createCommandHandler } from "./routes";
`;

  return {
    path: "index.ts",
    content,
  };
}
