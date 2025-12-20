import { relative, join } from "node:path";
import type {
  OrchestratorAnalysis,
  OrchestratorDependency,
  TypeInfo,
  GeneratedFile,
  CompilerConfig,
} from "../types";

/**
 * Helper to capitalize first letter
 */
function capitalize(str: string): string {
  return str.charAt(0).toUpperCase() + str.slice(1);
}

/**
 * Generate TypeScript type string from TypeInfo
 */
function typeInfoToTypeScript(type: TypeInfo): string {
  switch (type.kind) {
    case "string":
      return "string";
    case "number":
      return "number";
    case "boolean":
      return "boolean";
    case "null":
      return "null";
    case "undefined":
      return "undefined";
    case "literal":
      if (typeof type.literalValue === "string") {
        return `"${type.literalValue}"`;
      }
      return String(type.literalValue);
    case "array":
      if (type.elementType) {
        return `${typeInfoToTypeScript(type.elementType)}[]`;
      }
      return "unknown[]";
    case "union":
      if (type.types) {
        return type.types.map(typeInfoToTypeScript).join(" | ");
      }
      return "unknown";
    case "object":
      if (type.properties) {
        const props = Object.entries(type.properties)
          .map(([key, value]) => `${key}: ${typeInfoToTypeScript(value)}`)
          .join("; ");
        return `{ ${props} }`;
      }
      return "object";
    default:
      return "unknown";
  }
}

/**
 * Generate the combined input type for an orchestrator handler.
 * Merges aggregate IDs with orchestrate() parameters.
 */
function generateInputType(orchestrator: OrchestratorAnalysis): string {
  const typeName = `${capitalize(orchestrator.orchestratorName)}Input`;

  const props: string[] = [];

  // Add aggregate ID params
  for (const dep of orchestrator.dependencies) {
    if (dep.kind === "aggregate" && dep.idParamName) {
      props.push(`${dep.idParamName}: string`);
    }
  }

  // Add orchestrate method params
  for (const param of orchestrator.orchestrateParams) {
    const typeStr = typeInfoToTypeScript(param.type);
    const optionalMark = param.optional ? "?" : "";
    props.push(`${param.name}${optionalMark}: ${typeStr}`);
  }

  return `export interface ${typeName} {
  ${props.join(";\n  ")};
}`;
}

/**
 * Generate the handler context type
 */
function generateContextType(): string {
  return `export interface OrchestratorHandlerContext {
  db: SpiteDbNapi;
  commandId: string;
  tenant: string;
  actorId?: string;
  adapters: Record<string, unknown>;
}`;
}

/**
 * Generate the event envelope types and helpers
 */
function generateEventHelpers(): string {
  return `type EventEnvelope<T> = {
  data: T;
  __meta: {
    tenantId: string;
    actorId?: string | null;
  };
};

function unwrapEvent<T>(event: T | EventEnvelope<T>): T {
  if (event && typeof event === "object" && "data" in event && "__meta" in event) {
    return (event as EventEnvelope<T>).data;
  }
  return event as T;
}

function wrapEvent<T>(event: T, tenantId: string, actorId?: string): EventEnvelope<T> {
  return {
    data: event,
    __meta: {
      tenantId,
      actorId: actorId ?? null,
    },
  };
}`;
}

/**
 * Generate the aggregate loading helper
 */
function generateLoadAggregateHelper(): string {
  return `interface AggregateBase {
  events: unknown[];
  apply(event: unknown): void;
}

interface LoadedAggregate<T extends AggregateBase> {
  aggregate: T;
  id: string;
  revision: number;
}

async function loadAggregate<T extends AggregateBase>(
  db: SpiteDbNapi,
  AggregateClass: new () => T,
  aggregateId: string,
  tenant: string
): Promise<LoadedAggregate<T>> {
  const existingEvents = await db.readStream(aggregateId, 0, 10000, tenant);

  const aggregate = new AggregateClass();

  for (const event of existingEvents) {
    const parsed = JSON.parse(event.data.toString());
    aggregate.apply(unwrapEvent(parsed));
  }

  return {
    aggregate,
    id: aggregateId,
    revision: existingEvents.length,
  };
}`;
}

/**
 * Generate the atomic commit helper
 */
