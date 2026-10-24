/**
 * LSP `TextEdit[]` application (format-on-save, and later quick fixes and
 * rename in the LSP UI). The kernel returns the server's raw JSON, so the
 * shapes are validated here rather than trusted.
 */

/** A position in a document, zero-based (LSP convention). */
export interface TextPosition {
  line: number;
  character: number;
}

/** A range in a document, zero-based (LSP convention). */
export interface TextRange {
  start: TextPosition;
  end: TextPosition;
}

/** One replacement of a range with new text. */
export interface TextEdit {
  range: TextRange;
  newText: string;
}

function isPosition(value: unknown): value is TextPosition {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.line === 'number' &&
    typeof candidate.character === 'number' &&
    Number.isInteger(candidate.line) &&
    Number.isInteger(candidate.character) &&
    candidate.line >= 0 &&
    candidate.character >= 0
  );
}

/** Narrow an unknown value to a `TextEdit`. */
export function isTextEdit(value: unknown): value is TextEdit {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  const range = candidate.range as Record<string, unknown> | undefined;
  return (
    typeof candidate.newText === 'string' &&
    typeof range === 'object' &&
    range !== null &&
    isPosition(range.start) &&
    isPosition(range.end)
  );
}

/**
 * Apply text edits to `text`.
 *
 * Edits are applied from the end of the document backwards so earlier offsets
 * stay valid; out-of-range positions are clamped, and overlapping edits are
 * applied in sequence (the last one wins over the overlap).
 */
export function applyTextEdits(text: string, edits: readonly TextEdit[]): string {
  const lines = text.split('\n');
  const offsets: number[] = [];
  let offset = 0;
  for (const line of lines) {
    offsets.push(offset);
    offset += line.length + 1;
  }

  const lineStart = (line: number): number =>
    offsets[Math.min(line, lines.length - 1)] ?? text.length;
  const toOffset = (position: TextPosition): number => {
    if (position.line >= lines.length) {
      return text.length;
    }
    const line = lines[position.line] ?? '';
    return lineStart(position.line) + Math.min(position.character, line.length);
  };

  const ordered = edits
    .map((edit) => ({
      start: toOffset(edit.range.start),
      end: toOffset(edit.range.end),
      newText: edit.newText,
    }))
    .sort((left, right) => right.start - left.start || right.end - left.end);

  let result = text;
  for (const edit of ordered) {
    const start = Math.min(edit.start, result.length);
    const end = Math.min(Math.max(edit.end, start), result.length);
    result = `${result.slice(0, start)}${edit.newText}${result.slice(end)}`;
  }
  return result;
}
