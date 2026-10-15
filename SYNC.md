# Sync

> Multi-device synchronization: LAN-first P2P, WAN fallback, CRDTs, conflict
> resolution, and the future self-hosted server.

**Audience:** developers, AI developers.
**Companion docs:** `ARCHITECTURE.md`, `BRAIN.md`, `SECURITY.md`.

---

## Table of contents

1. [Principles](#1-principles)
2. [Topologies](#2-topologies)
3. [Discovery](#3-discovery)
4. [Pairing](#4-pairing)
5. [Transport](#5-transport)
6. [What syncs](#6-what-syncs)
7. [Conflict resolution](#7-conflict-resolution)
8. [Scope and permissions](#8-scope-and-permissions)
9. [LAN-first behavior](#9-lan-first-behavior)
10. [WAN fallback](#10-wan-fallback)
11. [Self-hosted server](#11-self-hosted-server)
12. [Encryption](#12-encryption)
13. [Offline mode](#13-offline-mode)
14. [Performance](#14-performance)
15. [Observability](#15-observability)
16. [AI operations](#16-ai-operations)
17. [Examples](#17-examples)

---

## 1. Principles

- **LAN-first**: if two devices are on the same network, they sync directly.
  No cloud. No internet.
- **P2P by default**: no central server required.
- **Selective**: you choose what to sync, per brain, per project.
- **Encrypted**: E2E with device keys.
- **Conflict-free**: CRDTs for markdown, event log merge for structured
  data.
- **Resumable**: sync can be interrupted and resumed.
- **Optional server**: a self-hosted relay for teams, replacing P2P when
  convenient.

---

## 2. Topologies

### 2.1 Direct P2P (LAN)

Two devices on the same Wi-Fi sync directly, using mDNS for discovery and
QUIC for transport.

```
┌──────────┐                    ┌──────────┐
│ Desktop  │◀──── LAN ────────▶│ Laptop   │
└──────────┘                    └──────────┘
```

### 2.2 Direct P2P (WAN)

Two devices on different networks sync via hole punching (libp2p), falling
back to a relay if needed.

```
┌──────────┐                    ┌──────────┐
│ Desktop  │◀─── internet ────▶│ Laptop   │
└──────────┘   (hole punch)    └──────────┘
     │                              │
     └─────────── relay ────────────┘
                (if needed)
```

### 2.3 Star (via server)

A self-hosted Fergents server acts as a hub:

```
        ┌──────────┐
        │  Server  │
        └────┬─────┘
     ┌───────┼───────┐
     ▼       ▼       ▼
┌────────┐┌────────┐┌────────┐
│Device 1││Device 2││Device 3│
└────────┘└────────┘└────────┘
```

### 2.4 Mobile companion

The mobile app connects to the desktop kernel over LAN (preferred) or WAN.

```
┌──────────┐                    ┌──────────┐
│ Desktop  │◀─── LAN/WAN ──────▶│ Android  │
└──────────┘                    └──────────┘
```

---

## 3. Discovery

### 3.1 LAN (mDNS)

- Service: `_fergents._udp.local`
- Advertises: device id, name, platform, capabilities, fingerprint.
- Discovery: automatic, continuous.
- No configuration.

### 3.2 WAN (DHT + rendezvous)

- libp2p Kademlia DHT for peer routing.
- A small rendezvous service (self-hostable) for initial contact.
- Peers are identified by their public key.

### 3.3 Manual

A peer can be added manually by address (for networks without mDNS).

---

## 4. Pairing

### 4.1 Why pairing

Peers must be explicitly paired. This prevents unauthorized devices from
syncing.

### 4.2 Flow

1. Device A shows a QR code or a short code.
2. Device B scans or enters the code.
3. Both devices exchange public keys.
4. Both users confirm the pairing (with a fingerprint check).
5. The pairing is stored in `peers`.

### 4.3 Fingerprints

Each device has a public key (X25519). The fingerprint is a short hash
(e.g. `4A2F-9B1C-7D3E`). Both users verify the fingerprints match. This
prevents MITM attacks during pairing.

### 4.4 Revocation

A paired device can be revoked at any time. Revocation is propagated to all
other peers.

---

## 5. Transport

### 5.1 libp2p

- **QUIC** for transport (fast, multiplexed, encrypted by default).
- **Noise** for authenticated encryption.
- **Yamux** for multiplexing streams.
- **Hole punching** for NAT traversal.
- **Relay** as fallback.

### 5.2 Streams

Each sync uses multiple streams:

- **Control**: pairing, capability exchange, status.
- **Data**: file transfers, event log.
- **Realtime**: live collaboration (chat, cursors).

### 5.3 Compression

Payloads are compressed (zstd) before transmission. Small payloads
(< 1 KB) are sent uncompressed.

### 5.4 Resumption

Interrupted transfers resume from the last acknowledged chunk.

---

## 6. What syncs

### 6.1 Per brain

- **Markdown files**: via CRDT (Automerge).
- **Attachments**: via chunked transfer + hash.
- **Index**: not synced (regenerated locally).
- **Graph**: not synced (regenerated locally).
- **Config** (`brain.toml`): via event log.
- **Backups** (`.fergents/`): not synced by default.

### 6.2 Global

- **Projects**: metadata via event log.
- **Agents**: definitions via file sync.
- **Skills**: definitions via file sync.
- **Workflows**: definitions via file sync.
- **Plugins**: not synced (installed per device).
- **Settings**: per-device by default; some sync (opt-in).
- **Sessions**: not synced.
- **Chat history**: optional, per-brain.

### 6.3 Realtime

- **Chat messages**: streamed live between paired devices.
- **Agent sessions**: a session can be shared; both devices see the same
  stream.
- **Permission requests**: routed to the primary device.
- **Cursors** (future): live collaboration.

---

## 7. Conflict resolution

### 7.1 Markdown: CRDT

Markdown files are synced via **Automerge** (or Yjs). Each device has a
replica; edits are merged automatically. Conflicts are resolved by the CRDT
algorithm (no data loss).

When two devices edit the same note offline, on reconnect:

1. Both replicas exchange changes.
2. Automerge merges them.
3. The merged result is written to disk.
4. Both devices see the same content.

### 7.2 Structured data: event log merge

For tables (documents, links, tags, agents, workflows), changes are events.
Events are ordered by:

- Vector clocks (causal ordering).
- Timestamps (for concurrent events).
- Device ID (tie-breaker).

Last-writer-wins for scalar fields, set-union for collection fields.

### 7.3 Binary files

Last-writer-wins with conflict copies:

- If both devices modify the same binary offline, the newer one wins.
- The older version is saved as `<file>.<timestamp>.conflict`.

### 7.4 Config

`brain.toml`, `project.toml`: merged field by field. Conflicting scalar
fields use last-writer-wins.

### 7.5 Conflict UI

When a conflict is detected:

- The user is notified.
- A diff is shown (if textual).
- The user can choose which version to keep, or merge manually.
- The resolution is propagated to all peers.

---

## 8. Scope and permissions

### 8.1 Per-brain scope

Each brain has a sync scope:

- `full`: sync everything.
- `notes-only`: sync markdown + attachments, not config or backups.
- `custom`: explicit include/exclude patterns.

### 8.2 Per-peer scope

Each peer can be granted access to specific brains:

```toml
[[peers]]
id = "01J8K..."
brains = ["personal", "research"]
mode = "read-write"   # or "read-only"
```

### 8.3 Revocation

Revoking a peer:

- Removes their access to all shared resources.
- Propagates to other peers (so they don't re-share).
- Optionally rotates encryption keys.

---

## 9. LAN-first behavior

### 9.1 Requirement

If two devices are on the same Wi-Fi, they **must** use the LAN for sync.
This is a hard requirement: no cloud dependency when on the same network.

### 9.2 Detection

mDNS advertises presence on the LAN. When a peer is discovered on the LAN,
it is preferred over WAN.

### 9.3 Parallel work

On the same LAN, two devices can:

- Share chat messages in real time.
- Share agent sessions.
- Collaborate on the same note (future: live cursors).
- Trigger workflows that affect both.

### 9.4 Fallback

If mDNS fails (some networks block it), manual IP entry or WAN is used.

---

## 10. WAN fallback

### 10.1 Hole punching

libp2p attempts direct hole punching using STUN-like techniques.

### 10.2 Relay

If hole punching fails, a relay is used:

- **Public relays**: free, run by the community.
- **Self-hosted relay**: run by the user (a Fergents instance with
  `--relay`).

### 10.3 Relay privacy

Relays only forward encrypted traffic. They cannot read the content.

### 10.4 Cost

Public relays are free but may be slow. Self-hosted relays are fast but
require a VPS.

---

## 11. Self-hosted server

### 11.1 Concept

A Fergents instance can run headless with `--server`. It:

- Acts as a relay.
- Optionally hosts shared brains for a team.
- Provides HTTP API for mobile and external clients.
- Manages users, roles, quotas.

### 11.2 Deployment

- Docker Compose.
- Single binary.
- Helm chart (future).

### 11.3 Data

The server stores:

- User accounts.
- Encrypted shared brains.
- Event logs.
- Audit logs.

### 11.4 Access control

- Roles: admin, member, guest.
- Quotas per user (storage, tokens).
- Per-brain access lists.

### 11.5 When to use a server

- Teams that want a single source of truth.
- Devices that can't do P2P (corporate firewalls).
- Mobile devices that need a stable endpoint.

---

## 12. Encryption

### 12.1 Identity

Each device has an X25519 key pair, stored in the OS keychain.

### 12.2 Pairing

During pairing, public keys are exchanged and verified.

### 12.3 Session

Each sync session uses a Noise XX handshake, establishing a shared secret.

### 12.4 Data

- **In transit**: encrypted by QUIC + Noise.
- **At rest**: optional SQLCipher per brain.
- **Server**: encrypted at rest; server cannot read content (E2E).

### 12.5 Key rotation

Keys can be rotated at any time. Rotation is propagated to peers.

---

## 13. Offline mode

### 13.1 Concept

A device can work offline. Changes are queued and synced when a peer is
reachable.

### 13.2 Queue

- File changes: stored as CRDT operations.
- Structured changes: stored as events.
- The queue is persistent (survives restarts).

### 13.3 Merge

On reconnect:

1. Exchange the operation/event logs since the last sync.
2. Apply missing operations.
3. Resolve conflicts (CRDT or last-writer-wins).
4. Confirm sync.

### 13.4 Long offline periods

If a device is offline for a long time, the queue may grow. Fergents:

- Compacts the queue periodically (merges operations on the same file).
- Warns if the queue exceeds a threshold.
- Offers to sync via a smaller scope.

---

## 14. Performance

### 14.1 Targets

| Operation | Target |
|---|---|
| Discover peer on LAN | < 2 s |
| Initial sync (empty brain) | < 5 s |
| Initial sync (1 GB brain) | < 5 min |
| Incremental sync (1 file) | < 500 ms |
| Realtime message latency | < 100 ms |

### 14.2 Techniques

- **Delta sync**: only changed parts.
- **Compression**: zstd.
- **Parallel streams**: multiple files in parallel.
- **Resumption**: interrupted transfers resume.
- **LAN prioritization**: LAN is preferred over WAN.

---

## 15. Observability

Per peer:

- Connection status (LAN/WAN/offline).
- Last sync time.
- Bytes transferred.
- Errors.

Per brain:

- Sync status.
- Conflicts.
- Pending changes.

All in the dashboard.

---

## 16. AI operations

The AI can:

- **List** paired peers.
- **Show** sync status.
- **Trigger** a manual sync.
- **Diagnose** sync issues.
- **Suggest** scope changes.
- **Explain** conflicts.

The AI cannot:

- Pair new devices without user approval.
- Revoke peers without confirmation.
- Change encryption keys.

---

## 17. Examples

### 17.1 Pairing two desktops

```
Desktop A: Settings → Sync → Pair a device → shows QR.
Desktop B: Settings → Sync → Scan QR → confirms fingerprints.
Both: Synced. Brains appear in both.
```

### 17.2 LAN sync

```
Desktop A and Laptop B on the same Wi-Fi.
A edits a note.
B sees the change in < 1 s.
No internet used.
```

### 17.3 WAN sync

```
Desktop at home, laptop at office.
Both online, hole punching succeeds.
Sync happens directly.
```

### 17.4 Offline merge

```
Laptop offline, edits 3 notes.
Desktop edits 2 of the same notes.
Laptop reconnects.
Automerge merges all edits without loss.
```

### 17.5 Mobile companion

```
Android app paired with desktop.
User chats with an agent on mobile.
Agent runs on desktop kernel.
Responses streamed to mobile.
```

---

*End of `SYNC.md`.*