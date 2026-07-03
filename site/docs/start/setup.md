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
2. Installs root, docs, and desktop npm dependencies.
3. Configures repository git hooks.
4. Builds the Rust workspace.
5. Builds the Docusaurus docs site.
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

If `blip` is not found, add `~/.local/bin` to your shell path:

```bash title="Shell path"
export PATH="$HOME/.local/bin:$PATH"
```
