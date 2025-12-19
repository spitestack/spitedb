import { existsSync } from "node:fs";
import { resolve, join } from "node:path";
import type {
  CompilerConfig,
  PartialConfig,
  SpiteStackAppConfig,
  SpiteStackRegistration,
} from "./types";

const DEFAULT_CONFIG: CompilerConfig = {
  domainDir: "./src/domain/aggregates",
  outDir: "./.spitestack/generated",
  include: ["**/*.ts"],
  exclude: ["**/*.test.ts", "**/*.spec.ts"],
  generate: {
    handlers: true,
    validators: true,
    wiring: true,
  },
  diagnostics: {
    colors: true,
    maxErrors: 50,
  },
  routes: {
    basePath: "/api",
    publicSessionHeader: "x-session-id",
    publicSessionRequired: true,
    publicTenantId: undefined,
  },
};

const CONFIG_FILES = [
  "spitestack.config.ts",
  "spitestack.config.js",
  "spitestack.config.mjs",
];

const APP_FILES = [
  "spitestack.app.ts",
  "spitestack.app.js",
  "spitestack.app.mjs",
  "index.ts",
  "index.js",
  "index.mjs",
];

/**
 * Define a SpiteStack compiler configuration.
 * Use this in your spitestack.config.ts file.
 */
export function defineConfig(config: PartialConfig): PartialConfig {
  return config;
}

/**
 * Merge partial config with defaults
 */
function mergeConfig(partial: PartialConfig): CompilerConfig {
  return {
    domainDir: partial.domainDir ?? DEFAULT_CONFIG.domainDir,
    outDir: partial.outDir ?? DEFAULT_CONFIG.outDir,
    include: partial.include ?? DEFAULT_CONFIG.include,
    exclude: partial.exclude ?? DEFAULT_CONFIG.exclude,
    generate: {
      handlers: partial.generate?.handlers ?? DEFAULT_CONFIG.generate.handlers,
      validators: partial.generate?.validators ?? DEFAULT_CONFIG.generate.validators,
      wiring: partial.generate?.wiring ?? DEFAULT_CONFIG.generate.wiring,
    },
    diagnostics: {
      colors: partial.diagnostics?.colors ?? DEFAULT_CONFIG.diagnostics.colors,
      maxErrors: partial.diagnostics?.maxErrors ?? DEFAULT_CONFIG.diagnostics.maxErrors,
    },
    routes: {
      basePath: partial.routes?.basePath ?? DEFAULT_CONFIG.routes.basePath,
      publicSessionHeader:
        partial.routes?.publicSessionHeader ?? DEFAULT_CONFIG.routes.publicSessionHeader,
      publicSessionRequired:
        partial.routes?.publicSessionRequired ?? DEFAULT_CONFIG.routes.publicSessionRequired,
      publicTenantId: partial.routes?.publicTenantId ?? DEFAULT_CONFIG.routes.publicTenantId,
    },
  };
}

/**
 * Find and load the config file from the current directory or parents
 */
export async function loadConfig(cwd: string = process.cwd()): Promise<{
  config: CompilerConfig;
  configPath: string | null;
}> {
  // Search for app file first
  let searchDir = cwd;
  let configPath: string | null = null;
  let appPath: string | null = null;

  while (searchDir !== "/") {
    for (const appFile of APP_FILES) {
      const candidate = join(searchDir, appFile);
      if (existsSync(candidate)) {
        appPath = candidate;
        break;
      }
    }
    if (appPath) break;
    searchDir = resolve(searchDir, "..");
  }

  // Search for config file
  if (!appPath) {
    searchDir = cwd;
    while (searchDir !== "/") {
      for (const configFile of CONFIG_FILES) {
        const candidate = join(searchDir, configFile);
        if (existsSync(candidate)) {
          configPath = candidate;
          break;
        }
      }
      if (configPath) break;
      searchDir = resolve(searchDir, "..");
    }
  }

  const resolvedPath = appPath ?? configPath;

  // If no config found, use defaults
  if (!resolvedPath) {
    return {
      config: DEFAULT_CONFIG,
      configPath: null,
    };
  }

  // Load and merge config
  try {
    const imported = await import(resolvedPath);
    const raw = imported.default ?? imported;
    const isAppInstance = appPath && raw && typeof raw === "object" && "config" in raw;
    const appConfig = isAppInstance
      ? (raw as { config?: SpiteStackAppConfig }).config ?? null
      : null;
    const registrations = isAppInstance
      ? (raw as { registrations?: SpiteStackRegistration[] }).registrations ?? null
      : null;
    const partial: PartialConfig = (appConfig ?? raw) as PartialConfig;
    const config = appConfig ? (appConfig as CompilerConfig) : mergeConfig(partial);

    // Resolve paths relative to config file
    const configDir = resolve(resolvedPath, "..");
    config.domainDir = resolve(configDir, config.domainDir);
    config.outDir = resolve(configDir, config.outDir);
    config.appPath = appPath;
    config.appConfig = appConfig;
    config.registrations = registrations;

    return { config, configPath: resolvedPath };
  } catch (error) {
    throw new Error(
      `Failed to load config from ${resolvedPath}: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

/**
 * Get default config (for `spitestack init`)
 */
export function getDefaultConfigContent(): string {
  return `import { defineConfig } from "@spitestack/compiler";

export default defineConfig({
  domainDir: "./src/domain/aggregates",
  outDir: "./.spitestack/generated",
  include: ["**/*.ts"],
  exclude: ["**/*.test.ts", "**/*.spec.ts"],
});
`;
}
