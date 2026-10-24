/**
 * Inline AI model (spec §6, task 3.12): when a completion may run, which part
 * of the buffer is sent, and how a suggestion is normalized before it reaches
 * the editor. Pure functions so the engine stays testable without Monaco.
 */

/** Debounce before asking the kernel (spec §6: ~300 ms). */
export const INLINE_DEBOUNCE_MS = 300;

/** Files above this size never ask for completions (spec §6). */
export const INLINE_MAX_FILE_CHARS = 120_000;

/** Characters of context sent before and after the cursor. */
export const INLINE_CONTEXT_BEFORE = 8_000;
export const INLINE_CONTEXT_AFTER = 2_000;

/** Per-request completion budget, shown in the status bar. */
export const INLINE_MAX_TOKENS = 256;

/** Why inline completions are not running right now. */
export type InlineSuppression = 'disabled' | 'noProject' | 'untrusted' | 'readOnly' | 'largeFile';

/** The inputs that decide whether a completion request is allowed. */
export interface InlineEligibility {
  /** `capability.inline-ai` is enabled in the kernel. */
  enabled: boolean;
  hasProject: boolean;
  trusted: boolean;
  readOnly: boolean;
  fileChars: number;
}

/** The suppression reason, or `null` when completions are allowed. */
export function suppressionReason(state: InlineEligibility): InlineSuppression | null {
  if (!state.enabled) {
    return 'disabled';
  }
  if (!state.hasProject) {
    return 'noProject';
  }
  if (!state.trusted) {
    return 'untrusted';
  }
  if (state.readOnly) {
    return 'readOnly';
  }
  if (state.fileChars > INLINE_MAX_FILE_CHARS) {
    return 'largeFile';
  }
  return null;
}

/** The text around the cursor, bounded to the inline context window. */
export function contextAt(text: string, offset: number): { prefix: string; suffix: string } {
  const at = Math.max(0, Math.min(text.length, offset));
  return {
    prefix: text.slice(Math.max(0, at - INLINE_CONTEXT_BEFORE), at),
    suffix: text.slice(at, Math.min(text.length, at + INLINE_CONTEXT_AFTER)),
  };
}

/**
 * Normalize a model suggestion: strip a surrounding Markdown fence and drop
 * blank output. Leading indentation is preserved because it is inserted.
 */
export function cleanSuggestion(raw: string): string | null {
  let text = raw.replace(/\r\n/g, '\n');
  const fenced = /^```[^\n]*\n([\s\S]*?)\n?```\s*$/.exec(text.trim());
  if (fenced !== null && fenced[1] !== undefined) {
    text = fenced[1];
  }
  if (text.trim() === '') {
    return null;
  }
  return text.replace(/\s+$/, '');
}
