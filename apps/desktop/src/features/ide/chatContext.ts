/**
 * Editor → chat context (task 3.14). The kernel's `chat.message.send` takes
 * text only, so a selection travels as a `@path#Lx-Ly` mention followed by the
 * code in a fence (spec §Codebase Awareness).
 */

/** An editor selection attached to the chat draft. */
export interface SelectionAttachment {
  path: string;
  /** One-based first line of the selection. */
  startLine: number;
  /** One-based last line of the selection. */
  endLine: number;
  text: string;
}

/** The `@path#Lx-Ly` mention of a selection (`#Lx` for a single line). */
export function selectionMention(selection: SelectionAttachment): string {
  return selection.startLine === selection.endLine
    ? `@${selection.path}#L${selection.startLine}`
    : `@${selection.path}#L${selection.startLine}-L${selection.endLine}`;
}

/** The chat payload for a selection: the mention, then the code in a fence. */
export function selectionPayload(selection: SelectionAttachment): string {
  const body = selection.text.replace(/\s+$/, '');
  return `${selectionMention(selection)}\n\`\`\`\n${body}\n\`\`\``;
}

/** Append a context payload to an existing draft. */
export function appendContext(draft: string, payload: string): string {
  const body = draft.trimEnd();
  return body.length === 0 ? `${payload} ` : `${body}\n${payload} `;
}
