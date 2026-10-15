# Graph

> Entities, relations, backlinks, visualization, and queries over the
> knowledge graph.

**Audience:** developers, AI developers.
**Companion docs:** `BRAIN.md`, `SEARCH.md`.

---

## Table of contents

1. [What the graph is](#1-what-the-graph-is)
2. [Two graphs](#2-two-graphs)
3. [Entity extraction](#3-entity-extraction)
4. [Relation extraction](#4-relation-extraction)
5. [Storage](#5-storage)
6. [Queries](#6-queries)
7. [Backlinks](#7-backlinks)
8. [Visualization](#8-visualization)
9. [Filters and search](#9-filters-and-search)
10. [Graph algorithms](#10-graph-algorithms)
11. [Cross-brain graph](#11-cross-brain-graph)
12. [Performance](#12-performance)
13. [AI operations](#13-ai-operations)
14. [Observability](#14-observability)
15. [Examples](#15-examples)

---

## 1. What the graph is

The graph is the **network of entities and relations** extracted from the
brain. It complements search: where search finds *text*, the graph finds
*things* and *connections*.

Examples of what the graph enables:

- "Show me everyone mentioned in my notes and how they relate."
- "What projects reference the MCP protocol?"
- "Find all notes that cite this paper."
- "What's the shortest path between these two concepts?"

---

## 2. Two graphs

Fergents maintains two graphs:

### 2.1 The backlink graph

Derived directly from markdown links (`[[...]]`, `[text](url)`). Nodes are
**documents**, edges are **links**. It is:

- Exact (no inference).
- Cheap to compute.
- Stored in the brain DB (`links` table).

### 2.2 The entity graph

Derived from entity extraction. Nodes are **entities** (people, orgs,
concepts, files), edges are **relations** (mentions, cites, authored_by,
etc.). It is:

- Inferred (via NER + LLM).
- Richer but less exact.
- Stored in CozoDB (`graph.db`).

The two graphs are linked: each entity mention points to a document, and
each document can have many mentions.

---

## 3. Entity extraction

### 3.1 Sources

Entities are extracted from:

- Markdown content (notes).
- Frontmatter (explicit `entities:`, `people:`, `projects:`).
- Tags (`#person/john-doe`).
- Wiki links (`[[people/John Doe]]`).
- Links to external URLs (domains become entities).

### 3.2 Methods

**Regex-based** (always on, fast):

- URLs, emails, dates, code identifiers, file paths.
- Markdown links.

**NER-based** (local model, fast):

- Persons, organizations, locations, products.
- Multilingual.

**LLM-based** (opt-in, slow, costs tokens):

- Semantic entities (concepts, ideas).
- Relations between entities.
- Summaries of entity context.

### 3.3 Entity resolution

Duplicate entities (same person mentioned differently) are merged via:

- Name normalization (case, punctuation).
- Alias lists (from frontmatter).
- Embedding similarity (threshold-based).
- Manual merging by the user.

### 3.4 Entity types

Built-in types:

- `person`
- `organization`
- `place`
- `event`
- `concept`
- `file`
- `url`
- `date`
- `code_symbol`
- `custom` (user-defined)

Users can define custom types in `brain.toml`.

---

## 4. Relation extraction

### 4.1 Methods

**Structural** (from links):

- `links_to`: document A links to document B.
- `mentions`: document mentions entity E.
- `tagged_with`: document has tag T.
- `in_folder`: document is in folder F.

**Semantic** (opt-in via LLM):

- `cites`: document cites paper P.
- `authored_by`: paper P is authored by person X.
- `works_at`: person X works at organization Y.
- `part_of`: entity A is part of entity B.
- `related_to`: generic relation.

### 4.2 Confidence

Each relation has a confidence score (0-1):

- Structural: 1.0.
- NER-based: 0.7-0.9.
- LLM-based: 0.5-0.9 (model-dependent).

Low-confidence relations are hidden by default; users can reveal them.

### 4.3 Direction

Relations are directed. Some have inverses (`authored_by` ↔ `author_of`).

---

## 5. Storage

### 5.1 CozoDB schema

```datalog
:create entity {
    id: String,
    name: String,
    type: String,
    canonical_name: String?,
    metadata: Json?,
    first_seen_at: Int,
    last_seen_at: Int
}

:create relation {
    id: String,
    src: String,
    dst: String,
    kind: String,
    weight: Float,
    confidence: Float,
    metadata: Json?,
    source_doc: String?,
    created_at: Int
}

:create mention {
    entity_id: String,
    document_id: String,
    chunk_id: String?,
    start_offset: Int?,
    end_offset: Int?,
    confidence: Float
}
```

### 5.2 Relational mirror

For fast lookups and cross-references, the same data is mirrored in SQLite
(`entities`, `relations`, `entity_mentions`). See `DATA_MODEL.sql`.

### 5.3 When it's updated

- On document change: re-extract entities and relations for that document.
- On delete: remove mentions; keep entities if referenced elsewhere.
- On merge: rewrite relations to point to the canonical entity.

---

## 6. Queries

### 6.1 Datalog (CozoDB)

```datalog
# All entities of type person mentioned in the last 30 days
?[name, count] :=
    *entity{id, name, type: "person"},
    *mention{entity_id: id, document_id},
    *document{document_id, updated_at},
    updated_at > (now() - 30 * 86400),
    count = count(document_id)
```

### 6.2 SQL (relational mirror)

For simple queries, the SQLite mirror is enough:

```sql
SELECT e.name, COUNT(*) as mentions
FROM entities e
JOIN entity_mentions m ON m.entity_id = e.id
WHERE e.type = 'person'
GROUP BY e.id
ORDER BY mentions DESC
LIMIT 20;
```

### 6.3 API

```json
{
  "command": "graph.query",
  "params": {
    "query": "...",
    "language": "datalog",
    "brain": "personal"
  }
}
```

### 6.4 Natural language

The AI can translate natural language to graph queries:

```
User: "Who are the people most mentioned in my notes?"
Agent: [translates to Datalog, runs, returns results]
```

---

## 7. Backlinks

### 7.1 Computation

Backlinks are computed from the `links` table:

```sql
SELECT src_doc_id, anchor, display_text
FROM links
WHERE dst_doc_id = ? OR dst_path = ?
```

### 7.2 Display

In a note, the backlinks panel shows:

- All notes linking to this note.
- The context of each link (the paragraph).
- Unresolved links (targets that don't exist yet).

### 7.3 Unlinked mentions

The graph also detects **unlinked mentions**: places where the note's title
appears as plain text (not a link). The UI suggests linking them.

### 7.4 Orphan and dead-end notes

- **Orphans**: no incoming links.
- **Dead ends**: no outgoing links.

The UI surfaces these for cleanup.

---

## 8. Visualization

### 8.1 Engine

A custom renderer built on:

- **WebGL** for large graphs (via PixiJS or a custom shader).
- **Force-directed layout** (d3-force or a Rust implementation via WASM).

### 8.2 Features

- Pan, zoom, drag nodes.
- Click a node to focus (highlight neighbors).
- Double-click to open the note.
- Multi-select for batch operations.
- Filter by entity type, tag, date, confidence.
- Search within the graph.
- Layout presets (force, radial, hierarchical, cluster).

### 8.3 Performance

- **Level of detail**: distant nodes are simplified.
- **Sampling**: for very large graphs, sample or cluster.
- **Virtualization**: only render what's visible.
- **WebGL**: GPU-accelerated rendering.

Targets:

- 10k nodes: 60 fps.
- 100k nodes: 30 fps (with clustering).

### 8.4 Views

- **Local graph**: neighbors of the current note (1-2 hops).
- **Global graph**: the entire brain.
- **Entity graph**: entities and relations only.
- **Backlink graph**: documents and links only.
- **Custom view**: filtered by any combination.

---

## 9. Filters and search

### 9.1 Filters

- Entity type.
- Tag.
- Date range.
- Confidence threshold.
- Degree (min/max connections).
- Path (folder).
- Brain (in multi-brain view).

### 9.2 Search

Search within the graph:

- By entity name (fuzzy).
- By relation kind.
- By document.
- By query (natural language, translated to graph query).

### 9.3 Highlighting

Matched nodes and edges are highlighted. Non-matching nodes are dimmed.

---

## 10. Graph algorithms

### 10.1 Built-in

- **Shortest path** between two nodes.
- **Neighbors** at depth N.
- **Centrality** (degree, betweenness, PageRank).
- **Communities** (Louvain, label propagation).
- **Similarity** (Jaccard, cosine over embeddings).
- **Cycles** detection.
- **Subgraph** extraction.

### 10.2 Use cases

- "Find the most central concepts in my research."
- "What communities exist in my notes?"
- "What's the path between these two ideas?"
- "What are the most similar notes to this one?"

### 10.3 API

```json
{
  "command": "graph.algo",
  "params": {
    "algo": "pagerank",
    "brain": "personal",
    "top_k": 20
  }
}
```

---

## 11. Cross-brain graph

### 11.1 Concept

When multiple brains are attached, a **cross-brain graph** can be built:

- Nodes from all brains.
- Edges within and across brains.
- Cross-brain edges come from:
  - Explicit cross-brain links (`[[brain:other/...]]`).
  - Shared entities (same name, same type).
  - Shared tags.

### 11.2 Scope

The cross-brain graph is computed on demand and cached. It's not persisted
(as it depends on which brains are attached).

### 11.3 Visualization

A special view shows brains as clusters, with cross-brain edges highlighted.

---

## 12. Performance

### 12.1 Targets

| Operation | Target |
|---|---|
| Local graph (2 hops) | < 100 ms |
| Global graph (10k nodes) | < 1 s to load, 60 fps render |
| Shortest path | < 500 ms |
| PageRank (10k nodes) | < 2 s |
| Communities (10k nodes) | < 5 s |

### 12.2 Techniques

- **Incremental updates**: only changed parts are recomputed.
- **Caching**: query results cached.
- **Background computation**: heavy algorithms run in the background.
- **Sampling**: for very large graphs.

---

## 13. AI operations

The AI can:

- **Query** the graph in Datalog, SQL, or natural language.
- **Explain** a subgraph.
- **Suggest** new entities or relations.
- **Merge** duplicate entities.
- **Detect** anomalies (orphans, cycles, inconsistencies).
- **Visualize** a subgraph.
- **Export** a subgraph.

---

## 14. Observability

Per brain:

- Entity count, relation count.
- Extraction latency.
- Merge operations.
- Query latency.
- Graph size.

---

## 15. Examples

### 15.1 Find central concepts

```
User: "What are the most central concepts in my research brain?"
Agent: [runs PageRank on entity graph, returns top 20]
```

### 15.2 Path between ideas

```
User: "How are MCP and RAG connected in my notes?"
Agent: [runs shortest path, explains the chain]
```

### 15.3 Cleanup

```
User: "Find duplicate people in my brain."
Agent: [runs similarity detection, suggests merges]
```

### 15.4 Visualization

```
User: "Show me the graph of my project notes."
Agent: [opens the graph view, filtered to projects/]
```

---

*End of `GRAPH.md`.*
