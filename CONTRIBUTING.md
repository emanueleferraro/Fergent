# Contributing to Fergents

Thanks for your interest in Fergents. This guide covers the development
environment, the quality gates every change must pass, and the conventions the
project follows. See `README.md` for what Fergents is and `ARCHITECTURE.md` for
the system design.

---

## 1. Development setup

### Reference environment: WSL2 + devcontainer

The reference development environment is **WSL2 (Ubuntu)**. A `.devcontainer/`
(Docker-based) is provided and recommended: it provisions Rust (stable, with
`clippy` and `rustfmt`), Node LTS, and pnpm, so the toolchain is reproducible
across machines and matches CI.

- **VS Code:** open the repository and choose "Reopen in Container" (requires
  the Dev Containers extension and a running Docker daemon).
- **Without the container:** install Rust via `rustup` with the `clippy` and
  `rustfmt` components (the channel is pinned by `rust-toolchain.toml`), install
  Node LTS, then enable pnpm via `corepack enable pnpm` (or
  `npm install -g pnpm`).

### Build

```sh
cargo build --release   # Rust: all crates + both binaries
pnpm install            # TypeScript: install workspace dependencies
pnpm build              # TypeScript: build all packages and apps
```

### Run

```sh
cargo run -p fergentsd -- start --foreground
cargo run -p fergents -- status
```

---

## 2. Quality gates

Every pull request must pass all of the following. Run them locally before
opening a PR; CI enforces the same set.

Rust:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

TypeScript:

```sh
pnpm lint              # eslint
pnpm format:check      # prettier --check
pnpm -r typecheck      # tsc --noEmit
pnpm -r test           # vitest
pnpm build
```

Do not open a PR that fails any gate. If a gate is genuinely inapplicable,
explain why in the PR description.

---

## 3. Commit conventions

- Follow **Conventional Commits**: `type(scope): summary`
  - types: `feat`, `fix`, `docs`, `refactor`, `test`, `build`, `ci`, `chore`,
    `perf`.
  - example: `feat(storage): add audit-log hash chain verification`.
- Keep the subject line in the imperative mood and under ~72 characters.
- Reference related issues in the body when applicable.

---

## 4. Pull requests

- **One deliverable per PR.** A switch case, a bug fix, and a docs update
  belong in three PRs. Small, focused PRs are reviewed faster and are easier to
  revert.
- **Tests are required.** New behavior needs tests: unit tests for logic and
  integration tests for cross-crate flows (see `STACK.md` §21). A bug fix
  should include a test that fails before the fix.
- **CI must be green.** No PR is merged with failing checks.
- **Describe the why.** In the PR description, state the problem and the
  approach, not just the mechanics.

---

## 5. Architecture and design rules

- **ADRs for non-trivial decisions.** Any decision that is hard to reverse, or
  that affects more than one crate, needs an Architecture Decision Record.
  Existing decisions are summarized in `ARCHITECTURE.md` §17.
- **No global mutable state.** The kernel is a set of async tasks coordinating
  through the event bus and explicit registries. Do not introduce global
  mutable singletons; pass state explicitly.
- **No unjustified dependencies.** Every new crate or npm dependency must be
  justified in the PR description (what it does, why nothing existing suffices,
  its license compatibility with AGPL-3.0-or-later, and its maintenance
  status). Prefer the standard library and existing dependencies.
- **The protocol is the contract.** Changes to the wire protocol start in
  `protocol/` (JSON Schema) and flow through codegen; generated bindings are
  committed and checked for freshness.
- **UI uses the design system.** All UI is built from `style/` tokens and
  components (CSS include order: `tokens.css`, then `typography.css`, then
  `components.css`). No hardcoded colors, spacing, radii, or motion.

---

## 6. Code of conduct

By participating, you agree to abide by the
[Code of Conduct](CODE_OF_CONDUCT.md).

---

## 7. License

Fergents is licensed under **AGPL-3.0-or-later**. By contributing, you agree
that your contributions are licensed under the same terms. See `LICENSE`.
