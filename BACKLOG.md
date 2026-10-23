# Phase 2 — Agent Runtime & Chat UI Backlog

## 0. Modular Architecture (Feature Toggles & Capabilities)
- Every feature must be toggleable (enable/disable).
- Disabling must occur at the Kernel level (not just the UI): if a feature is turned off, the backend must stop allocating resources for it.
- Features must be grouped into "Capabilities" (e.g., `capability.search`, `capability.graph`, `capability.voice`, `capability.charts`).
- A "Features" page in the Settings must provide switches (On/Off) for all capabilities. For instance, if "Charts" is disabled, the Context Viewer will not display them, and the AI ​​will not attempt to generate them.
- Plugins and custom functionalities must be able to register themselves as optional "Capabilities."

## 1. Chat Interface (Minimalist & Powerful)
- Claude-style layout (centered, minimalist chat).
- File, image, and PDF uploads (drag & drop).
- Hover actions: Copy, Regenerate, Fork (with visual branching).
- Sidebar containing chat history and a prompt library.
- Composer with an "Improve Input" feature (magic wand icon).
- Multiple-choice questions (A/B/C buttons + an "Other" field).
- Rich Outputs: Support for AI-generated charts (via specific code blocks) rendered securely (e.g., using Recharts).
- "Explain This": Select text or an error to receive a quick explanation in a popup.
- Quick Capture: A global shortcut for sending prompts on the fly.
- Chat Search: Full-text search within chat history.

## 2. Agent Runtime & Memory
- Custom agents (system prompts, avatars, personalities). - **Agent Builder UI**: A visual interface for creating and configuring new agents, moving beyond manual editing of `.md` files.
- Permission Management (Permission Bar).
- `brain.md` Memory (user preferences, common errors).
- `session.md` Summary (context compaction).
- Self-Improving Prompt: The agent learns and updates its own `brain.md`.
- Context Viewer: A UI with checkboxes to toggle injected sections (Data, Capabilities, RAG) and a token counter.
- Spec/Plan/Tasks: A workflow for complex tasks (generating files, requesting approval, and executing).
- **Auto-Continue**: If output is truncated due to token limits, the system automatically sends a "continue" command, up to a user-configurable limit.

## 3. Multi-Agent & Orchestration
- **Agent Sync**: Two or more agents (e.g., a Coder and a Reviewer) can collaborate in real-time on the same session or file, with visibility into each other's changes.
- **Time Travel (State Rollback)**: A timeline bar within the chat that allows users to return to a previous message, restore the exact state of the AI ​​and its "Brain" at that point in time, and re-explore a decision path (combined with a "Fork" feature).
- **Asynchronous Sub-Agents**: A manager agent can spawn sub-agents to work in the background, continue with other tasks, and collect the results once they are ready.

## 4. Providers & Integrations
- **Multi-Model UI**: A model selector within the chat (supporting both cloud and local models). Users can run two parallel chat sessions using different models to compare their responses. - **Provider Manager**: A UI for adding/removing API keys (OpenAI, Anthropic, Ollama) and testing connections.
- **MCP Manager**: A dedicated UI for adding, removing, testing, and monitoring the health status of MCP servers.
- Secret Vault: Secure management of API keys (via OS Keychain).

## 5. Ecosystem & Extensions
- Ombra: A global overlay accessible via hotkey.
- Voice Mode: Whisper integration (for voice input/output).
- Agent Web: A browser extension that allows the agent to read web pages.
- Plugin Factory: The AI ​​writes, compiles, and tests plugins in a sandbox (using predefined UI templates to ensure adherence to the style guide).
- Workflow Debugger: A visual, step-by-step debugger.