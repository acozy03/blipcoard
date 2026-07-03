#!/usr/bin/env bash

set -euo pipefail

repo_url="${BLIPCOARD_REPO_URL:-https://github.com/blipcoard/blipcoard.git}"
target_dir="${BLIPCOARD_DIR:-${HOME}/blipcoard}"
install_runtime="${BLIPCOARD_INSTALL_RUNTIME:-1}"
start_service="${BLIPCOARD_START_SERVICE:-1}"

usage() {
  cat <<'EOF'
Usage: setup-dev.sh [--dir PATH] [--no-runtime] [--no-service]

Environment:
  BLIPCOARD_REPO_URL        Git URL to clone when not already in a checkout.
  BLIPCOARD_DIR             Target checkout path. Defaults to ~/blipcoard.
  BLIPCOARD_INSTALL_RUNTIME Set to 0 to skip installing blip/blipd.
  BLIPCOARD_START_SERVICE   Set to 0 to skip service install/start.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir)
      target_dir="$2"
      shift 2
      ;;
    --no-runtime)
      install_runtime=0
      shift
      ;;
    --no-service)
      start_service=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

need git
need npm
need cargo

if git rev-parse --show-toplevel >/dev/null 2>&1; then
  repo_root="$(git rev-parse --show-toplevel)"
else
  if [[ -e "${target_dir}/.git" ]]; then
    repo_root="${target_dir}"
  elif [[ -e "${target_dir}" ]]; then
    echo "target exists but is not a git checkout: ${target_dir}" >&2
    exit 1
  else
    git clone "${repo_url}" "${target_dir}"
    repo_root="${target_dir}"
  fi
fi

cd "${repo_root}"

echo "==> using checkout: ${repo_root}"
echo "==> installing root tooling"
npm install

echo "==> installing docs dependencies"
npm --prefix site install

if [[ -d apps/desktop ]]; then
  echo "==> installing desktop dependencies"
  npm --prefix apps/desktop install
fi

echo "==> configuring git hooks"
npm run hooks:install

echo "==> building Rust workspace"
cargo build --workspace --locked

echo "==> validating docs build"
npm run docs:build

if [[ "${install_runtime}" == "1" ]]; then
  echo "==> installing blip and blipd into ~/.local/bin"
  cargo install --path crates/blip-cli --locked --root "${HOME}/.local"
  cargo install --path crates/blip-daemon --locked --root "${HOME}/.local"

  export PATH="${HOME}/.local/bin:${PATH}"

  if [[ "${start_service}" == "1" ]]; then
    case "$(uname -s)" in
      Darwin|Linux)
        echo "==> installing and starting user daemon service"
        blip service install
        blip service start
        blip service status
        ;;
      *)
        echo "==> service startup skipped on this platform"
        ;;
    esac
  fi
fi

cat <<EOF

blipcoard setup complete.

Checkout: ${repo_root}
Docs:     npm run docs:start
CLI:      ${HOME}/.local/bin/blip
Daemon:   ${HOME}/.local/bin/blipd

If ~/.local/bin is not on PATH, add:
  export PATH="\$HOME/.local/bin:\$PATH"
EOF
