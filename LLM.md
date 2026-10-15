# LLM

> The model layer: providers, routing, streaming, tool calling, caching,
> cost tracking, local models, and multimodal support.

**Audience:** developers, AI developers.
**Companion docs:** `AGENTS.md`, `STACK.md`, `MCP.md`.

---

## Table of contents

1. [Principles](#1-principles)
2. [The Provider trait](#2-the-provider-trait)
3. [Supported providers](#3-supported-providers)
4. [The router](#4-the-router)
5. [Streaming](#5-streaming)
6. [Tool calling](#6-tool-calling)
7. [Structured output](#7-structured-output)
8. [Multimodal](#8-multimodal)
9. [Embeddings](#9-embeddings)
10. [Reranking](#10-reranking)
11. [Caching](#11-caching)
12. [Cost tracking](#12-cost-tracking)
13. [Budgets and limits](#13-budgets-and-limits)
14. [Local models](#14-local-models)
15. [Errors and fallbacks](#15-errors-and-fallbacks)
16. [Secrets](#16-secrets)
17. [Observability](#17-observability)
18. [Configuration](#18-configuration)
19. [Appendix: message format](#19-appendix-message-format)

---

## 1. Principles

- **Model-agnostic**: no provider is privileged.
- **Normalized**: every provider is mapped to a common internal schema.
- **Streaming by default**: tokens arrive as they are generated.
- **Tool-calling first-class**: normalized across providers.
- **Cost-aware**: every call is tracked, budgets are enforced.
- **Local-first**: local models are always an option.
- **User-controlled**: routing, budgets, and providers are user-configured.

---

## 2. The Provider trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> Capabilities;

    async fn chat(
        &self,
        request: ChatRequest,
    ) -> Result<ChatResponse, ProviderError>;

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<ChatStream, ProviderError>;

    async fn embed(
        &self,
        request: EmbedRequest,
    ) -> Result<EmbedResponse, ProviderError>;

    async fn rerank(
        &self,
        request: RerankRequest,
    ) -> Result<RerankResponse, ProviderError>;

    async fn models(&self) -> Result<Vec<ModelInfo>, ProviderError>;

    async fn health(&self) -> Result<(), ProviderError>;
}
```

Every provider implements this. The kernel calls providers only through this
trait.

### 2.1 Capabilities

```rust
pub struct Capabilities {
    pub streaming: bool,
    pub tool_calling: bool,
    pub parallel_tool_calls: bool,
    pub structured_output: bool,
    pub vision: bool,
    pub audio: bool,
    pub reasoning: bool,
    pub prompt_cache: bool,
    pub max_context: usize,
}
```

The router uses capabilities to decide which provider/model can handle a
request.

---

## 3. Supported providers

| Provider | Chat | Streaming | Tools | Vision | Audio | Reasoning | Cache |
|---|---|---|---|---|---|---|---|
| OpenAI | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Anthropic | ✓ | ✓ | ✓ | ✓ | — | ✓ | ✓ |
| Google Gemini | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| Mistral | ✓ | ✓ | ✓ | ✓ | — | — | — |
| Cohere | ✓ | ✓ | ✓ | — | — | — | — |
| Kimi / Moonshot | ✓ | ✓ | ✓ | ✓ | — | — | — |
| Qwen (DashScope) | ✓ | ✓ | ✓ | ✓ | — | ✓ | — |
| TogetherAI | ✓ | ✓ | ✓ | ✓ | — | — | — |
| Groq | ✓ | ✓ | ✓ | — | — | — | — |
| OpenRouter | ✓ | ✓ | ✓ | ✓ | — | ✓ | — |
| Custom (OpenAI-compatible) | ✓ | ✓ | ✓ | ✓ | — | — | — |
| Ollama | ✓ | ✓ | ✓ | ✓ | — | — | — |
| llama.cpp | ✓ | ✓ | ✓ | ✓ | — | — | — |
| MLX (Apple) | ✓ | ✓ | ✓ | ✓ | — | — | — |

New providers can be added via plugins.

---

## 4. The router

### 4.1 Concept

The router maps a **task** to a **model**. Tasks are semantic labels:

- `chat.simple`, `chat.reasoning`, `chat.long`
- `code.edit`, `code.review`, `code.explain`
- `summarize`, `translate`, `classify`
- `embed`, `rerank`
- `vision`, `audio.transcribe`

### 4.2 Configuration

```toml
[[route]]
task = "chat.simple"
model = "ollama:llama3.1:8b"
fallback = ["openai:gpt-4o-mini"]

[[route]]
task = "chat.reasoning"
model = "anthropic:claude-sonnet-4.5"
fallback = ["openai:o1-mini"]

[[route]]
task = "code.edit"
model = "anthropic:claude-sonnet-4.5"
fallback = ["openai:gpt-4o", "ollama:qwen2.5-coder:14b"]

[[route]]
task = "embed"
model = "local:bge-m3"

[[route]]
task = "rerank"
model = "local:bge-reranker-v2-m3"
```

### 4.3 Resolution

When a request arrives:

1. Determine the task (from the caller: agent, workflow, plugin).
2. Look up the route.
3. Check capabilities (does the model support tools, vision, etc.?).
4. Check budget (would this exceed the limit?).
5. Check rate limits.
6. If all pass, call the provider.
7. On failure, try the next fallback.

### 4.4 Overrides

- **Per-agent**: an agent can specify `model:` in its definition.
- **Per-request**: a caller can specify `model` explicitly.
- **Per-session**: the user can pin a model in the chat UI.

Overrides take precedence over routes but are still subject to budgets and
capabilities.

---

## 5. Streaming

### 5.1 Protocol

Streaming uses SSE (Server-Sent Events) internally, regardless of the
provider's native streaming method. The kernel normalizes:

```json
{ "type": "start", "model": "claude-sonnet-4.5" }
{ "type": "token", "text": "Hello" }
{ "type": "token", "text": " world" }
{ "type": "tool_call", "id": "call_1", "name": "brain.search", "args": { ... } }
{ "type": "usage", "input_tokens": 123, "output_tokens": 45 }
{ "type": "stop", "reason": "end_turn" }
```

### 5.2 Backpressure

Tokens are buffered in a bounded channel. Slow consumers (UI, CLI) don't
block the provider. If the buffer fills, oldest tokens are dropped (with a
warning).

### 5.3 Cancellation

A streaming call can be cancelled at any time. The provider's HTTP
connection is closed, and the stream ends.

---

## 6. Tool calling

### 6.1 Normalization

Every provider's tool-calling format is mapped to a common schema:

```json
{
  "id": "call_abc",
  "name": "brain.search",
  "arguments": { "query": "MCP security", "top_k": 5 }
}
```

### 6.2 Parallel calls

If the provider supports parallel tool calls, the model can emit multiple
calls in one response. The kernel executes them concurrently (subject to
per-tool limits) and returns all results in the next message.

### 6.3 Tool result format

```json
{
  "role": "tool",
  "tool_call_id": "call_abc",
  "content": "...",
  "status": "ok"
}
```

Errors use the same shape with `status: "error"` and an error object.

### 6.4 Tool selection

For agents with many tools, the kernel can **retrieve** the most relevant
tools for the current context and pass only those to the model. See
`AGENTS.md §7.5`.

---

## 7. Structured output

### 7.1 JSON mode

Many providers support JSON mode or structured output. The kernel uses it
when the caller provides a schema:

```json
{
  "response_format": {
    "type": "json_schema",
    "schema": { ... }
  }
}
```

### 7.2 Fallback

For providers without native structured output, the kernel:

1. Instructs the model to return JSON.
2. Parses the response.
3. Repairs common issues (trailing commas, missing quotes).
4. Validates against the schema.
5. Retries once if invalid.

### 7.3 Schema validation

Every structured output is validated against the caller's schema. Invalid
outputs are rejected with a clear error.

---

## 8. Multimodal

### 8.1 Images

Images are passed as content blocks:

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "What's in this image?" },
    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "..." } }
  ]
}
```

Images are resized and re-encoded to fit the model's limits. Large images
are split or downsampled.

### 8.2 Audio

Audio is transcribed first (Whisper local or API), then passed as text. For
models with native audio (Gemini), the audio is passed directly.

### 8.3 Video

Video is processed by:

1. Extracting key frames (scene detection).
2. Transcribing audio.
3. Passing frames + transcript to a vision model.

Configurable frame rate, resolution, and max duration.

### 8.4 PDFs

PDFs are processed by:

1. Extracting text (with layout).
2. Rendering pages as images (optional).
3. Passing text + images to a vision model.

The `brain.import` tool handles this automatically.

---

## 9. Embeddings

### 9.1 Local (default)

- **Model**: BGE-M3 (multilingual, 1024 dims).
- **Runtime**: fastembed-rs (ONNX).
- **Speed**: ~50 docs/s on modern CPU.
- **GPU**: optional (CUDA, Metal, Vulkan).

### 9.2 Remote (optional)

- OpenAI `text-embedding-3-large`.
- Cohere `embed-v3`.
- Voyage `voyage-3`.
- Custom (OpenAI-compatible).

### 9.3 Batching

Embeddings are batched (default 64 texts per request) and cached by content
hash. Identical content is never re-embedded.

### 9.4 Dimensionality

The vector dimension is stored per brain. Changing the model requires
re-indexing (the UI warns and offers a migration).

---

## 10. Reranking

- **Model**: BGE-reranker-v2-m3 (local).
- **Input**: a query + a list of candidate texts.
- **Output**: scores, sorted.
- **Usage**: after hybrid search, rerank the top-K (default 20) to get the
  top-N (default 8).

Remote alternatives: Cohere Rerank, Voyage Rerank.

---

## 11. Caching

### 11.1 Prompt cache

Provider-side prompt caching is enabled where supported (Anthropic, OpenAI).
The kernel marks cacheable prefixes in requests.

### 11.2 Local cache

Every request/response is hashed:

```
cache_key = sha256(provider | model | messages | tools | params)
```

Stored in `llm_cache`. On hit, the cached response is returned. TTL is
configurable (default: 24h). Deterministic requests (temperature = 0) use
caching aggressively.

### 11.3 Semantic cache

For repeated FAQ-like queries, an optional semantic cache compares the query
embedding to cached queries. If the similarity exceeds a threshold, the
cached answer is returned.

### 11.4 Cache invalidation

- Time-based (TTL).
- Manual (user clears the cache).
- Content-based (if the brain changes, brain-dependent caches are
  invalidated).

---

## 12. Cost tracking

Every request records:

- Provider, model, task.
- Input, output, cached tokens.
- Cost (USD).
- Latency.
- Status.
- Agent, session, run.

Aggregated per:

- Day / week / month.
- Provider / model.
- Agent / session / project.
- Task.

Surfaced in the dashboard with alerts at user-defined thresholds.

---

## 13. Budgets and limits

### 13.1 Per-run

Enforced by the agent runtime (see `AGENTS.md §13`).

### 13.2 Per-provider, per-model, global

Configured in settings:

```toml
[budgets]
global_monthly_usd = 100.0
global_daily_usd = 10.0

[[budgets.provider]]
provider = "openai"
monthly_usd = 50.0

[[budgets.model]]
model = "anthropic:claude-sonnet-4.5"
monthly_usd = 30.0
```

The kernel checks before each call. If the call would exceed a budget, it's
rejected with a clear error, and the caller adapts (fallback, cheaper model,
or stop).

### 13.3 Soft thresholds

At 80% (configurable), a warning is emitted. At 100%, the call is blocked
(or allowed with a warning, depending on `hard_stop`).

---

## 14. Local models

### 14.1 Ollama

- Default local runtime.
- Models managed via `ollama pull`.
- The kernel discovers installed models and their capabilities.

### 14.2 llama.cpp

- Embedded option (via FFI).
- No external process.
- Useful for users who want zero dependencies.

### 14.3 MLX

- Apple silicon only.
- Fastest local inference on Mac.

### 14.4 Model management

The UI shows installed models, their sizes, and their capabilities. Users
can pull, remove, and switch models. The AI can too (subject to permission).

---

## 15. Errors and fallbacks

| Error | Detection | Handling |
|---|---|---|
| Rate limit (429) | HTTP 429 | Backoff, then fallback |
| Auth error (401/403) | HTTP | Fail fast, notify user |
| Invalid request (400) | HTTP | Fail fast, log details |
| Server error (5xx) | HTTP | Retry, then fallback |
| Timeout | No response in N s | Retry, then fallback |
| Context overflow | Token count | Compact, retry |
| Content filter | Provider signal | Return to caller with explanation |
| Tool call malformed | Schema validation | Tell model, retry |
| Streaming interrupted | Stream close | Retry from scratch, or resume if supported |

Fallback chain: configured per route. If all fallbacks fail, the request
fails with a clear error.

---

## 16. Secrets

API keys are stored in the OS keychain, never in config files. The kernel
resolves `$KEYCHAIN:name` references at call time.

Plugins and agents never see raw keys. They request an operation; the kernel
performs it with the key.

---

## 17. Observability

- **Traces**: one span per request, with child spans for streaming, tool
  calls, retries.
- **Metrics**: requests, tokens, cost, latency, errors, cache hit rate, per
  provider/model/task.
- **Logs**: structured, per request (hashed inputs by default; full inputs
  optional).

All in the dashboard.

---

## 18. Configuration

### 18.1 Providers

```toml
[[provider]]
id = "openai"
api_key = "$KEYCHAIN:openai"
base_url = "https://api.openai.com/v1"

[[provider]]
id = "anthropic"
api_key = "$KEYCHAIN:anthropic"

[[provider]]
id = "ollama"
base_url = "http://localhost:11434"

[[provider]]
id = "custom"
base_url = "https://my-llm.example.com/v1"
api_key = "$KEYCHAIN:custom"
```

### 18.2 Routes

(see §4.2)

### 18.3 Defaults

```toml
[defaults]
temperature = 0.7
max_tokens = 4096
stream = true
timeout_s = 120
retry = { max = 2, backoff_ms = 500 }
```

---

## 19. Appendix: message format

The internal message format is provider-agnostic:

```json
{
  "role": "user | assistant | system | tool",
  "content": "..." | [ ContentBlock ],
  "name": "optional",
  "tool_calls": [ ToolCall ],
  "tool_call_id": "for tool role"
}
```

Content blocks:

```json
{ "type": "text", "text": "..." }
{ "type": "image", "source": { ... } }
{ "type": "audio", "source": { ... } }
{ "type": "video", "source": { ... } }
{ "type": "file", "source": { ... } }
```

The provider adapter converts this to the provider's native format.

---

*End of `LLM.md`.*