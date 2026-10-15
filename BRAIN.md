# Brain

> The knowledge base: markdown vault, multi-brain model, templates, versioning,
> portability, and cross-brain operations.

**Audience:** developers, AI developers.
**Companion docs:** `ARCHITECTURE.md`, `SEARCH.md`, `GRAPH.md`, `SYNC.md`.

---

## Table of contents

1. [What a brain is](#1-what-a-brain-is)
2. [Anatomy of a brain](#2-anatomy-of-a-brain)
3. [File layout](#3-file-layout)
4. [The vault](#4-the-vault)
5. [The index](#5-the-index)
6. [Configuration](#6-configuration)
7. [Templates](#7-templates)
8. [Multi-brain operations](#8-multi-brain-operations)
9. [Lifecycle](#9-lifecycle)
10. [Portability](#10-portability)
11. [Versioning](#11-versioning)
12. [Attachments](#12-attachments)
13. [Linking across brains](#13-linking-across-brains)
14. [Import and export](#14-import-and-export)
15. [AI operations on brains](#15-ai-operations-on-brains)
16. [Performance](#16-performance)
17. [Observability](#17-observability)
18. [Examples](#18-examples)
19. [Appendix: brain.toml](#19-appendix-braintoml)

---

## 1. What a brain is

A **brain** is a portable, self-contained knowledge base. It is:

- **A folder** on disk, movable and copyable.
- **Plain markdown** as the source of truth.
- **An SQLite index** for fast search (FTS5, vectors, entities).
- **A graph** of entities and relations.
- **A configuration** (`brain.toml`) with identity and settings.
- **Optionally versioned** with git.

A brain is **not** a database in the traditional sense. It's a folder you can
zip, email, back up, or commit. Fergents can open it, index it, and reason
over it, but if Fergents disappears tomorrow, the brain is still readable in
any text editor.

### 1.1 Brain vs project

| Concept | What it is | Relationship |
|---|---|---|
| **Brain** | A knowledge base | Portable, independent |
| **Project** | A workspace | Links one or more brains |
| **Vault** | The markdown files inside a brain | The brain's content |

A brain can be attached to **multiple projects** simultaneously. A project
can link **multiple brains** (e.g. a personal brain + a work brain + a
shared team brain).

---

## 2. Anatomy of a brain

```
~/Fergents/brains/personal/
├── brain.toml              # identity + config (required)
├── vault/                  # markdown files (required)
│   ├── daily/
│   ├── notes/
│   ├── projects/
│   ├── attachments/
│   └── templates/
├── index.sqlite            # documents, chunks, embeddings, FTS
├── graph.db                # CozoDB relations (optional)
├── .git/                   # optional versioning
├── .fergents/              # backups, snapshots, logs, cache
│   ├── backups/
│   ├── snapshots/
│   ├── cache/
│   └── logs/
└── README.md               # optional human description
```

Everything except `brain.toml` and `vault/` is **regenerable**. Deleting
`index.sqlite` and `graph.db` is safe; they'll be rebuilt on next open.

---

## 3. File layout

### 3.1 Required

- **`brain.toml`**: the brain's identity and configuration.
- **`vault/`**: the markdown files.

### 3.2 Regenerable

- **`index.sqlite`**: chunks, embeddings, FTS, notes_meta.
- **`graph.db`**: CozoDB relations.
- **`.fergents/cache/`**: temporary caches.

### 3.3 Optional

- **`.git/`**: if the user enables git versioning.
- **`.fergents/backups/`**: `.fergents` backups of agent modifications.
- **`.fergents/snapshots/`**: periodic `.fergbrain` archives.
- **`README.md`**: a human description.

### 3.4 Ignored

The following are ignored by the indexer and by sync:

- `.fergents/`
- `index.sqlite*` (WAL, SHM)
- `graph.db*`
- `.DS_Store`, `Thumbs.db`
- Any file matched by `.brainignore` (gitignore syntax).

---

## 4. The vault

### 4.1 Structure

The vault is a folder of markdown files. There is no required structure, but
the default template suggests:

```
vault/
├── daily/              # daily notes (YYYY-MM-DD.md)
├── notes/              # atomic notes
├── projects/           # project-specific notes
├── people/             # people notes
├── sources/            # references, clippings
├── attachments/        # images, PDFs, other binaries
├── templates/          # note templates
└── index.md            # optional home note
```

Users can organize however they want. Folders are just paths.

### 4.2 Markdown flavor

Fergents supports:

- **CommonMark** as the base.
- **GFM tables** and task lists.
- **Wiki links**: `[[note]]`, `[[note|display]]`, `[[note#heading]]`.
- **Embeds**: `![[note]]`, `![[image.png]]`.
- **Block refs**: `^block-id` (at end of a block).
- **Block links**: `[[note#^block-id]]`.
- **Tags**: `#tag`, `#nested/tag`.
- **Callouts**: `> [!note] ...` (Obsidian-compatible).
- **Math**: `$inline$` and `$$block$$` (KaTeX).
- **Mermaid** diagrams in fenced blocks.
- **Frontmatter**: YAML at the top.

### 4.3 Frontmatter

Every note can have YAML frontmatter:

```yaml
---
title: Note title
tags: [research, mcp]
created: 2025-10-14
updated: 2025-10-14
aliases: [MCP Security]
cssclasses: [wide]
publish: false
---
```

Unknown keys are preserved and queryable. Frontmatter is the primary way to
add structured metadata to notes.

### 4.4 Daily notes

Daily notes are auto-created on first open of the day. Path is configurable
(default: `daily/YYYY-MM-DD.md`). The daily note can include a template with
sections (journal, tasks, links, etc.).

### 4.5 Templates

Templates are markdown files in `vault/templates/`. They support variables:

- `{{date}}`, `{{time}}`, `{{datetime}}`
- `{{title}}` (prompted on insert)
- `{{selection}}` (current selection)
- `{{clipboard}}`
- `{{cursor}}` (cursor position after insertion)

The AI can create templates too.

### 4.6 Canvas

Fergents supports Obsidian-style **canvas** files (`.canvas`, JSON format).
The canvas is rendered as an interactive board with cards (notes, images,
text, links) and edges.

### 4.7 Attachments

Binaries (images, PDFs, audio, video) live in `vault/attachments/` (or
wherever the user puts them). They are:

- Indexed by path and MIME.
- Thumbnailed (images) or text-extracted (PDFs).
- Referenced from notes via standard markdown or wiki embeds.

---

## 5. The index

### 5.1 What's indexed

For each document:

- Path, MIME, size, hash, created/updated timestamps.
- Frontmatter (parsed).
- Tags.
- Links (outgoing).
- Backlinks (incoming).
- Chunks (with embeddings).
- Entities mentioned.
- Word count, reading time.

### 5.2 When it's updated

- **On open**: a full scan of the vault, with incremental updates (hash
  comparison).
- **On change**: a file watcher triggers re-indexing of changed files.
- **On demand**: a "reindex" command rebuilds from scratch.

### 5.3 Incremental

For a changed file:

1. Compute the new hash.
2. If different from the stored hash, re-chunk, re-embed, re-extract.
3. Update the document record, chunks, links, mentions.
4. Update the graph.

For a deleted file:

1. Soft-delete the document.
2. Remove or mark chunks, links, mentions.

### 5.4 Concurrency

Indexing runs in the background, in a worker pool. Priority: user-visible
operations first, then background re-indexing. A file currently being edited
is not re-indexed until the edit is saved.

### 5.5 Size limits

- Files larger than 50 MB are skipped (configurable).
- Files with more than 1M tokens are chunked but not embedded (unless the
  user opts in).

---

## 6. Configuration

### 6.1 `brain.toml`

```toml
[brain]
id = "01J8K2X9..."
name = "Personal"
slug = "personal"
template = "personal"
color = "#6B94D9"
icon = "🧠"
created_at = "2025-10-14T09:00:00Z"

[vault]
path = "vault"
default_note_folder = "notes"
daily_folder = "daily"
daily_format = "%Y-%m-%d"
attachments_folder = "attachments"
templates_folder = "templates"

[markdown]
wiki_links = true
embeds = true
block_refs = true
callouts = true
math = true
mermaid = true

[index]
embedding_model = "local:bge-m3"
chunk_size = 512
chunk_overlap = 64
auto_index = true
watch = true

[graph]
enabled = true
extract_entities = true
extract_relations = true
llm_extraction = false

[search]
default_mode = "hybrid"
default_top_k = 8
rerank = true

[sync]
enabled = false
scope = "full"          # full | notes-only | custom
peers = []

[git]
enabled = false
auto_commit = false
commit_message_template = "fergents: {{action}} {{path}}"
```

### 6.2 Per-brain vs global

Some settings are per-brain (vault structure, embedding model), others are
global (UI preferences, permission modes). The split is documented in each
section.

---

## 7. Templates

### 7.1 Brain templates

When creating a brain, the user picks a template:

| Template | Contents |
|---|---|
| **Blank** | Empty vault, minimal config |
| **Personal** | Daily notes, journal, people, projects, tasks |
| **Research** | Sources, notes, papers, annotations |
| **Coding** | Snippets, project notes, decisions, docs |
| **Team** | Shared brain for collaboration |

Templates are folders with pre-filled `brain.toml`, sample notes, and
templates. They live in `~/.fergents/templates/brains/`.

### 7.2 Custom templates

Any brain can be saved as a template:

1. Right-click the brain → "Save as template".
2. The vault and config are copied to `~/.fergents/templates/brains/<name>/`.
3. Personal data is excluded (configurable).

### 7.3 AI-generated templates

The AI can create a template from a description:

```
User: "Create a brain template for tracking my reading."
Agent: [creates the folder structure, sample notes, templates, config]
Agent: [registers it]
```

---

## 8. Multi-brain operations

### 8.1 Attaching

A project can attach multiple brains:

```toml
# project.toml
[[brains]]
id = "01J8K..."
role = "primary"

[[brains]]
id = "01J8L..."
role = "read-only"

[[brains]]
id = "01J8M..."
role = "scratch"
```

Roles:

- **primary**: the default brain for read/write. One per project.
- **read-only**: searchable, readable, but not writable by agents.
- **scratch**: temporary, disposable, cleared on project close.

### 8.2 Cross-brain search

A search can target:

- A single brain.
- A set of brains (union).
- All attached brains.

Results are merged and re-ranked. Each result shows its source brain.

### 8.3 Cross-brain links

A note in brain A can link to a note in brain B via a stable brain-scoped
ID:

```markdown
See [[brain:research/notes/mcp-security]] for context.
```

The link is resolved at render time. If the target brain isn't attached,
the link is shown as "unresolved".

### 8.4 Brain switcher

The UI provides a quick brain switcher (command palette + sidebar). The
active brain is shown in the status bar.

### 8.5 Duplication

Duplicating a brain:

1. Copies the folder.
2. Generates a new `brain.id`.
3. Rewrites internal links to point to the new ID (optional).
4. Optionally excludes attachments or large files.

---

## 9. Lifecycle

### 9.1 States

```
created → indexing → ready → (archived) → deleted
```

### 9.2 Creation

1. User picks a template.
2. Kernel creates the folder structure.
3. Kernel writes `brain.toml`.
4. Kernel registers the brain in the global DB.
5. Kernel triggers the first index.

### 9.3 Opening

1. Kernel reads `brain.toml`.
2. Kernel opens `index.sqlite`.
3. Kernel verifies the schema version (migrates if needed).
4. Kernel starts the file watcher.
5. Kernel triggers an incremental index.
6. Kernel emits `brain.opened`.

### 9.4 Closing

1. Kernel flushes pending index operations.
2. Kernel stops the file watcher.
3. Kernel checkpoints `index.sqlite`.
4. Kernel emits `brain.closed`.

### 9.5 Archiving

Archiving a brain:

- Removes it from the active list.
- Keeps the files on disk.
- Doesn't sync.
- Can be un-archived.

### 9.6 Deletion

Deleting a brain:

1. Confirmation (type the brain name).
2. Option to export first.
3. Files moved to trash (or deleted permanently, per config).
4. Registry updated.

---

## 10. Portability

### 10.1 A brain is a folder

You can move, copy, rename, or back up a brain with standard file tools.
Fergents picks up the change on next open.

### 10.2 A brain is portable

Because content is markdown and the index is regenerable, a brain works on
any OS, with or without Fergents.

### 10.3 Absolute vs relative paths

Internal references use **relative paths** (relative to the vault). This
makes a brain portable across machines and mount points.

### 10.4 Brain ID

The brain's stable identity is its `id` in `brain.toml`, not its path. This
lets you move a brain without breaking links, sync, or peers.

---

## 11. Versioning

### 11.1 Optional git

If enabled, the vault is a git repo. Fergents can:

- Auto-commit on change (with a template message).
- Show history.
- Diff versions.
- Revert.

### 11.2 Shadow git

Independently of user git, Fergents maintains a **shadow git** at
`.fergents/shadow.git` that commits every agent action. This is the
reversibility layer (see `SECURITY.md`).

### 11.3 Snapshots

Periodic `.fergbrain` archives (configurable schedule: hourly, daily,
weekly). Kept in `.fergents/snapshots/` with a retention policy.

### 11.4 External versioning

If the vault is inside a user's existing git repo, Fergents detects it and
uses it instead of creating its own.

---

## 12. Attachments

### 12.1 Storage

Attachments are stored in the vault (default `attachments/`). They can also
live elsewhere in the vault, or outside (via absolute paths, discouraged).

### 12.2 Indexing

- **Images**: thumbnails generated, metadata (dimensions, EXIF).
- **PDFs**: text extracted, pages rendered as images.
- **Audio**: transcribed (Whisper local or API), metadata.
- **Video**: key frames extracted, audio transcribed.
- **Other**: indexed by name and MIME.

### 12.3 Embedding

Attachment text (extracted) is chunked and embedded like notes. Images can
be embedded as captions (via a vision model) if enabled.

### 12.4 Deduplication

Files are hashed. Duplicate attachments (same hash) are linked, not
duplicated.

---

## 13. Linking across brains

### 13.1 Syntax

- **Same brain**: `[[notes/foo]]`
- **Cross-brain**: `[[brain:research/notes/foo]]`
- **By ID**: `[[id:01J8K...]]`

### 13.2 Resolution

At render time, Fergents resolves:

1. Same-brain links: direct.
2. Cross-brain links: if the target brain is attached, resolved; else shown
   as unresolved.

### 13.3 Backlinks across brains

Backlinks are computed per brain by default. Cross-brain backlinks are
computed on demand (in the UI) and cached.

---

## 14. Import and export

### 14.1 `.fergbrain`

A zip archive containing:

- `brain.toml`
- `vault/`
- Optionally `index.sqlite` and `graph.db` (usually excluded; regenerated
  on import).
- A `manifest.json` with checksums.

### 14.2 Import

1. User picks a `.fergbrain` file.
2. Kernel extracts to a chosen path.
3. Kernel validates `brain.toml`.
4. Kernel registers the brain.
5. Kernel triggers indexing.

### 14.3 Export

1. User picks a brain.
2. Kernel zips the folder (excluding regenerable files by default).
3. Kernel writes the archive.

### 14.4 Third-party importers

- **Obsidian**: directly compatible (same markdown flavor, wiki links,
  frontmatter). Just point Fergents at the vault.
- **Notion**: export as markdown + CSV, then import via a tool.
- **Logseq**: compatible with tweaks (block refs, journal format).
- **Roam**: via Roam's markdown export.

Importers are plugins (see `PLUGINS.md`).

---

## 15. AI operations on brains

The AI can:

- **Create** a brain from a template.
- **Attach/detach** brains to a project.
- **Duplicate** a brain.
- **Move** a brain.
- **Export/import** a brain.
- **Reindex** a brain.
- **Archive/delete** a brain (with confirmation).
- **Create templates** from a description.
- **Write notes**, create links, add tags.
- **Reorganize** the vault (move notes, rename, merge).
- **Clean up** broken links, orphan notes, duplicate tags.

All subject to permissions and reversible via shadow git + `.fergents`
backups.

---

## 16. Performance

### 16.1 Targets

| Metric | Target |
|---|---|
| Open a brain (10k notes) | < 2 s |
| Incremental reindex (1 file) | < 200 ms |
| Full reindex (10k notes) | < 10 min |
| Search latency (hybrid, top-10) | < 100 ms |
| Graph load (1k nodes) | < 1 s |

### 16.2 Techniques

- **Lazy loading**: only index what's needed.
- **Batching**: embed in batches of 64.
- **Caching**: cache embeddings, search results, graph queries.
- **WAL mode**: for concurrent reads.
- **Partial indexes**: for soft-deleted rows.
- **Background workers**: for indexing, extraction, thumbnail generation.

### 16.3 Large brains

For brains > 100k notes:

- Move embeddings to LanceDB (via `VectorStore` trait).
- Shard the vault by top-level folder.
- Use a dedicated CozoDB instance.
- Consider a dedicated machine for indexing.

---

## 17. Observability

Per brain:

- Document count, chunk count, entity count.
- Index size, last index time, index errors.
- Search latency, hit rate, cache hit rate.
- Sync status, last sync, conflicts.
- Growth over time.

All in the dashboard (see `DASHBOARD.md`).

---

## 18. Examples

### 18.1 Creating a brain from the CLI

```bash
fergents brain create --template research --name "MCP Research"
```

### 18.2 Attaching a brain to a project

```bash
fergents project attach-brain my-project --brain mcp-research --role read-only
```

### 18.3 Cross-brain search

```bash
fergents search "MCP security" --brains personal,mcp-research --mode hybrid
```

### 18.4 Exporting a brain

```bash
fergents brain export personal --output ~/backups/personal.fergbrain
```

---

## 19. Appendix: brain.toml

(see §6.1 for the full example)

---

*End of `BRAIN.md`.*