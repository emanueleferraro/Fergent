# AI Workflow

> How to feed this documentation set to an AI developer, what tokens and
> tools to provide, how to test, how to iterate, and how to ship.

**Audience:** the architect (you), the AI developer.
**Companion docs:** every other doc, plus `PROMPTS/`.

---

## Table of contents

1. [Overview](#1-overview)
2. [The AI developer's environment](#2-the-ai-developers-environment)
3. [What to give the AI](#3-what-to-give-the-ai)
4. [Tokens, keys, and accounts](#4-tokens-keys-and-accounts)
5. [The devcontainer](#5-the-devcontainer)
6. [The development loop](#6-the-development-loop)
7. [The testing strategy](#7-the-testing-strategy)
8. [The publishing pipeline](#8-the-publishing-pipeline)
9. [How to prompt the AI](#9-how-to-prompt-the-ai)
10. [Iterating on features](#10-iterating-on-features)
11. [Debugging with the AI](#11-debugging-with-the-ai)
12. [Reviewing AI output](#12-reviewing-ai-output)
13. [Working with the styleguide](#13-working-with-the-styleguide)
14. [Milestones and checkpoints](#14-milestones-and-checkpoints)
15. [Common pitfalls](#15-common-pitfalls)
16. [Checklist: first week](#16-checklist-first-week)
17. [Checklist: first month](#17-checklist-first-month)
18. [Checklist: first release](#18-checklist-first-release)
19. [Appendix: prompt templates](#19-appendix-prompt-templates)

---

## 1. Overview

Fergents is designed to be built primarily by an AI developer, with a human
architect reviewing and steering. This document tells you exactly what to
give the AI, in what order, and how to keep it productive.

The AI is not a code generator. It is a **developer** with:

- The full documentation set as its spec.
- A devcontainer as its environment.
- A test harness as its feedback loop.
- The styleguide as its visual contract.
- The architect as its reviewer.

---

## 2. The AI developer's environment

### 2.1 Where it runs

The AI developer runs in one of:

- **Trae** (the recommended environment; the architect already uses it).
- **Cursor** with Claude/GPT.
- **Claude Code** with a local terminal.
- **Aider** in a terminal.
- **Any agent with filesystem + shell + git access.**

The environment must have:

- Read/write access to the monorepo.
- Shell access (to run builds and tests).
- Git access (to commit).
- Network access to the providers the AI uses.
- Optional: access to a browser for visual testing.

### 2.2 What it needs installed

- Rust (via rustup).
- Node.js (LTS) + pnpm.
- `wasm-tools`, `wasmtime`, `cargo-component`.
- Tauri CLI.
- `sqlite3`, DB Browser for SQLite.
- `git`, `gh`.
- `sccache` (for fast Rust builds).

All provided by the devcontainer (see §5).

---

## 3. What to give the AI

### 3.1 The documentation set

Give the AI **the full set of docs, in this order**:

1. `README.md` — orientation.
2. `ARCHITECTURE.md` — the big picture.
3. `STACK.md` — every technical choice.
4. `DATA_MODEL.sql` — the schema.
5. `ROADMAP.md` — the plan.
6. `AGENTS.md` — the agent runtime.
7. `MCP.md` — integration.
8. `SKILLS.md` — skills.
9. `WORKFLOW.md` — automation.
10. `LLM.md` — models.
11. `BRAIN.md` — knowledge.
12. `SEARCH.md` — retrieval.
13. `GRAPH.md` — entities.
14. `SYNC.md` — multi-device.
15. `SECURITY.md` — permissions, sandbox, crypto.
16. `UI.md` — layout, interaction.
17. `PLUGINS.md` — extension.
18. `DASHBOARD.md` — observability.
19. `STYLEGUIDE_EXTENSIONS.md` — Fergents components.
20. `/style/readme.md` + `/style/*` — the design system.

### 3.2 The protocol

Give the AI the JSON Schema definitions for the kernel protocol
(`protocol/`). These are the contract between kernel, UI, and CLI.

### 3.3 The styleguide

Give the AI the full `/style/` folder. Instruct it to **never invent new
tokens** and **never hardcode colors/spacing**. Every UI decision must
reference an existing token.

### 3.4 The prompts

`PROMPTS/` contains initial prompts for:

- Setting up the monorepo.
- Implementing the kernel skeleton.
- Implementing the event bus.
- Implementing the LLM router.
- Implementing the agent runtime.
- Implementing the chat UI.
- Implementing a plugin.
- Writing tests.

These are starting points; the AI can adapt.

---

## 4. Tokens, keys, and accounts

### 4.1 GitHub

- Create a GitHub account (if not already).
- Create a **private** repo for Fergents (public later).
- Generate a **Personal Access Token** (classic, `repo` + `workflow`
  scopes) or use a **fine-grained token** with the same permissions.
- Store the token in the devcontainer as `GITHUB_TOKEN`.
- The AI uses `gh` CLI authenticated with this token.

### 4.2 LLM API keys

At minimum:

- **Anthropic** (for Claude models).
- **OpenAI** (for GPT models and embeddings fallback).
- Optional: Google, Mistral, Cohere, etc.
- Optional: **Ollama** running locally for free local inference.

Store keys in the devcontainer as env vars:

```
ANTHROPIC_API_KEY=...
OPENAI_API_KEY=...
```

The AI uses them for its own development (writing code) and for testing
Fergents' LLM calls.

### 4.3 Other services

- **Hugging Face** (optional): for downloading models.
- **Codecov** (optional): for coverage.
- **Sentry** (optional, opt-in): for crash reports.

### 4.4 What NOT to give the AI

- Production keys.
- Personal keys not intended for development.
- Any key with broad permissions beyond the dev repo.

---

## 5. The devcontainer

### 5.1 What it is

A `.devcontainer/` folder with:

- `devcontainer.json` — the config.
- `Dockerfile` — the image.
- `post-create.sh` — setup script.

### 5.2 Contents

The image includes:

- Ubuntu 24.04.
- Rust (latest stable).
- Node.js 22 LTS + pnpm.
- `wasm-tools`, `wasmtime`, `cargo-component`.
- Tauri CLI + system deps (webkit2gtk, etc.).
- `sqlite3`, `sqlitebrowser`.
- `git`, `gh`.
- `sccache`.
- `ripgrep`, `fd`, `bat`, `jq`, `yq`.
- The AI's preferred editor tools.

### 5.3 Why a devcontainer

- **Reproducibility**: same environment everywhere.
- **Safety**: the AI works inside a container, not on your host.
- **Speed**: pre-installed toolchains, cached builds.

### 5.4 How to use it

- VS Code / Cursor / Trae: "Reopen in Container".
- CLI: `devcontainer up`.

---

## 6. The development loop

### 6.1 The cycle

```
1. Architect writes or updates a spec doc.
2. Architect prompts the AI with the spec + context.
3. AI writes code + tests.
4. AI runs tests (cargo test, vitest).
5. AI runs the linter (clippy, eslint).
6. AI runs the build (cargo build, pnpm build).
7. AI commits to a branch.
8. AI opens a PR (gh pr create).
9. CI runs (GitHub Actions).
10. Architect reviews the PR.
11. Architect merges or requests changes.
12. Repeat.
```

### 6.2 Branching

- `main` — always green.
- `feat/<name>` — features.
- `fix/<name>` — fixes.
- `docs/<name>` — docs.
- `chore/<name>` — chores.

### 6.3 Commits

Follow **Conventional Commits**:

```
feat(kernel): add event bus persistence
fix(ui): correct chat scroll on stream
docs(agents): clarify sub-agent limits
chore(deps): bump tokio to 1.40
```

### 6.4 PRs

Every PR:

- Has a description with context and test plan.
- Passes CI.
- Has at least one reviewer (the architect).
- Squash-merges into `main`.

---

## 7. The testing strategy

### 7.1 For the AI

The AI must run tests before declaring done:

- **Rust**: `cargo test --workspace`, `cargo clippy -- -D warnings`, `cargo
  fmt --check`.
- **TypeScript**: `pnpm test`, `pnpm lint`, `pnpm typecheck`.
- **Integration**: `cargo test --test integration`.
- **Visual**: `pnpm test:visual` (Playwright).
- **E2E**: `pnpm test:e2e` (Playwright).

### 7.2 The feedback loop

The AI's productivity depends on fast feedback:

- Rust builds: use `sccache` + incremental.
- Tests: use `cargo-nextest` (parallel, fast).
- UI tests: use Vite's HMR.
- Visual tests: run only on affected components.

### 7.3 When tests fail

The AI must:

1. Read the failure carefully.
2. Reproduce it locally.
3. Fix the root cause (not the test).
4. Re-run.
5. Only then commit.

### 7.4 Golden tests

For agent behavior, use golden files:

- Input: a task.
- Output: expected tool calls, expected final answer.
- Store as JSON. Compare on every run.

This catches regressions in agent behavior.

---

## 8. The publishing pipeline

### 8.1 Releases

- Tag: `v0.1.0`, `v0.2.0`, etc.
- GitHub Actions builds all platforms.
- Artifacts uploaded to GitHub Releases.
- Auto-update feed updated.
- Changelog generated from commits.

### 8.2 Channels

- **stable**: tagged releases.
- **beta**: pre-releases.
- **nightly**: every push to `main`.

### 8.3 Signing

- **Windows**: code-signing certificate (optional initially).
- **macOS**: Apple Developer certificate (required for notarization).
- **Linux**: GPG-signed packages.

### 8.4 Distribution

- **Windows**: `.msi`, `.exe`, Scoop, Winget.
- **macOS**: `.dmg`, Homebrew Cask.
- **Linux**: `.AppImage`, `.deb`, `.rpm`, Flatpak, Snap, AUR.
- **Android**: `.apk` + Play Store (later).

### 8.5 The first release

For the first release, don't worry about code signing or all channels.
Ship:

- A GitHub Release with binaries for Windows, macOS, Linux.
- A minimal README on the repo.
- A CHANGELOG.

Iterate from there.

---

## 9. How to prompt the AI

### 9.1 The structure of a good prompt

```
Context:
  [Which doc(s) are relevant]
  [Which crate/package]
  [Which phase of the roadmap]

Task:
  [What to build, in one sentence]

Details:
  [Constraints, edge cases, acceptance criteria]

Deliverables:
  [Files to create/modify]
  [Tests to write]
  [Docs to update]

Do not:
  [Things to avoid]
```

### 9.2 Example

```
Context:
  See ARCHITECTURE.md §4 (event bus) and STACK.md §4.4.
  Working in crates/events/.

Task:
  Implement the event bus with SQLite persistence.

Details:
  - Events are ULIDs, monotonic.
  - Persist to the `events` table (see DATA_MODEL.sql).
  - Support publish, subscribe (with topic filter), replay (from event id).
  - Backpressure: bounded channel, drop oldest with `events.dropped` notice.
  - Async API using tokio.

Deliverables:
  - crates/events/src/lib.rs
  - crates/events/src/store.rs
  - crates/events/tests/bus.rs (integration tests)

Do not:
  - Add new dependencies without justification.
  - Introduce global state.
  - Skip tests.
```

### 9.3 One task per prompt

Do not batch unrelated tasks. One prompt, one deliverable.

### 9.4 Reference, don't repeat

Point the AI to the docs. Don't paste the spec into the prompt; the AI has
access to the docs.

### 9.5 Ask for a plan first

For non-trivial tasks, ask the AI to:

1. Write a short plan.
2. Get your approval.
3. Then implement.

This catches misunderstandings early.

---

## 10. Iterating on features

### 10.1 Small steps

Each PR should be small enough to review in < 30 minutes.

### 10.2 Vertical slices

Prefer a thin slice that works end-to-end over a thick layer that doesn't.

### 10.3 Refactor as you go

If a pattern emerges, refactor. Don't let technical debt accumulate.

### 10.4 Update docs

Every PR that changes behavior updates the relevant doc. The AI does this
automatically if instructed.

---

## 11. Debugging with the AI

### 11.1 Give the AI

- The full error message.
- The reproduction steps.
- The relevant code.
- The expected behavior.

### 11.2 Ask for

- A hypothesis.
- A minimal reproduction.
- A fix with tests.

### 11.3 Don't

- Let the AI guess.
- Let the AI disable tests.
- Let the AI swallow errors.

---

## 12. Reviewing AI output

### 12.1 What to check

- Does it match the spec?
- Does it follow the styleguide?
- Are there tests?
- Do the tests pass?
- Is it readable?
- Are there edge cases missed?
- Is there dead code?
- Are there new dependencies? (Justify.)
- Is it secure? (Permissions, sandbox.)

### 12.2 When to reject

- Tests are missing or weak.
- The spec is not followed.
- New dependencies without justification.
- The code is unreadable.
- Security is compromised.

### 12.3 When to accept

- It works.
- It's tested.
- It's readable.
- It matches the docs.
- CI is green.

---

## 13. Working with the styleguide

### 13.1 The rule

Every UI change must:

1. Use only tokens from `/style/tokens.css`.
2. Follow the spacing/radii/motion scales.
3. Match an existing component or extend `STYLEGUIDE_EXTENSIONS.md`.
4. Pass accessibility checks.

### 13.2 The AI's prompt

When asking for UI work, always include:

```
Follow /style/ religiously.
Do not hardcode colors, spacing, or motion.
Use tokens and components from /style/ and STYLEGUIDE_EXTENSIONS.md.
If a new component is needed, add it to STYLEGUIDE_EXTENSIONS.md first.
```

### 13.3 Visual review

The AI can take screenshots (if the environment supports it) and compare
them to expected designs. Otherwise, the architect reviews visually.

---

## 14. Milestones and checkpoints

### 14.1 Per phase

At the end of each roadmap phase:

- All acceptance criteria met.
- Docs updated.
- Retrospective written.
- Next phase planned.

### 14.2 Weekly

- Review the week's PRs.
- Update the roadmap if needed.
- Triage issues.

### 14.3 Daily

- Check CI.
- Review open PRs.
- Unblock the AI.

---

## 15. Common pitfalls

| Pitfall | Symptom | Fix |
|---|---|---|
| Batching too much in one prompt | AI loses focus | One deliverable per prompt |
| Not giving context | AI invents APIs | Always reference the docs |
| Skipping tests | Regressions | Require tests in the prompt |
| Ignoring the styleguide | Inconsistent UI | Reference `/style/` every time |
| Over-engineering | Slow progress | Push for the minimal slice |
| Under-documenting | Confusion later | Update docs in every PR |
| Not committing often | Lost work | Commit after each passing test |
| Ignoring warnings | Accumulating debt | `-D warnings` in CI |
| Trusting without verifying | Silent bugs | Review every PR |
| Not running the app | "Works in tests" | Run the app manually each week |

---

## 16. Checklist: first week

- [ ] Create GitHub repo (private).
- [ ] Set up devcontainer.
- [ ] Verify Rust, Node, Tauri, wasm-tools all work.
- [ ] `cargo new` + `pnpm init` + Cargo workspace + pnpm workspace.
- [ ] Commit the initial skeleton.
- [ ] Set up CI (`ci.yml`).
- [ ] Give the AI the docs.
- [ ] Prompt: "Set up the monorepo per MONOREPO.md."
- [ ] Review and merge.
- [ ] Prompt: "Implement the kernel skeleton per ROADMAP.md Phase 0."
- [ ] Iterate.

## 17. Checklist: first month

- [ ] Phase 0 complete (kernel runs, desktop + CLI connect).
- [ ] Event bus working with persistence.
- [ ] Protocol codegen working (Rust + TS).
- [ ] First integration test passing.
- [ ] CI green on all platforms.
- [ ] Nightly builds available.
- [ ] Phase 1 started (brain).

## 18. Checklist: first release

- [ ] MVP complete (end of Phase 3).
- [ ] All acceptance criteria met.
- [ ] Docs complete.
- [ ] Tests pass on all platforms.
- [ ] Accessibility audit passed.
- [ ] Performance targets met.
- [ ] Release build on GitHub.
- [ ] CHANGELOG written.
- [ ] Public announcement (if desired).

---

## 19. Appendix: prompt templates

### 19.1 Setup

```
Set up the monorepo per MONOREPO.md.
- Cargo workspace with crates: core, storage, events, protocol, api, supervisor.
- pnpm workspace with packages: ui, protocol.
- Rust: rust-toolchain.toml, rustfmt.toml, clippy config.
- Node: package.json, tsconfig, eslint, prettier.
- CI: .github/workflows/ci.yml.
- Devcontainer: .devcontainer/.
- README, LICENSE, CONTRIBUTING, CODE_OF_CONDUCT.
Commit as "chore: initial monorepo setup".
```

### 19.2 Kernel skeleton

```
Implement the kernel skeleton per ROADMAP.md Phase 0 and
ARCHITECTURE.md §3.
- `fergentsd` binary with `start`, `stop`, `status`.
- Local socket server (JSON-RPC 2.0).
- Health endpoint.
- Structured logging with `tracing`.
- Graceful shutdown.
- Configuration from `~/.fergents/config.toml`.
- Tests for the socket protocol.
```

### 19.3 Event bus

(see §9.2)

### 19.4 LLM router

```
Implement the LLM router per LLM.md §4 and STACK.md §7.
- `Provider` trait.
- Implementations: OpenAI, Anthropic, Ollama.
- Routing from TOML config.
- Streaming.
- Cost tracking.
- Tests with mocked providers.
```

### 19.5 Chat UI

```
Implement the chat UI per UI.md §7 and STYLEGUIDE_EXTENSIONS.md §1-8.
- Message bubbles (user, assistant, tool, system).
- Streaming with cursor.
- Thinking block (collapsible).
- Tool call cards.
- Permission bar and card.
- Composer with attachments, mentions, commands.
- Follow /style/ religiously.
```

### 19.6 Plugin

```
Implement the plugin host per PLUGINS.md §4-6.
- WASM host with wasmtime.
- Host functions (fs, http, llm, mcp, storage, events, log).
- Permission checking.
- Load, init, shutdown lifecycle.
- A sample "hello" plugin.
- Tests.
```

### 19.7 Tests

```
Write integration tests for <feature> per the test plan in STACK.md §21.
- Happy path.
- Error cases.
- Edge cases.
- Property-based tests where applicable.
- Use `cargo-nextest` for speed.
```

---

*End of `AI_WORKFLOW.md`.*