function generateAtomicCommitHelper(): string {
  return `interface AtomicCommitResult {
  eventCount: number;
  revisions: Map<string, number>;
}

async function commitAggregatesAtomic(
  db: SpiteDbNapi,
  aggregates: Array<{ aggregate: AggregateBase; id: string; revision: number }>,
  commandId: string,
  tenant: string,
  actorId?: string
): Promise<AtomicCommitResult> {
  const commands: Array<{
    streamId: string;
    commandId: string;
    expectedRev: number;
    events: unknown[];
  }> = [];

  let totalEvents = 0;

  for (const { aggregate, id, revision } of aggregates) {
    const newEvents = aggregate.events;

    if (newEvents.length === 0) {
      continue;
    }

    totalEvents += newEvents.length;

    commands.push({
      streamId: id,
      commandId: \`\${commandId}:\${id}\`,
      expectedRev: revision === 0 ? 0 : revision,
      events: newEvents.map((e) => wrapEvent(e, tenant, actorId)),
    });
  }

  if (commands.length === 0) {
    return { eventCount: 0, revisions: new Map() };
  }

  // Use appendBatchJson for atomic multi-stream commit with optimized JSON path
  const payload = JSON.stringify({ commands, tenant });
  const results = await db.appendBatchJson(payload);

  const revisions = new Map<string, number>();
  for (let i = 0; i < commands.length; i++) {
    revisions.set(commands[i].streamId, results[i].lastRev);
  }

  return { eventCount: totalEvents, revisions };
}`;
}

/**
 * Generate the handler function for an orchestrator
 */
function generateHandlerFunction(
  orchestrator: OrchestratorAnalysis,
  adapterDeps: OrchestratorDependency[] = []
): string {
  const inputType = `${capitalize(orchestrator.orchestratorName)}Input`;
  const handlerName = `${orchestrator.orchestratorName}Handler`;

  // Generate aggregate loading code
  const aggregateDeps = orchestrator.dependencies.filter((d) => d.kind === "aggregate");

  // Generate parallel loading using Promise.all
  const loadPromises = aggregateDeps.map((dep) => {
    return `loadAggregate(ctx.db, ${dep.typeName}, input.${dep.idParamName}, ctx.tenant)`;
  });
  const loadedVarNames = aggregateDeps.map((dep) => `${dep.name}Loaded`);

  let loadStatement: string;
  if (aggregateDeps.length === 1) {
    // Single aggregate - no need for Promise.all
    loadStatement = `const ${loadedVarNames[0]} = await ${loadPromises[0]};`;
  } else {
    // Multiple aggregates - use Promise.all for parallel loading
    loadStatement = `const [${loadedVarNames.join(", ")}] = await Promise.all([
    ${loadPromises.join(",\n    ")}
  ]);`;
  }

  const adapterStatements = adapterDeps.map((dep) => {
    // Derive adapter name from type name (remove "Adapter" suffix, lowercase first char)
    let adapterKey = dep.typeName;
    if (adapterKey.endsWith("Adapter")) {
      adapterKey = adapterKey.slice(0, -"Adapter".length);
    }
    adapterKey = adapterKey.charAt(0).toLowerCase() + adapterKey.slice(1);
    // Use unknown type since adapter types aren't imported
    return `const ${dep.name} = ctx.adapters["${adapterKey}"];`;
  });

  // Build constructor arguments
  const constructorArgs = orchestrator.dependencies.map((dep) => {
    if (dep.kind === "aggregate") {
      return `${dep.name}Loaded.aggregate`;
    }
    return dep.name;
  });

  // Build orchestrate call arguments
  let orchestrateCall: string;
  if (orchestrator.paramsStyle === "object") {
    // Single object parameter
    const paramProps = orchestrator.orchestrateParams.map((p) => `${p.name}: input.${p.name}`);
    orchestrateCall = `await orchestrator.orchestrate({ ${paramProps.join(", ")} });`;
  } else {
    // Separate parameters
    const params = orchestrator.orchestrateParams.map((p) => `input.${p.name}`);
    orchestrateCall = `await orchestrator.orchestrate(${params.join(", ")});`;
  }

  // Build aggregates array for commit
  const aggregatesArray = aggregateDeps.map((dep) => dep.name + "Loaded");

  return `export async function ${handlerName}(
  ctx: OrchestratorHandlerContext,
  input: ${inputType}
): Promise<void> {
  // 1. Load aggregates
  ${loadStatement}

  // 2. Get adapters
${adapterStatements.map((s) => `  ${s}`).join("\n")}

  // 3. Instantiate orchestrator with dependencies
  const orchestrator = new ${orchestrator.className}(${constructorArgs.join(", ")});

  // 4. Execute orchestration
  ${orchestrateCall}

  // 5. Atomic commit of all aggregate events
  await commitAggregatesAtomic(
    ctx.db,
    [${aggregatesArray.join(", ")}],
    ctx.commandId,
    ctx.tenant,
    ctx.actorId
  );
}`;
}

