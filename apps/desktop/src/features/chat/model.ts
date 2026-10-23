/**
 * Pure helpers for the chat panel (Task 2.13): message parsing, tool-call
 * cards, thinking blocks, multiple-choice prompts, and formatting.
 */
import type { ChatMessage, ChatSession } from '@fergents/protocol';

/** A tool call rendered as a card. */
export interface ToolCallView {
  /** The tool name, e.g. `brain.search`. */
  name: string;
  /** The arguments, as JSON. */
  arguments: unknown;
  /** Whether the call errored. */
  error: string | null;
}

/** A structured multiple-choice question (`ask_choice` tool call). */
export interface ChoicePrompt {
  question: string;
  options: { label: string; value: string }[];
}

/** The renderable parts of one message. */
export interface ParsedMessage {
  thinking: string | null;
  toolCalls: ToolCallView[];
  choice: ChoicePrompt | null;
  body: string;
}

const THINKING = /<thinking>([\s\S]*?)<\/thinking>/i;
const TOOL_CALL_LINE = /^tool_call\(([^)]+)\):\s*(.*)$/;
const TOOL_RESULT_LINE = /^tool_result\(([^)]+)\):\s*(.*)$/;

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : {};
}

/** Tolerant extraction of tool calls from a message's `tool_calls` column. */
export function structuredToolCalls(raw: unknown): ToolCallView[] {
  const record = asRecord(raw);
  const list = Array.isArray(raw) ? raw : Array.isArray(record.calls) ? record.calls : [];
  const calls: ToolCallView[] = [];
  for (const entry of list) {
    const call = asRecord(entry);
    const name = typeof call.name === 'string' ? call.name : null;
    if (!name) {
      continue;
    }
    const error = typeof call.error === 'string' ? call.error : null;
    calls.push({ name, arguments: call.arguments ?? null, error });
  }
  return calls;
}

function parseChoiceArguments(name: string, args: unknown): ChoicePrompt | null {
  if (name !== 'ask_choice' && name !== 'ask_user') {
    return null;
  }
  const record = asRecord(args);
  const question = typeof record.question === 'string' ? record.question : null;
  if (!question) {
    return null;
  }
  const rawOptions = Array.isArray(record.options) ? record.options : [];
  const options = rawOptions.flatMap((option) => {
    if (typeof option === 'string') {
      return [{ label: option, value: option }];
    }
    const item = asRecord(option);
    const label = typeof item.label === 'string' ? item.label : null;
    if (!label) {
      return [];
    }
    const value = typeof item.value === 'string' ? item.value : label;
    return [{ label, value }];
  });
  return options.length > 0 ? { question, options } : null;
}

/** Split a message into thinking, tool cards, a choice prompt, and body. */
export function parseMessage(message: ChatMessage): ParsedMessage {
  let body = message.content ?? '';
  let thinking: string | null = null;

  const thought = THINKING.exec(body);
  if (thought) {
    thinking = thought[1]?.trim() ?? '';
    body = body.replace(THINKING, '').trim();
  }

  const toolCalls: ToolCallView[] = [];
  let choice: ChoicePrompt | null = null;

  for (const call of structuredToolCalls(message.tool_calls)) {
    const parsed = parseChoiceArguments(call.name, call.arguments);
    if (parsed) {
      choice = parsed;
    } else {
      toolCalls.push(call);
    }
  }

  const kept: string[] = [];
  for (const line of body.split('\n')) {
    const call = TOOL_CALL_LINE.exec(line.trim());
    if (call) {
      let args: unknown = call[2];
      try {
        args = JSON.parse(call[2] ?? '');
      } catch {
        // Keep the raw text when the payload is not JSON.
      }
      const parsed = parseChoiceArguments(call[1] ?? '', args);
      if (parsed) {
        choice = parsed;
      } else {
        toolCalls.push({ name: call[1] ?? '', arguments: args, error: null });
      }
      continue;
    }
    const result = TOOL_RESULT_LINE.exec(line.trim());
    if (result) {
      continue;
    }
    kept.push(line);
  }

  return { thinking, toolCalls, choice, body: kept.join('\n').trim() };
}

/** Format a token count, e.g. `2.3k`. */
export function formatTokens(tokens: number): string {
  if (tokens >= 1000) {
    return `${(tokens / 1000).toFixed(1)}k`;
  }
  return String(tokens);
}

/** Format a USD cost with two decimals. */
export function formatCost(cost: number): string {
  return `$${cost.toFixed(2)}`;
}

/** The display title of a session. */
export function sessionTitle(session: ChatSession, fallback: string): string {
  const title = session.title?.trim();
  return title && title.length > 0 ? title : fallback;
}

/** The one-line summary shown under a session title. */
export function sessionMeta(session: ChatSession): string {
  const parts = [session.agent_id];
  if (session.model) {
    parts.push(session.model);
  }
  return parts.join(' · ');
}
