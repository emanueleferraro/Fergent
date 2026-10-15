# Fergents Phase 0 dev container image.
#
# Base: Ubuntu 24.04 LTS (matches the reference WSL2 environment in STACK.md
# section 22). The image installs only the toolchains the Phase 0 monorepo
# needs: the Rust stable toolchain (with clippy + rustfmt) and Node LTS + pnpm.
# Steps are pinned and ordered for deterministic, cache-friendly builds.

FROM ubuntu:24.04

# Avoid interactive prompts from apt during the build.
ENV DEBIAN_FRONTEND=noninteractive

# --- System build dependencies -------------------------------------------
# build-essential : compilers and make (native crates, Tauri build deps)
# pkg-config      : locate shared libraries at build time
# libssl-dev      : TLS for crates that link OpenSSL
# curl            : fetch rustup and the NodeSource setup script
# git             : VCS (required by cargo and rustup)
# ca-certificates : trust store so curl/TLS verifications succeed
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        curl \
        git \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# --- Rust toolchain -------------------------------------------------------
# Non-root-capable default location, on PATH for all later layers and shells.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# Install the stable toolchain with the clippy and rustfmt components so the
# quality gates (cargo fmt --check, cargo clippy -D warnings) run in-container.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal \
            --default-toolchain stable \
            --component clippy,rustfmt \
    && rustup --version \
    && cargo --version

# --- Node.js (LTS) + pnpm -------------------------------------------------
# Node.js is installed from NodeSource (Node LTS line). pnpm is installed via
# npm; corepack is enabled so the pinned pnpm version is used per repository.
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/* \
    && corepack enable \
    && npm install -g pnpm \
    && node --version \
    && pnpm --version

# The workspace is mounted at /workspaces by the devcontainer tooling.
WORKDIR /workspaces
