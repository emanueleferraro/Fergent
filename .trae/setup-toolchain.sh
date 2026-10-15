#!/usr/bin/env bash
set -euxo pipefail
export DEBIAN_FRONTEND=noninteractive

echo "=== OS ==="
cat /etc/os-release | head -n 3
echo "HOME=$HOME"

apt-get update
apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev curl ca-certificates git

echo "=== rustup ==="
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable --component clippy --component rustfmt
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"
rustc --version
cargo --version
cargo clippy --version || true
cargo fmt --version || true

echo "=== node ==="
if ! command -v node >/dev/null 2>&1; then
  curl -fsSL https://deb.nodesource.com/setup_lts.x | bash -
  apt-get install -y nodejs
fi
node --version
npm --version

echo "=== pnpm ==="
if ! command -v pnpm >/dev/null 2>&1; then
  npm install -g pnpm
fi
pnpm --version

echo "TOOLCHAIN_SETUP_DONE"
