/**
 * Production build command
 *
 * Compiles the SpiteStack app to a standalone binary using Bun's --compile flag.
 * Side-loads the NAPI native module (.node file) alongside the binary.
 */

import { spawn } from "bun";
import { existsSync } from "node:fs";
import { copyFile, mkdir, writeFile, rm } from "node:fs/promises";
import { dirname, join, relative } from "node:path";
import { platform, arch } from "node:process";
import { loadConfig } from "./config";
import type { CLIOptions } from "./index";

/**
 * Get the platform-specific native module filename
 */
function getNativeModuleName(): string {
  const platformMap: Record<string, string> = {
    darwin: "darwin",
    linux: "linux",
    win32: "win32",
  };
  const archMap: Record<string, string> = {
    arm64: "arm64",
    x64: "x64",
  };

  const p = platformMap[platform] ?? platform;
  const a = archMap[arch] ?? arch;

  // Linux has gnu/musl variants - assume gnu for now
  if (platform === "linux") {
    return `spitedb.linux-${a}-gnu.node`;
  }

  return `spitedb.${p}-${a}.node`;
}

/**
 * Find the native module in node_modules
 */
function findNativeModule(): string | null {
  const nativeModuleName = getNativeModuleName();

  // Check in node_modules/@spitestack/db
  const candidates = [
    join(process.cwd(), "node_modules", "@spitestack", "db", nativeModuleName),
    join(process.cwd(), "..", "node_modules", "@spitestack", "db", nativeModuleName),
    join(process.cwd(), "..", "..", "node_modules", "@spitestack", "db", nativeModuleName),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  return null;
}

/**
 * Find server entry point by convention
 */
function findServerEntry(appPath: string | null): string | null {
  const dir = appPath ? dirname(appPath) : process.cwd();
  const candidates = [
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
 * Run the production build
 */
export async function runBuild(
  args: string[],
  options: CLIOptions,
  runCompile: (options: CLIOptions, checkOnly: boolean) => Promise<number>
): Promise<number> {
  const cwd = process.cwd();

  // Load config
  let config;
  let configPath;
  try {
    const result = await loadConfig(cwd);
    config = result.config;
    configPath = result.configPath;
  } catch (error) {
    console.error(`Error loading config: ${error instanceof Error ? error.message : error}`);
    return 1;
  }

  // Find server entry point
  const entry = args[0] ?? findServerEntry(configPath);
  if (!entry) {
    console.error("Could not find server entry point.");
    console.error("Create server.ts or pass entry: spitestack build ./server.ts");
    return 1;
  }

  if (!existsSync(entry)) {
    console.error(`Server entry point not found: ${entry}`);
    return 1;
  }

  // Run compile first
  console.log("Compiling SpiteStack code...\n");
  const compileResult = await runCompile(options, false);
  if (compileResult !== 0) {
    console.error("\nCompile failed. Fix errors and try again.");
    return compileResult;
  }

  // Prepare output directory
  const outDir = join(cwd, "dist");
  await mkdir(outDir, { recursive: true });

  // Find native module
  const nativeModuleSrc = findNativeModule();
  if (!nativeModuleSrc) {
    console.error("Could not find native module (@spitestack/db .node file)");
    console.error("Make sure @spitestack/db is installed and built.");
    return 1;
  }

  const nativeModuleName = getNativeModuleName();

  // Generate production entry wrapper
  // This sets SPITEDB_NATIVE_PATH to load the .node from the binary's directory
  const wrapperPath = join(config.outDir, "_build_entry.ts");
  const entryRelative = relative(dirname(wrapperPath), entry);

  await writeFile(
    wrapperPath,
    `// Production entry wrapper - patches native module resolution
import { join, dirname } from "node:path";

// For Bun compiled binary, load .node from same directory as executable
const binDir = dirname(process.execPath);
process.env.SPITEDB_NATIVE_PATH = join(binDir, "${nativeModuleName}");

// Import the actual server
import "${entryRelative.startsWith(".") ? entryRelative : "./" + entryRelative}";
`
  );

  // Build with Bun
  console.log(`\nBuilding production binary...`);
  console.log(`  Entry: ${relative(cwd, entry)}`);
  console.log(`  Output: dist/server`);

  const buildProcess = spawn({
    cmd: ["bun", "build", wrapperPath, "--compile", "--outfile", join(outDir, "server")],
    stdout: "inherit",
    stderr: "inherit",
    cwd,
  });

  const buildExit = await buildProcess.exited;
  if (buildExit !== 0) {
    console.error("\nBuild failed");
    // Clean up wrapper
    await rm(wrapperPath, { force: true });
    return 1;
  }

  // Copy native module
  const nativeModuleDest = join(outDir, nativeModuleName);
  await copyFile(nativeModuleSrc, nativeModuleDest);

  // Clean up wrapper
  await rm(wrapperPath, { force: true });

  console.log(`\nBuild complete!`);
  console.log(`  Binary: dist/server`);
  console.log(`  Native: dist/${nativeModuleName}`);
  console.log(`\nTo run: ./dist/server`);
  console.log(`Or: spitestack start`);

  return 0;
}
