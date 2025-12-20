// Copy the compiled dylib to the .node file
// napi build doesn't always do this correctly

const { copyFileSync, existsSync } = require('fs');
const { join } = require('path');

const platform = process.platform;
const arch = process.arch;

// Map node arch to rust target
const archMap = {
  'arm64': 'aarch64',
  'x64': 'x86_64',
};

// Determine source and destination paths
const rootDir = join(__dirname, '..', '..', '..', '..');
const napiDir = __dirname.replace('/scripts', '');

let sourcePath;
let destPath;

if (platform === 'darwin') {
  const rustArch = archMap[arch] || arch;
  sourcePath = join(rootDir, 'target', 'release', 'libspitedb_napi.dylib');
  destPath = join(napiDir, `spitedb.darwin-${arch}.node`);
} else if (platform === 'linux') {
  sourcePath = join(rootDir, 'target', 'release', 'libspitedb_napi.so');
  destPath = join(napiDir, `spitedb.linux-${arch}-gnu.node`);
} else if (platform === 'win32') {
  sourcePath = join(rootDir, 'target', 'release', 'spitedb_napi.dll');
  destPath = join(napiDir, `spitedb.win32-${arch}-msvc.node`);
}

if (existsSync(sourcePath)) {
  copyFileSync(sourcePath, destPath);
  console.log(`Copied ${sourcePath} to ${destPath}`);
} else {
  console.error(`Source not found: ${sourcePath}`);
  process.exit(1);
}
