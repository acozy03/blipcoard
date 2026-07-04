const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const repoRoot = path.join(__dirname, '..');
const packageJson = require(path.join(repoRoot, 'package.json'));
const version = process.argv[2] || packageJson.version;
const platform = `${process.platform}-${process.arch}`;
const releaseDir = path.join(repoRoot, 'dist', 'release');
const stagingDir = path.join(releaseDir, `blipcoard-${version}-${platform}`);
const archivePath = path.join(releaseDir, `blipcoard-${version}-${platform}.tar.gz`);

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
    ...options,
  });

  if (result.error) {
    console.error(`Failed to run ${command}: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

fs.rmSync(stagingDir, { recursive: true, force: true });
fs.rmSync(archivePath, { force: true });
fs.mkdirSync(stagingDir, { recursive: true });

run('cargo', [
  'build',
  '--release',
  '--locked',
  '-p',
  'blip-cli',
  '-p',
  'blip-daemon',
  '-p',
  'blip-cloud',
]);

for (const name of ['blip', 'blipd', 'blip-cloud']) {
  const executable = process.platform === 'win32' ? `${name}.exe` : name;
  const source = path.join(repoRoot, 'target', 'release', executable);
  const destination = path.join(stagingDir, executable);
  fs.copyFileSync(source, destination);
  fs.chmodSync(destination, 0o755);
}

for (const name of ['README.md', 'LICENSE']) {
  fs.copyFileSync(path.join(repoRoot, name), path.join(stagingDir, name));
}

fs.writeFileSync(
  path.join(stagingDir, 'VERSION'),
  `blipcoard ${version}${os.EOL}`,
);

run('tar', ['-czf', archivePath, '-C', releaseDir, path.basename(stagingDir)]);
console.log(`Created ${archivePath}`);