/**
 * Calculate the relative import path from a generated handler file to a domain file
 */
function calculateImportPath(
  orchestrator: OrchestratorAnalysis,
  config: CompilerConfig
): string {
  // Handler file location: {outDir}/handlers/{name}.orchestrator.ts
  const handlerDir = join(config.outDir, "handlers");

  // Domain file location: {domainDir}/{relativePath}
  const domainFile = join(config.domainDir, orchestrator.relativePath);

  // Calculate relative path from handler directory to domain file
  let relativePath = relative(handlerDir, domainFile);

  // Remove .ts extension for import
  relativePath = relativePath.replace(/\.ts$/, "");

  // Ensure it starts with ./ or ../
  if (!relativePath.startsWith(".") && !relativePath.startsWith("/")) {
    relativePath = "./" + relativePath;
  }

  return relativePath;
}

/**
 * Generate orchestrator handler file
 */
export function generateOrchestratorHandlerFile(
  orchestrator: OrchestratorAnalysis,
  config: CompilerConfig
): GeneratedFile {
  const fileName = `${orchestrator.orchestratorName}.orchestrator.ts`;

  // Calculate import path for orchestrator
  const orchestratorImportPath = calculateImportPath(orchestrator, config);

  // Handler file location for calculating relative paths
  const handlerDir = join(config.outDir, "handlers");

  // Collect unique aggregate imports (dedupe by type name)
  const aggregateDeps = orchestrator.dependencies.filter((d) => d.kind === "aggregate");
  const uniqueAggregateTypes = new Map<string, OrchestratorDependency>();
  for (const dep of aggregateDeps) {
    if (!uniqueAggregateTypes.has(dep.typeName)) {
      uniqueAggregateTypes.set(dep.typeName, dep);
    }
  }

  const aggregateImports = Array.from(uniqueAggregateTypes.values()).map((dep) => {
    // Calculate relative path from handler directory to aggregate file
    // Aggregates are in domain/aggregates/{Name}/aggregate.ts
    const aggregateName = dep.typeName.replace(/Aggregate$/, "");
    const aggregateFile = join(config.domainDir, "aggregates", aggregateName, "aggregate.ts");
    let importPath = relative(handlerDir, aggregateFile);
    importPath = importPath.replace(/\.ts$/, "");
    if (!importPath.startsWith(".") && !importPath.startsWith("/")) {
      importPath = "./" + importPath;
    }
    return `import { ${dep.typeName} } from "${importPath}";`;
  });

  // Collect adapter dependencies (use unknown type to avoid import issues)
  const adapterDeps = orchestrator.dependencies.filter((d) => d.kind === "adapter");

  const content = `/**
 * Auto-generated handler for ${orchestrator.className}
 * DO NOT EDIT - regenerate with \`spitestack compile\`
 *
 * @generated from ${orchestrator.relativePath}
 */

import type { SpiteDbNapi } from "@spitestack/db";
import { ${orchestrator.className} } from "${orchestratorImportPath}";
${aggregateImports.join("\n")}

${generateContextType()}

${generateInputType(orchestrator)}

${generateEventHelpers()}

${generateLoadAggregateHelper()}

${generateAtomicCommitHelper()}

${generateHandlerFunction(orchestrator, adapterDeps)}
`;

  return {
    path: `handlers/${fileName}`,
    content,
  };
}

/**
 * Generate all orchestrator handler files
 */
export function generateOrchestratorHandlers(
  orchestrators: OrchestratorAnalysis[],
  config: CompilerConfig
): GeneratedFile[] {
  return orchestrators.map((orch) => generateOrchestratorHandlerFile(orch, config));
}
