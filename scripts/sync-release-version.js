const fs = require('node:fs');
const path = require('node:path');

const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error('Usage: node scripts/sync-release-version.js <semver>');
  process.exit(2);
}

const repoRoot = path.join(__dirname, '..');
const packagePath = path.join(repoRoot, 'package.json');
const cargoPath = path.join(repoRoot, 'Cargo.toml');
const cargoLockPath = path.join(repoRoot, 'Cargo.lock');
const workspacePackages = new Set([
  'blip-api',
  'blip-cli',
  'blip-clipboard',
  'blip-cloud',
  'blip-config',
  'blip-core',
  'blip-daemon',
  'blip-sync',
]);

const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
packageJson.version = version;
fs.writeFileSync(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`);

const cargoToml = fs.readFileSync(cargoPath, 'utf8');
if (!/^version = "[^"]+"/m.test(cargoToml)) {
  console.error('Could not find workspace.package version in Cargo.toml');
  process.exit(1);
}
const updatedCargoToml = cargoToml.replace(
  /^version = "[^"]+"/m,
  `version = "${version}"`,
);

fs.writeFileSync(cargoPath, updatedCargoToml);

const cargoLock = fs.readFileSync(cargoLockPath, 'utf8');
let updatedCargoLockPackages = 0;
const updatedCargoLock = cargoLock.replace(
  /(\[\[package\]\]\nname = "([^"]+)"\nversion = ")[^"]+"/g,
  (match, prefix, name) => {
    if (!workspacePackages.has(name)) {
      return match;
    }

    updatedCargoLockPackages += 1;
    return `${prefix}${version}"`;
  },
);

if (updatedCargoLockPackages === 0) {
  console.error('Could not find workspace package versions in Cargo.lock');
  process.exit(1);
}

fs.writeFileSync(cargoLockPath, updatedCargoLock);
