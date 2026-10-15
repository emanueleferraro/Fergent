# Skills

> Skills are reusable capabilities: prompts, tools, or code. They extend what
> agents can do without modifying the kernel.

**Audience:** developers, AI developers.
**Companion docs:** `AGENTS.md`, `PLUGINS.md`, `MCP.md`.

---

## Table of contents

1. [What a skill is](#1-what-a-skill-is)
2. [Skill kinds](#2-skill-kinds)
3. [The SKILL.md format](#3-the-skillmd-format)
4. [Skill runtime](#4-skill-runtime)
5. [Permissions](#5-permissions)
6. [Discovery and installation](#6-discovery-and-installation)
7. [Versioning](#7-versioning)
8. [AI-created skills](#8-ai-created-skills)
9. [Marketplace](#9-marketplace)
10. [Examples](#10-examples)
11. [Appendix: frontmatter schema](#11-appendix-frontmatter-schema)

---

## 1. What a skill is

A skill is a **reusable, named capability** that agents (and workflows, and
plugins) can invoke. Unlike a plugin, a skill is:

- **Simpler**: no UI, no kernel extension, just a capability.
- **Composable**: skills can call other skills.
- **Portable**: a skill is a folder with a `SKILL.md` and optional assets.

Skills are the "functions" of the agent world. They encode best practices,
domain knowledge, and common operations.

---

## 2. Skill kinds

| Kind | What it is | Runtime |
|---|---|---|
| **prompt** | A parameterized prompt template | LLM call |
| **tool** | A wrapper around an existing tool or MCP tool | Kernel |
| **code** | A WASM module | wasmtime |
| **hybrid** | Combines prompt + code or tools | Mixed |

---

## 3. The SKILL.md format

### 3.1 Structure

```
skills/summarize/
├── SKILL.md          (required)
├── prompt.md         (optional, for prompt skills)
├── skill.wasm        (optional, for code skills)
├── schema.json       (optional, input/output schema)
├── examples/         (optional, few-shot examples)
└── README.md         (optional)
```

### 3.2 Full example (prompt skill)

```markdown
---
id: com.fergents.summarize
name: Summarize
description: Summarize a piece of text into a structured summary.
version: 1.0.0
author: fergents
kind: prompt
tags: [text, summary]

model: ollama:qwen2.5:7b
model_fallback: [openai:gpt-4o-mini]
temperature: 0.3
max_tokens: 1024

input:
  type: object
  properties:
    text: { type: string }
    style: { enum: [bullet, paragraph, tldr], default: bullet }
    max_words: { type: integer, default: 150 }
  required: [text]

output:
  type: object
  properties:
    summary: { type: string }
    key_points: { type: array, items: { type: string } }
  required: [summary]

permissions: []
---

# Summarize

You are a summarization expert. Given the input text, produce:

1. A summary of at most `max_words` words, in the requested style.
2. A list of key points (3-7 bullets).

## Rules

- Preserve the author's intent and tone.
- Do not introduce facts not present in the text.
- If the text is too short to summarize, return it unchanged as the summary
  and an empty list of key points.

## Style

- `bullet`: a bulleted list.
- `paragraph`: a single paragraph.
- `tldr`: one sentence.
```

### 3.3 Full example (code skill)

```markdown
---
id: com.example.wordcount
name: Word Count
description: Count words, lines, and characters in a text.
version: 1.0.0
kind: code
entry: skill.wasm
language: rust

input:
  type: object
  properties:
    text: { type: string }
  required: [text]

output:
  type: object
  properties:
    words: { type: integer }
    lines: { type: integer }
    chars: { type: integer }

permissions:
  - storage.local
---

# Word Count

(no prompt needed; the WASM module implements the logic)
```

```rust
// skill.wasm (source: src/lib.rs)
use fergents_skill_sdk::*;

#[fergents_skill]
fn run(input: Input) -> Result<Output> {
    let text = input.text;
    Ok(Output {
        words: text.split_whitespace().count() as u32,
        lines: text.lines().count() as u32,
        chars: text.chars().count() as u32,
    })
}
```

### 3.4 Full example (tool skill)

```markdown
---
id: com.example.github-issue
name: Create GitHub Issue
description: Create an issue on a GitHub repository.
version: 1.0.0
kind: tool
wraps: mcp:github:create_issue

input:
  type: object
  properties:
    repo: { type: string }
    title: { type: string }
    body: { type: string }
    labels: { type: array, items: { type: string } }
  required: [repo, title]

permissions:
  - mcp.connect
---

# Create GitHub Issue

Wraps the GitHub MCP server's `create_issue` tool with sensible defaults:

- Always adds the `triage` label unless specified.
- Prepends the current project name to the title.
```

### 3.5 Full example (hybrid skill)

```markdown
---
id: com.example.code-review
name: Code Review
description: Review a code diff and produce structured feedback.
version: 1.0.0
kind: hybrid
model: anthropic:claude-sonnet-4.5
entry: skill.wasm

input:
  type: object
  properties:
    diff: { type: string }
  required: [diff]

output:
  type: object
  properties:
    issues: { type: array, items: { type: object } }
    summary: { type: string }
---

# Code Review

1. The WASM module parses the diff into structured hunks.
2. Each hunk is sent to the LLM with the prompt below.
3. Results are merged and returned.

## Prompt

You are a senior code reviewer. For each hunk, identify:
- Bugs
- Security issues
- Performance problems
- Style violations

Return a JSON array of issues with severity, line, and suggestion.
```

---

## 4. Skill runtime

### 4.1 Invocation

Skills are invoked via the `skill.invoke` tool:

```json
{
  "tool": "skill.invoke",
  "arguments": {
    "skill_id": "com.fergents.summarize",
    "input": { "text": "...", "style": "bullet" }
  }
}
```

### 4.2 Execution

1. **Validate** input against the skill's schema.
2. **Check permissions**.
3. **Execute** based on kind:
   - **prompt**: render the template, call the LLM, parse the output.
   - **tool**: call the wrapped tool.
   - **code**: run the WASM module with the input.
   - **hybrid**: run WASM first, then LLM (or vice versa) as declared.
4. **Validate** output against the schema.
5. **Return** the result.

### 4.3 Chaining

A skill can invoke other skills. The runtime tracks the call stack and
enforces a maximum depth (default 5) to prevent infinite recursion.

### 4.4 Caching

Prompt skills support caching: identical inputs (hash) return cached
outputs. Configurable TTL.

---

## 5. Permissions

Skills declare permissions in their frontmatter. The user approves at
install. Possible permissions:

- `storage.local` — use skill-local storage.
- `fs.read` — read files (with scope).
- `fs.write` — write files (with scope).
- `net.http` — make HTTP requests (with domains).
- `llm.call` — call an LLM (with models).
- `mcp.connect` — call MCP servers.
- `skill.invoke` — call other skills.

---

## 6. Discovery and installation

### 6.1 Sources

- **Local**: a folder in `skills/`.
- **Git**: a URL to a repo.
- **Marketplace**: post-v1.
- **AI-created**: the AI writes the SKILL.md and any assets.

### 6.2 Installation flow

1. User (or AI) provides a source.
2. Kernel fetches and validates the SKILL.md.
3. Permissions are shown to the user.
4. User approves.
5. Skill is installed to `skills/<id>/`.
6. Registry updated.

### 6.3 Uninstallation

- Skill folder removed.
- Registry updated.
- Running invocations are cancelled.
- Skill-local storage is deleted (with confirmation).

---

## 7. Versioning

Skills follow semver. The registry tracks the installed version. Updates are
manual (or automatic if configured).

The kernel enforces compatibility: if a skill requires a newer API version
than the kernel provides, it's rejected.

---

## 8. AI-created skills

The AI can create skills:

```
User: "Create a skill that extracts action items from meeting notes."
Agent: [writes SKILL.md with kind: prompt, a good prompt, an input/output schema]
Agent: [calls skill.create with the content]
Kernel: [validates, installs, registers]
Agent: "Done. You can now use `com.user.extract-actions`."
```

The AI can also:

- **Modify** an existing skill (with a `.fergents` backup).
- **Duplicate** a skill.
- **Delete** a skill (with confirmation).
- **Test** a skill with sample inputs.

---

## 9. Marketplace

Post-v1. A curated index of skills, fetched from a git repo, verified by
signature. Skills can be free or paid. Moderation pipeline for community
submissions.

The AI can search and install from the marketplace (subject to permission).

---

## 10. Examples

### 10.1 A citation skill

```markdown
---
id: com.fergents.cite
name: Cite
description: Format a source as a citation in a given style.
version: 1.0.0
kind: prompt
model: ollama:qwen2.5:7b
input:
  type: object
  properties:
    source: { type: object }
    style: { enum: [apa, mla, chicago], default: apa }
  required: [source]
output:
  type: object
  properties:
    citation: { type: string }
  required: [citation]
---

# Cite

Given a source object (title, author, year, url), produce a citation in the
requested style. Follow the style guide precisely. If a field is missing,
omit it gracefully.
```

### 10.2 A skill that calls an MCP tool

```markdown
---
id: com.example.search-arxiv
name: Search arXiv
description: Search arXiv for papers matching a query.
version: 1.0.0
kind: tool
wraps: mcp:arxiv:search_papers
input:
  type: object
  properties:
    query: { type: string }
    limit: { type: integer, default: 5 }
  required: [query]
permissions:
  - mcp.connect
---

# Search arXiv

Wraps the arXiv MCP tool with a default limit and a filter for recent papers.
```

---

## 11. Appendix: frontmatter schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Fergents Skill",
  "type": "object",
  "required": ["id", "name", "version", "kind"],
  "properties": {
    "id": { "type": "string", "pattern": "^[a-z][a-z0-9.]*[a-z0-9]$" },
    "name": { "type": "string" },
    "description": { "type": "string" },
    "version": { "type": "string" },
    "author": { "type": "string" },
    "tags": { "type": "array", "items": { "type": "string" } },
    "kind": { "enum": ["prompt", "tool", "code", "hybrid"] },

    "model": { "type": "string" },
    "model_fallback": { "type": "array", "items": { "type": "string" } },
    "temperature": { "type": "number" },
    "max_tokens": { "type": "integer" },

    "entry": { "type": "string" },
    "language": { "type": "string" },
    "wraps": { "type": "string" },

    "input": { "type": "object" },
    "output": { "type": "object" },

    "permissions": { "type": "array", "items": { "type": "string" } },
    "cache": {
      "type": "object",
      "properties": {
        "enabled": { "type": "boolean" },
        "ttl_s": { "type": "integer" }
      }
    }
  }
}
```

---

*End of `SKILLS.md`.*