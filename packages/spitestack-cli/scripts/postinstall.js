#!/usr/bin/env node

/**
 * Postinstall script for spitestack CLI
 *
 * This verifies that the correct platform-specific binary was installed.
 * The actual binary comes from optional dependencies that npm/bun install
 * based on the current platform.
 */

import { existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));

const platform = process.platform;
const arch = process.arch;

const platformMap = {
  "darwin-arm64": "@spitestack/cli-darwin-arm64",
  "darwin-x64": "@spitestack/cli-darwin-x64",
  "linux-x64": "@spitestack/cli-linux-x64-gnu",
  "win32-x64": "@spitestack/cli-win32-x64-msvc",
};

const key = `${platform}-${arch}`;
const packageName = platformMap[key];

if (!packageName) {
  console.warn(`\n⚠️  SpiteStack: No prebuilt binary for ${platform}-${arch}`);
  console.warn("   You'll need to build from source using Rust/Cargo.\n");
  process.exit(0);
}

// Check if the optional dependency was installed
const binaryName = platform === "win32" ? "spitestack.exe" : "spitestack";
const possiblePaths = [
  join(__dirname, "..", "node_modules", packageName, binaryName),
  join(__dirname, "..", "..", packageName, binaryName),
];

const installed = possiblePaths.some(p => existsSync(p));

if (!installed) {
  // This is expected during development or if optional deps are skipped
  console.log(`\n📦 SpiteStack: Binary not found for ${platform}-${arch}`);
  console.log("   This is normal during development.");
  console.log("   For production, ensure optional dependencies are installed.\n");
}

// Success - binary is available or we're in development mode
