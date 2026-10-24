import { describe, expect, it } from 'vitest';

import { applyTextEdits, isTextEdit, type TextEdit } from './textEdits';

function edit(
  startLine: number,
  startCharacter: number,
  endLine: number,
  endCharacter: number,
  newText: string,
): TextEdit {
  return {
    range: {
      start: { line: startLine, character: startCharacter },
      end: { line: endLine, character: endCharacter },
    },
    newText,
  };
}

describe('applyTextEdits', () => {
  it('returns the text unchanged for no edits', () => {
    expect(applyTextEdits('a\nb\n', [])).toBe('a\nb\n');
  });

  it('replaces a range on one line', () => {
    expect(applyTextEdits('let a = 1;\n', [edit(0, 4, 0, 5, 'b')])).toBe('let b = 1;\n');
  });

  it('applies multiple edits from the end so offsets stay valid', () => {
    const text = 'first\nsecond\nthird\n';
    const edits = [edit(0, 0, 0, 5, '1st'), edit(2, 0, 2, 5, '3rd')];
    expect(applyTextEdits(text, edits)).toBe('1st\nsecond\n3rd\n');
  });

  it('inserts text at the end of a line', () => {
    expect(applyTextEdits('foo\n', [edit(0, 3, 0, 3, ' bar')])).toBe('foo bar\n');
  });

  it('clamps out-of-range positions instead of throwing', () => {
    expect(applyTextEdits('abc', [edit(5, 0, 9, 0, '!')])).toBe('abc!');
    expect(applyTextEdits('abc', [edit(0, 99, 0, 99, '!')])).toBe('abc!');
  });

  it('rejects malformed edits', () => {
    expect(
      isTextEdit({
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
        newText: 'x',
      }),
    ).toBe(true);
    expect(isTextEdit({ range: null, newText: 'x' })).toBe(false);
    expect(isTextEdit({ newText: 'x' })).toBe(false);
    expect(
      isTextEdit({
        range: { start: { line: -1, character: 0 }, end: { line: 0, character: 0 } },
        newText: 'x',
      }),
    ).toBe(false);
  });
});
