#!/usr/bin/env node

const { spawnSync } = require('child_process');
const { existsSync, copyFileSync, mkdirSync } = require('fs');
const path = require('path');

const repoRoot = path.join(__dirname, '..');
const nativeDir = path.join(repoRoot, 'native');
const manifest = path.join(nativeDir, 'Cargo.toml');

const cargoArgs = ['build', '--manifest-path', manifest, '--release'];
const cargoCandidates = process.env.CARGO
  ? [process.env.CARGO]
  : ['cargo', '/opt/rust/bin/cargo'];

let selectedCargo = null;
let buildResult = null;

for (const candidate of cargoCandidates) {
  buildResult = spawnSync(candidate, cargoArgs, { stdio: 'inherit' });
  if (buildResult.error && buildResult.error.code === 'ENOENT') {
    continue;
  }
  selectedCargo = candidate;
  break;
}

if (!selectedCargo) {
  console.error(`Failed to execute cargo. Tried: ${cargoCandidates.join(', ')}`);
  process.exit(1);
}

if (buildResult && buildResult.status !== 0) {
  process.exit(buildResult.status ?? 1);
}

const artifactName = (() => {
  switch (process.platform) {
    case 'win32':
      return 'jsonata_js_bridge.dll';
    case 'darwin':
      return 'libjsonata_js_bridge.dylib';
    default:
      return 'libjsonata_js_bridge.so';
  }
})();

const sourcePath = path.join(nativeDir, 'target', 'release', artifactName);
if (!existsSync(sourcePath)) {
  console.error(`Native artifact not found: ${sourcePath}`);
  process.exit(1);
}

const outputPath = path.join(nativeDir, 'index.node');
mkdirSync(path.dirname(outputPath), { recursive: true });
copyFileSync(sourcePath, outputPath);
