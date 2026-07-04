# One-Command Setup

Use this when you want to clone `blipcoard`, install local tooling, build the
workspace, install `blip` and `blipd`, and start the user daemon service.

```bash title="Clone, build, install, and start blipcoard"
curl -fsSL https://raw.githubusercontent.com/blipcoard/blipcoard/develop/scripts/setup-dev.sh | bash
```

The script clones into `~/blipcoard` by default. To choose another directory:

```bash title="Choose a checkout directory"
curl -fsSL https://raw.githubusercontent.com/blipcoard/blipcoard/develop/scripts/setup-dev.sh | bash -s -- --dir ~/src/blipcoard
```

If you only want the repo dependencies and builds without installing or starting
the daemon:

```bash title="Set up for development only"
curl -fsSL https://raw.githubusercontent.com/blipcoard/blipcoard/develop/scripts/setup-dev.sh | bash -s -- --no-runtime --no-service
```

## What The Script Does

1. Clones the repository if you are not already inside a checkout.
2. Installs root, docs, desktop, and hosted web npm dependencies.
3. Configures repository git hooks.
4. Builds the Rust workspace.
5. Builds the Docusaurus docs site and hosted web app.
6. Installs `blip` and `blipd` into `~/.local/bin`.
7. Installs and starts the user daemon service on macOS or Linux.

Windows service startup is still unavailable until Windows daemon IPC lands. The
script still handles dependency installation and workspace builds.

## Run From An Existing Checkout

```bash title="Existing checkout"
./scripts/setup-dev.sh
```

Or through npm:

```bash npm2yarn title="Existing checkout through npm"
npm run setup
```

## Install With npm

CLI-only users can install the current stable package from npm:

```bash npm2yarn title="Install stable CLI and daemon"
npm install -g @cosentinode/blipcoard
```

Install the latest prerelease from `develop` with the `next` dist-tag:

```bash npm2yarn title="Install next prerelease"
npm install -g @cosentinode/blipcoard@next
```

Update an existing global install:

```bash npm2yarn title="Update global install"
npm update -g @cosentinode/blipcoard
```

The npm package builds `blip` and `blipd` from source during install, so Rust and
Cargo must be available on `PATH`. Desktop bundles are still distributed
separately from the npm CLI package.

## After Setup

Check the daemon and CLI:

```bash title="Verify runtime"
blip service status
blip health
blip workspaces
```

Start the docs site locally:

```bash npm2yarn title="Start docs"
npm run docs:start
```

Start the hosted web app locally:

```bash npm2yarn title="Start hosted web"
npm run web:dev
```

If `blip` is not found, add `~/.local/bin` to your shell path:

```bash title="Shell path"
export PATH="$HOME/.local/bin:$PATH"
```
