/**
 * Production start command
 *
 * Runs the production server without watch mode or hot reloading.
 * Can run either the compiled binary or a TypeScript entry point.
 */

import { spawn } from "bun";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { loadConfig } from "./config";
import type { CLIOptions } from "./index";

/**
 * Find server entry point by convention
 */
function findServerEntry(appPath: string | null): string | null {
  const dir = appPath ? dirname(appPath) : process.cwd();
  const candidates = [
    // Check for compiled binary first
    join(dir, "dist", "server"),
    // Then check source files
    join(dir, "server.ts"),
    join(dir, "server.js"),
    join(dir, "src", "server.ts"),
    join(dir, "src", "server.js"),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  return null;
}

/**
 * Run the production server
 */
export async function runStart(args: string[], options: CLIOptions): Promise<number> {
  const cwd = process.cwd();

  // Load config to find app path
  let configPath: string | null = null;
  try {
    const result = await loadConfig(cwd);
    configPath = result.configPath;
  } catch {
    // Config not required for start
  }

  // Find entry point
  const entry = args[0] ?? findServerEntry(configPath);
  if (!entry) {
    console.error("Could not find server entry point.");
    console.error("Run 'spitestack build' first, or specify an entry:");
    console.error("  spitestack start ./server.ts");
    console.error("  spitestack start ./dist/server");
    return 1;
  }

  if (!existsSync(entry)) {
    console.error(`Server entry point not found: ${entry}`);
    return 1;
  }

  // Determine if this is a compiled binary or source file
  const isCompiledBinary = !entry.endsWith(".ts") && !entry.endsWith(".js");

  if (isCompiledBinary) {
    // Run the compiled binary directly
    console.log(`Starting production server: ${entry}`);
    const proc = spawn({
      cmd: [entry],
      stdout: "inherit",
      stderr: "inherit",
      cwd,
    });

    return await proc.exited;
  } else {
    // Run with bun (no --hot flag for production)
    console.log(`Starting server: ${entry}`);
    const proc = spawn({
      cmd: ["bun", entry],
      stdout: "inherit",
      stderr: "inherit",
      cwd,
    });

    return await proc.exited;
  }
}
