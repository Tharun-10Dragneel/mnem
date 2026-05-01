#!/usr/bin/env node
/**
 * Post-install script for @mnemos/mnem
 * Downloads the prebuilt binary for the current platform
 */

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const PKG_VERSION = require('./package.json').version;
const BIN_DIR = path.join(__dirname, 'bin');
const MNEM_BIN = path.join(BIN_DIR, process.platform === 'win32' ? 'mnem.exe' : 'mnem');

function getPlatform() {
  const platform = process.platform;
  const arch = process.arch;

  if (platform === 'darwin') {
    return arch === 'arm64' ? 'macos-arm64' : 'macos-x64';
  }
  if (platform === 'linux') {
    return arch === 'arm64' ? 'linux-aarch64' : 'linux-x86_64';
  }
  if (platform === 'win32') {
    return 'windows-x86_64';
  }
  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

function getDownloadURL() {
  const platform = getPlatform();
  const ext = platform.startsWith('windows') ? 'zip' : 'tar.gz';
  return `https://github.com/Uranid/mnem/releases/download/v${PKG_VERSION}/mnem-${platform}.${ext}`;
}

async function downloadBinary() {
  console.log(`Downloading mnem ${PKG_VERSION} for ${getPlatform()}...`);

  // For now, just warn that they should use cargo install
  console.log('');
  console.log('NOTE: npm install currently works best with the bundled embedder:');
  console.log('  npm install -g @mnemos/mnem');
  console.log('');
  console.log('Or install from source:');
  console.log('  cargo install --locked mnem-cli --features bundled-embedder');
  console.log('');

  // TODO: Implement actual binary download when releases are published
  // For now, this package just provides the npm package scaffolding
  console.log('Full binary downloads coming in v0.2.0 release.');
}

if (fs.existsSync(MNEM_BIN)) {
  console.log('mnem binary already installed.');
} else {
  // Skip download for now - just ensure bin directory exists
  if (!fs.existsSync(BIN_DIR)) {
    fs.mkdirSync(BIN_DIR, { recursive: true });
  }
  console.log('mnem binary will be available after full v0.2.0 release.');
}

console.log('');
console.log('Get started:');
console.log('  mnem --version');
console.log('  mnem init');
console.log('  mnem doctor');