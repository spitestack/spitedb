import { mkdir, writeFile } from "node:fs/promises";
import { join, dirname, relative, extname } from "node:path";
import type {
  AggregateAnalysis,
  CompilerConfig,
  GenerationResult,
  GeneratedFile,
  SpiteStackRegistration,
  SpiteStackScope,
} from "../types";
import { generateValidators } from "../generators/validator";
import { generateHandlers } from "../generators/handler";
import { generateWiringFile, generateIndexFile } from "../generators/wiring";
import { generateAuthFile } from "../generators/auth";
import { generateRoutesFile } from "../generators/routes";

/**
 * Generate all code from analyzed aggregates
 */
export function generateCode(
  aggregates: AggregateAnalysis[],
  config: CompilerConfig
): GenerationResult {
  const registrationResult = applyRegistrations(aggregates, config.registrations);
  const result: GenerationResult = {
    handlers: [],
    validators: [],
    wiring: null,
    index: null,
    auth: null,
    routes: null,
  };

  if (registrationResult.aggregates.length === 0) {
    return result;
  }

  // Generate validators
  if (config.generate.validators) {
    result.validators = generateValidators(registrationResult.aggregates);
  }

  // Generate handlers
  if (config.generate.handlers) {
    result.handlers = generateHandlers(registrationResult.aggregates, config);
  }

  // Generate wiring
  if (config.generate.wiring) {
    result.wiring = generateWiringFile(registrationResult.aggregates, {
      allowedCommands: registrationResult.allowedCommands,
    });
    result.index = generateIndexFile(registrationResult.aggregates, {
      allowedCommands: registrationResult.allowedCommands,
    });
  }

  if (config.generate.wiring) {
    const appImportPath = config.appPath
      ? toImportPath(config.outDir, config.appPath)
      : null;
    result.auth = generateAuthFile(appImportPath);
    result.routes = generateRoutesFile(
      registrationResult.commandPolicies,
      appImportPath
    );
  }

  return result;
}

function toImportPath(fromDir: string, filePath: string): string {
  let relPath = relative(fromDir, filePath);
  relPath = relPath.replace(/\\/g, "/");
  const extension = extname(relPath);
  if (extension) {
    relPath = relPath.slice(0, -extension.length);
  }
  if (!relPath.startsWith(".")) {
    relPath = `./${relPath}`;
  }
  return relPath;
}

type CommandPolicy = {
  scope: SpiteStackScope;
  roles?: string[];
};

type RegistrationResult = {
  aggregates: AggregateAnalysis[];
  allowedCommands: Map<string, Set<string>>;
  commandPolicies: Map<string, CommandPolicy>;
};

function applyRegistrations(
  aggregates: AggregateAnalysis[],
  registrations?: SpiteStackRegistration[] | null
): RegistrationResult {
  const commandPolicies = new Map<string, CommandPolicy>();
  const allowedCommands = new Map<string, Set<string>>();

  if (!registrations || registrations.length === 0) {
    for (const aggregate of aggregates) {
      const allowed = new Set<string>();
      for (const cmd of aggregate.commands) {
        allowed.add(cmd.methodName);
        commandPolicies.set(`${aggregate.aggregateName}.${cmd.methodName}`, {
          scope: "auth",
        });
      }
      allowedCommands.set(aggregate.aggregateName, allowed);
    }

    return {
      aggregates,
      allowedCommands,
      commandPolicies,
    };
  }

  const registrationMap = new Map<string, SpiteStackRegistration>();
  for (const registration of registrations) {
    registrationMap.set(registration.aggregate.toLowerCase(), registration);
  }

  const filtered: AggregateAnalysis[] = [];

  for (const aggregate of aggregates) {
    const registration = registrationMap.get(aggregate.aggregateName.toLowerCase());
    if (!registration) {
      continue;
    }

    const allowed = new Set<string>();
    const defaultScope = registration.scope ?? "auth";
    const defaultRoles = registration.roles;

    for (const cmd of aggregate.commands) {
      const override = registration.methods?.[cmd.methodName];
      if (override === false) {
        continue;
      }

      let scope = defaultScope;
      let roles = defaultRoles;

      if (typeof override === "string") {
        scope = override;
      } else if (override && typeof override === "object") {
        if (override.scope) {
          scope = override.scope;
        }
        if (override.roles) {
          roles = override.roles;
        }
      }

      allowed.add(cmd.methodName);
      commandPolicies.set(`${aggregate.aggregateName}.${cmd.methodName}`, {
        scope,
        roles,
      });
    }

    allowedCommands.set(aggregate.aggregateName, allowed);
    filtered.push(aggregate);
  }

  return {
    aggregates: filtered,
    allowedCommands,
    commandPolicies,
  };
}

/**
 * Write generated files to disk
 */
export async function writeGeneratedFiles(
  result: GenerationResult,
  outDir: string
): Promise<void> {
  // Create output directories
  await mkdir(join(outDir, "handlers"), { recursive: true });
  await mkdir(join(outDir, "validators"), { recursive: true });

  // Helper to write a file
  async function writeGenFile(file: GeneratedFile) {
    const fullPath = join(outDir, file.path);
    await mkdir(dirname(fullPath), { recursive: true });
    await writeFile(fullPath, file.content, "utf-8");
  }

  // Write all files
  const allFiles: GeneratedFile[] = [
    ...result.handlers,
    ...result.validators,
  ];

  if (result.wiring) {
    allFiles.push(result.wiring);
  }

  if (result.index) {
    allFiles.push(result.index);
  }

  if (result.auth) {
    allFiles.push(result.auth);
  }

  if (result.routes) {
    allFiles.push(result.routes);
  }

  await Promise.all(allFiles.map(writeGenFile));
}

/**
 * Generate and write all code
 */
export async function generate(
  aggregates: AggregateAnalysis[],
  config: CompilerConfig
): Promise<GenerationResult> {
  const result = generateCode(aggregates, config);
  await writeGeneratedFiles(result, config.outDir);
  return result;
}
