# Security

> Permissions, sandboxing, cryptography, reversibility, audit, and the
> threat model.

**Audience:** developers, AI developers, security reviewers.
**Companion docs:** `ARCHITECTURE.md`, `AGENTS.md`, `SYNC.md`, `MCP.md`.

---

## Table of contents

1. [Threat model](#1-threat-model)
2. [Security principles](#2-security-principles)
3. [The permission system](#3-the-permission-system)
4. [The permission bar](#4-the-permission-bar)
5. [Policy engine](#5-policy-engine)
6. [Sandboxing layers](#6-sandboxing-layers)
7. [Filesystem sandbox](#7-filesystem-sandbox)
8. [Shell sandbox](#8-shell-sandbox)
9. [Network sandbox](#9-network-sandbox)
10. [Plugin sandbox](#10-plugin-sandbox)
11. [Agent sandbox](#11-agent-sandbox)
12. [MCP security](#12-mcp-security)
13. [Cryptography](#13-cryptography)
14. [Secrets management](#14-secrets-management)
15. [Reversibility](#15-reversibility)
16. [Audit log](#16-audit-log)
17. [Prompt injection](#17-prompt-injection)
18. [Supply chain](#18-supply-chain)
19. [Incident response](#19-incident-response)
20. [AI operations](#20-ai-operations)
21. [Observability](#21-observability)
22. [Appendix: permission categories](#22-appendix-permission-categories)

---

## 1. Threat model

### 1.1 What we protect

- **User data**: notes, code, memory, secrets.
- **User identity**: device keys, API keys.
- **User system**: files, processes, network.
- **User attention**: no surprise costs, no hidden actions.
- **User trust**: every action is auditable and reversible.

### 1.2 Adversaries

| Adversary | Capability | Goal |
|---|---|---|
| **Malicious plugin** | Arbitrary code in WASM sandbox | Exfiltrate data, escalate |
| **Malicious MCP server** | Return crafted content | Prompt injection, data exfil |
| **Compromised LLM** | Return crafted tool calls | Same |
| **Malicious agent** | A user-created agent with bad intent | Same |
| **Network attacker** | Intercept traffic | Read/modify data |
| **Malicious peer** | Paired device gone rogue | Exfiltrate shared data |
| **Supply chain** | Compromised dependency | Backdoor |
| **Physical access** | Access to the device | Read data at rest |
| **Insider** | A user with legitimate access | Escalate beyond scope |

### 1.3 Out of scope

- Nation-state adversaries with physical access.
- Side-channel attacks on the CPU.
- Compromised OS.

---

## 2. Security principles

1. **Least privilege**: everything runs with the minimum permissions needed.
2. **Defense in depth**: multiple independent layers.
3. **Fail closed**: if a check fails, deny.
4. **Explicit consent**: every new capability is user-approved.
5. **Reversibility**: every mutation is undoable.
6. **Auditability**: every action is logged.
7. **Transparency**: no hidden behavior.
8. **Local-first**: data stays on device unless explicitly shared.
9. **Zero trust in content**: treat all external content as untrusted.
10. **No privileged AI**: the AI uses the same APIs as humans.

---

## 3. The permission system

### 3.1 Categories

Every tool declares a permission category:

| Category | Scope | Examples |
|---|---|---|
| `fs.read` | Paths | Read a file |
| `fs.write` | Paths | Write, delete, move |
| `shell.exec` | Commands | Run a process |
| `net.http` | Domains | Fetch a URL |
| `net.p2p` | Peers | Connect to a peer |
| `llm.call` | Models | Call an LLM |
| `mcp.connect` | Servers | Connect to MCP |
| `plugin.install` | — | Install a plugin |
| `agent.spawn` | Agents | Spawn a sub-agent |
| `workflow.edit` | Workflows | Modify a workflow |
| `brain.write` | Brains | Modify a brain |
| `system.config` | Keys | Change settings |

### 3.2 Scope

Permissions are scoped:

- **fs.read/write**: a list of glob patterns.
- **shell.exec**: an allowlist of commands.
- **net.http**: a list of domains.
- **llm.call**: a list of models.
- **mcp.connect**: a list of server ids.
- **brain.write**: a list of brain ids.
- **system.config**: a list of keys.

### 3.3 Grant

Permissions are granted:

- **At install** (plugins, skills): the user reviews and approves.
- **At first use** (agents, MCP): the user is prompted.
- **By policy**: some permissions are auto-granted based on rules.

### 3.4 Revocation

Any permission can be revoked at any time. The effect is immediate:
subsequent calls fail.

### 3.5 Audit

Every permission check is logged:

- Who (actor).
- What (category, scope).
- When.
- Result (granted, denied, auto).
- Reason (policy id, user approval).

---

## 4. The permission bar

### 4.1 Three modes

In the chat, the user selects:

| Mode | Behavior |
|---|---|
| **Manual** | Every tool call requires approval. |
| **Auto** | Safe operations run automatically; risky ones prompt. |
| **Full** | Everything runs automatically. |

### 4.2 Risk classification

Each tool call is classified:

- **Safe**: read-only, no cost, no side effects.
- **Moderate**: writes within workspace, calls LLM within budget.
- **Risky**: writes outside workspace, runs shell, network to new domain,
  cost above threshold.

In Auto mode, safe and moderate run automatically; risky prompts.

### 4.3 Per-tool overrides

The user can override the mode for specific tools:

```toml
[permissions.overrides]
"shell.exec" = "prompt"   # always prompt
"fs.write" = "allow"      # always allow (within workspace)
```

### 4.4 Session vs global

The mode is per-session by default. The user can set a global default in
settings.

---

## 5. Policy engine

### 5.1 Rules

Policy rules are declarative TOML:

```toml
[[rule]]
id = "allow-safe-shell"
match = { category = "shell.exec", command = ["ls", "cat", "grep", "find"] }
action = "allow"

[[rule]]
id = "deny-rm-root"
match = { category = "shell.exec", command_starts_with = "rm -rf /" }
action = "deny"
reason = "destructive"

[[rule]]
id = "prompt-new-domain"
match = { category = "net.http", domain_not_in = "$SESSION_DOMAINS" }
action = "prompt"
```

### 5.2 Evaluation

Rules are evaluated in order. First match wins.

### 5.3 Actions

- `allow`: the call proceeds.
- `prompt`: the user is asked.
- `deny`: the call is rejected.

### 5.4 AI-proposed rules

The AI can propose new rules (e.g. after a repeated prompt, suggest an
allowlist entry). The user approves.

### 5.5 Defaults

Sensible defaults ship with Fergents. Users can extend, override, or
replace.

---

## 6. Sandboxing layers

```
┌──────────────────────────────────────────────────┐
│  LAYER 5: CRYPTOGRAPHY                          │
│  Encryption at rest, in transit, E2E            │
├──────────────────────────────────────────────────┤
│  LAYER 4: RESOURCE LIMITS                       │
│  Tokens, cost, time, CPU, memory                │
├──────────────────────────────────────────────────┤
│  LAYER 3: NETWORK SANDBOX                       │
│  Per-plugin/agent domain allowlists             │
├──────────────────────────────────────────────────┤
│  LAYER 2: OS SANDBOX                            │
│  Landlock, sandbox-exec, Job Objects            │
├──────────────────────────────────────────────────┤
│  LAYER 1: WASM SANDBOX                          │
│  Plugins, skills, user code                     │
├──────────────────────────────────────────────────┤
│  LAYER 0: PERMISSIONS                           │
│  Every call checked before execution            │
└──────────────────────────────────────────────────┘
```

Each layer is independent. A compromise in one doesn't break the others.

---

## 7. Filesystem sandbox

### 7.1 Scope

Each agent, plugin, or skill has a declared filesystem scope:

- **Workspace root**: the project folder.
- **Brain vault**: the active brain.
- **Custom paths**: explicitly granted.

### 7.2 Enforcement

- **At the kernel level**: paths are resolved and checked against the scope
  before any FS operation.
- **At the OS level**: the process runs with a restricted view (Landlock,
  sandbox-exec, Job Objects).

### 7.3 Symlinks

Symlinks are resolved to their real paths before checking. Symlinks pointing
outside the scope are rejected.

### 7.4 Path traversal

Paths are normalized (`..` resolved). Attempts to escape the scope are
rejected.

### 7.5 Case sensitivity

On case-insensitive filesystems (Windows, macOS), paths are compared
case-insensitively.

---

## 8. Shell sandbox

### 8.1 Linux / WSL: Landlock

- The process runs under Landlock LSM.
- File system access restricted to declared paths.
- Network restricted via network namespaces or nftables rules.
- No new privileges (`no_new_privs`).

### 8.2 macOS: sandbox-exec

- A sandbox profile restricts file system and network access.
- Deprecated but still functional; alternative: `sandbox_init`.

### 8.3 Windows: Job Objects

- Restricted token.
- Job Object with UI restrictions, process limits, and (where possible)
  AppContainer isolation.

### 8.4 Fallback

If OS-level sandboxing isn't available:

- The command runs normally.
- It requires explicit user approval.
- It is logged in the audit log with a warning.

### 8.5 Command allowlist

In Auto mode, only allowlisted commands run automatically. Others prompt.

---

## 9. Network sandbox

### 9.1 Domain allowlists

Every plugin, agent, and MCP server declares allowed domains. The kernel
proxies all outbound HTTP and enforces the allowlist.

### 9.2 Enforcement

- All HTTP requests go through the kernel's proxy.
- The proxy checks the domain against the allowlist.
- Requests to non-allowed domains are rejected (or prompt, per policy).

### 9.3 TLS

- TLS 1.3 only.
- Certificate verification always on.
- Certificate pinning optional (per server).

### 9.4 Localhost

Localhost traffic (Ollama, llama.cpp, local MCP servers) is always allowed.

### 9.5 DNS

DNS queries go through the kernel. DNS-over-HTTPS is used if configured.

---

## 10. Plugin sandbox

### 10.1 WASM isolation

- Plugins run in wasmtime with the Component Model.
- No ambient authority: no FS, no network, no env vars.
- All capabilities via host functions.
- Memory and CPU limits.

### 10.2 Host functions

Every host function checks the plugin's permissions before executing. For
example, `fs.read(path)` checks:

- Does the plugin have `fs.read` permission?
- Is `path` within the plugin's declared scope?

### 10.3 UI plugins

UI plugins run in Shadow DOM, isolated from the host:

- No access to host DOM.
- No access to host JS globals.
- Communication only via a message-passing bridge.

### 10.4 Signatures

Official plugins are signed. Unsigned plugins are allowed but require
explicit user consent with a warning.

---

## 11. Agent sandbox

### 11.1 Scope

An agent's scope is defined by:

- The session's permission mode.
- The agent's declared permissions.
- The project's workspace root.
- The active brain(s).

### 11.2 File modifications

Every file modification by an agent is:

1. Checked against the scope.
2. Backed up (shadow git or `.fergents`).
3. Logged in the audit log.
4. Reversible.

### 11.3 Command execution

Commands run in the shell sandbox (see §8). Allowlisted commands run
automatically; others prompt.

### 11.4 Network

Network access is restricted to declared domains. New domains prompt.

### 11.5 Resource limits

Agents have:

- Token limits.
- Cost limits.
- Time limits.
- Tool call limits.
- Sub-agent limits.

When a limit is hit, the agent pauses and the user is notified.

---

## 12. MCP security

### 12.1 Untrusted servers

MCP servers are untrusted by default. Their output is:

- Wrapped in `<untrusted>` tags.
- Treated as data, not instructions.
- Sanitized for obvious injection patterns.

### 12.2 Permission gates

Every MCP capability is permissioned:

- `mcp.connect`: connect to a new server.
- `tool.invoke`: invoke a tool.
- `resource.read`: read a resource.
- `prompt.get`: fetch a prompt.
- `sampling.request`: allow the server to sample.

### 12.3 Sampling limits

Servers that sample from the LLM are limited:

- Max tokens per request.
- Max requests per minute.
- Cost tracking.

### 12.4 Sandbox

stdio servers run under the OS-level sandbox (Landlock, etc.). Remote
servers are network-restricted.

### 12.5 Audit

Every MCP call is logged with:

- Server, tool, arguments (hashed for sensitive data).
- Duration, result size.
- Status.

---

## 13. Cryptography

### 13.1 Primitives

- **XChaCha20-Poly1305** for symmetric encryption (via libsodium).
- **X25519** for key exchange.
- **Ed25519** for signatures.
- **Argon2id** for password-based key derivation.
- **SHA-256** for hashing.
- **BLAKE3** for fast hashing (cache keys, content hashes).

### 13.2 At rest

- **SQLCipher** (optional, per brain): full-database encryption.
- **OS keychain**: for API keys, device keys, secrets.
- **File encryption**: none by default (markdown is plain); users can
  enable per-brain encryption.

### 13.3 In transit

- **Local IPC**: OS-level (named pipes, Unix sockets).
- **Remote**: QUIC + Noise (P2P), TLS 1.3 (HTTP).
- **API calls**: TLS 1.3, certificate pinning optional.

### 13.4 Key management

- Device keys: generated on first run, stored in the OS keychain.
- API keys: stored in the OS keychain.
- Secrets: stored in the OS keychain.
- Backups: keys are never included in `.fergbrain` archives.

### 13.5 Rotation

Keys can be rotated at any time. Rotation is propagated to peers.

---

## 14. Secrets management

### 14.1 Storage

Secrets (API keys, tokens) are stored in the OS keychain:

- Windows: Credential Manager.
- macOS: Keychain.
- Linux: libsecret (GNOME Keyring, KWallet).

### 14.2 Access

- The kernel resolves `$KEYCHAIN:name` references.
- Plugins and agents never see raw keys.
- The AI never sees raw keys.
- The UI shows masked values.

### 14.3 Export

Secrets are never exported in backups or `.fergbrain` archives. The user
must re-enter them on a new device.

### 14.4 Rotation

Users can rotate secrets at any time. The kernel updates references
automatically.

---

## 15. Reversibility

### 15.1 Shadow git

Every project has a shadow git repo at `.fergents/shadow.git`. Every agent
action that modifies files commits automatically:

```
[fergents] coder: edit src/main.rs
[fergents] researcher: write notes/mcp-security.md
[fergents] librarian: rename notes/foo.md → notes/bar.md
```

### 15.2 `.fergents` backups

For files outside a git repo, a timestamped copy is created before
modification:

```
<file>.<timestamp>.fergents
```

For example, `settings.toml.2025-10-14T09-00-00.fergents`.

### 15.3 Restore

- **Per-file**: restore from the last backup.
- **Per-session**: revert all changes from a session.
- **Per-run**: revert all changes from a run.
- **Global**: revert everything from the last N minutes.

### 15.4 UI

Every modified file shows a "Restore" button. The audit log has a "Revert"
action per entry.

### 15.5 Retention

- Shadow git: kept forever (or until the project is deleted).
- `.fergents` backups: configurable retention (default: 30 days).

---

## 16. Audit log

### 16.1 What's logged

Every action that mutates state:

- File reads/writes/deletes.
- Shell commands.
- Network requests.
- LLM calls.
- MCP calls.
- Plugin installs.
- Agent spawns.
- Workflow edits.
- Settings changes.
- Permission decisions.

### 16.2 Format

```json
{
  "id": "01J8K...",
  "ts": 1728892800123,
  "actor_type": "agent",
  "actor_id": "coder",
  "session_id": "01J8J...",
  "action": "fs.write",
  "target": "src/main.rs",
  "args_json": { "path": "src/main.rs", "size": 1234 },
  "result": "allowed",
  "reason": "policy:allow-workspace-write",
  "correlation_id": "01J8J...",
  "reversible": true,
  "revert_ref": "shadow-git:abc123"
}
```

### 16.3 Storage

Append-only table in the global SQLite DB (`audit_log`).

### 16.4 Query

- By actor, action, target, time range.
- Full-text search on args.
- Export as JSON or CSV.

### 16.5 Retention

Configurable. Default: keep forever for security-relevant actions
(permissions, installs), 90 days for others.

### 16.6 Tamper resistance

The audit log is append-only. Entries are hashed in a chain:

```
hash(n) = sha256(hash(n-1) || entry(n))
```

Any tampering breaks the chain and is detectable.

---

## 17. Prompt injection

### 17.1 The threat

External content (web pages, MCP tool results, PDFs, emails) may contain
instructions that try to manipulate the agent.

### 17.2 Mitigations

- **Untrusted markers**: all external content is wrapped in
  `<untrusted source="...">...</untrusted>`.
- **System prompt warning**: agents are instructed to ignore instructions
  inside untrusted tags.
- **Tool result sanitization**: obvious injection patterns (e.g. "ignore
  previous instructions") are flagged.
- **Least privilege**: even if the agent is manipulated, it can only do what
  its permissions allow.
- **User in the loop**: risky actions prompt the user.
- **Audit**: every action is logged and reviewable.

### 17.3 Limitations

No defense is complete. Users are warned when installing untrusted MCP
servers or plugins.

### 17.4 Reporting

If a prompt injection is detected, the user is notified, and the content is
flagged for review.

---

## 18. Supply chain

### 18.1 Dependencies

- Rust: `cargo audit`, `cargo deny` in CI.
- Node: `pnpm audit`.
- Lockfiles committed.
- Dependencies pinned.

### 18.2 Plugins

- Official plugins are signed.
- Unsigned plugins require explicit consent.
- Plugin permissions are reviewed at install.

### 18.3 Skills

- Skills are markdown + optional WASM.
- WASM is sandboxed.
- Skills can be signed (optional).

### 18.4 Updates

- Auto-update is signed.
- Stable and beta channels.
- Users can disable auto-update.

---

## 19. Incident response

### 19.1 Detection

- Anomaly detection on audit log (unusual patterns).
- User reports.
- Crash reports (opt-in).

### 19.2 Response

1. **Contain**: revoke permissions, disable plugins, disconnect peers.
2. **Investigate**: review audit log, reproduce.
3. **Fix**: patch, update, notify users.
4. **Post-mortem**: document, improve.

### 19.3 Disclosure

Security issues are disclosed responsibly:

- A private channel for reports.
- A public advisory after a fix is available.
- CVE if applicable.

---

## 20. AI operations

The AI can:

- **Review** the audit log.
- **Explain** a permission decision.
- **Suggest** policy changes.
- **Detect** anomalies.
- **Propose** a plugin/skill removal.

The AI cannot:

- **Grant** permissions to itself.
- **Disable** the audit log.
- **Modify** security policies without user approval.
- **Access** raw secrets.

---

## 21. Observability

Security-relevant metrics:

- Permission denials per category.
- Failed auth attempts.
- MCP errors.
- Plugin errors.
- Anomalies detected.

All in the security dashboard.

---

## 22. Appendix: permission categories

| Category | Scope | Default in Auto | Notes |
|---|---|---|---|
| `fs.read` | Paths | allow (workspace) | prompt outside workspace |
| `fs.write` | Paths | prompt (outside) | allow inside workspace |
| `shell.exec` | Commands | prompt | allowlist auto |
| `net.http` | Domains | prompt (new) | allow known domains |
| `net.p2p` | Peers | allow (LAN) | prompt (WAN) |
| `llm.call` | Models | allow (budget) | prompt above threshold |
| `mcp.connect` | Servers | prompt (new) | allow known |
| `plugin.install` | — | prompt | always |
| `agent.spawn` | Agents | allow (limits) | — |
| `workflow.edit` | Workflows | allow | — |
| `brain.write` | Brains | allow (scope) | prompt outside scope |
| `system.config` | Keys | prompt | always |

---

*End of `SECURITY.md`.*