const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.join(__dirname, '..');
const nativeDir = path.join(__dirname, 'native');
const isCheckout = fs.existsSync(path.join(repoRoot, '.git'));
const skip = process.env.BLIPCOARD_SKIP_POSTINSTALL === '1';

if (isCheckout || skip) {
  process.exit(0);
}

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });

  if (result.error) {
    console.error(`Failed to run ${command}: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const cargoCheck = spawnSync('cargo', ['--version'], {
  stdio: 'ignore',
  shell: process.platform === 'win32',
});

if (cargoCheck.error || cargoCheck.status !== 0) {
  console.error('Installing blipcoard from npm requires Rust and Cargo on PATH.');
  console.error('Install Rust from https://rustup.rs/ and run npm install -g blipcoard again.');
  process.exit(1);
}

run('cargo', ['build', '--release', '--locked', '-p', 'blip-cli', '-p', 'blip-daemon']);

fs.mkdirSync(nativeDir, { recursive: true });
for (const name of ['blip', 'blipd']) {
  const executable = process.platform === 'win32' ? `${name}.exe` : name;
  const source = path.join(repoRoot, 'target', 'release', executable);
  const destination = path.join(nativeDir, executable);
  fs.copyFileSync(source, destination);
  fs.chmodSync(destination, 0o755);
}
