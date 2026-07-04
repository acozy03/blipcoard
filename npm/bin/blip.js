#!/usr/bin/env node

const { spawnSync } = require('node:child_process');
const path = require('node:path');

const executable = process.platform === 'win32' ? 'blip.exe' : 'blip';
const binPath = path.join(__dirname, '..', 'native', executable);
const result = spawnSync(binPath, process.argv.slice(2), { stdio: 'inherit' });

if (result.error) {
  console.error(`Failed to run ${binPath}: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 0);
