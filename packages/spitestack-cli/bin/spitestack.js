#!/usr/bin/env node

import { execFileSync } from "child_process";
import { existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));

/**
 * Get the binary name for the current platform
 */
function getBinaryName() {
  const platform = process.platform;
  const arch = process.arch;

  const binaryName = platform === "win32" ? "spitestack.exe" : "spitestack";

  // Map platform/arch to package name
  const platformMap = {
    "darwin-arm64": "@spitestack/cli-darwin-arm64",
    "darwin-x64": "@spitestack/cli-darwin-x64",
    "linux-x64": "@spitestack/cli-linux-x64-gnu",
    "win32-x64": "@spitestack/cli-win32-x64-msvc",
  };

  const key = `${platform}-${arch}`;
  const packageName = platformMap[key];

  if (!packageName) {
    console.error(`Unsupported platform: ${platform}-${arch}`);
    console.error("Supported platforms: darwin-arm64, darwin-x64, linux-x64, win32-x64");
    process.exit(1);
  }

  return { packageName, binaryName };
}

/**
 * Find the binary path
 */
function findBinary() {
  const { packageName, binaryName } = getBinaryName();

  // Try to find in node_modules
  const paths = [
    // Installed as dependency
    join(__dirname, "..", "node_modules", packageName, binaryName),
    // Installed globally or in workspace
    join(__dirname, "..", "..", packageName, binaryName),
    // Monorepo development
    join(__dirname, "..", "npm", packageName.split("/")[1], binaryName),
  ];

  for (const p of paths) {
    if (existsSync(p)) {
      return p;
    }
  }

  // Fallback: try to find cargo-built binary in development
  const devBinary = join(__dirname, "..", "..", "..", "target", "release", binaryName);
  if (existsSync(devBinary)) {
    return devBinary;
  }

  const devBinaryDebug = join(__dirname, "..", "..", "..", "target", "debug", binaryName);
  if (existsSync(devBinaryDebug)) {
    return devBinaryDebug;
  }

  console.error(`Could not find spitestack binary for ${process.platform}-${process.arch}`);
  console.error("Try reinstalling: npm install spitestack");
  process.exit(1);
}

// Find and execute the binary
const binaryPath = findBinary();
const args = process.argv.slice(2);

try {
  execFileSync(binaryPath, args, { stdio: "inherit" });
} catch (error) {
  // execFileSync throws on non-zero exit, which is fine
  // The child process output is already inherited
  process.exit(error.status || 1);
}
