#!/usr/bin/env node
'use strict';
const { spawnSync } = require('child_process');
const { existsSync } = require('fs');
const path = require('path');
const os = require('os');

const dir = path.join(__dirname, '..');
const isWin = os.platform() === 'win32';
const native = path.join(dir, 'bin', isWin ? 'mnem.exe' : 'mnem');
const lib = path.join(dir, 'lib');

if (!existsSync(native)) {
  process.stderr.write(
    'mnem: native binary not found. Run `npm install` to retry,\n' +
    'or: cargo install --locked mnem-cli\n'
  );
  process.exit(1);
}

const env = { ...process.env };
if (os.platform() === 'linux') env.LD_LIBRARY_PATH = [lib, env.LD_LIBRARY_PATH].filter(Boolean).join(':');
if (os.platform() === 'darwin') env.DYLD_LIBRARY_PATH = [lib, env.DYLD_LIBRARY_PATH].filter(Boolean).join(':');

const r = spawnSync(native, process.argv.slice(2), { env, stdio: 'inherit' });
process.exit(r.status ?? 1);
