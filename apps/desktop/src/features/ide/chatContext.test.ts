import { describe, expect, it } from 'vitest';

import { appendContext, selectionMention, selectionPayload } from './chatContext';

describe('selectionMention', () => {
  it('uses a single line marker for a one-line selection', () => {
    expect(
      selectionMention({ path: 'src/main.ts', startLine: 4, endLine: 4, text: 'const a = 1;' }),
    ).toBe('@src/main.ts#L4');
  });

  it('uses a range marker for a multi-line selection', () => {
    expect(
      selectionMention({ path: 'src/main.ts', startLine: 4, endLine: 9, text: 'const a = 1;' }),
    ).toBe('@src/main.ts#L4-L9');
  });
});

describe('selectionPayload', () => {
  it('puts the mention before the fenced code', () => {
    const payload = selectionPayload({
      path: 'src/main.ts',
      startLine: 2,
      endLine: 3,
      text: 'const a = 1;\nconst b = 2;\n',
    });

    expect(payload).toBe('@src/main.ts#L2-L3\n```\nconst a = 1;\nconst b = 2;\n```');
  });
});

describe('appendContext', () => {
  it('returns the payload alone for an empty draft', () => {
    expect(appendContext('', '@a.ts')).toBe('@a.ts ');
    expect(appendContext('  \n', '@a.ts')).toBe('@a.ts ');
  });

  it('appends the payload after the existing draft', () => {
    expect(appendContext('explain this', '@a.ts')).toBe('explain this\n@a.ts ');
  });

  it('keeps both attachments when called twice', () => {
    const once = appendContext('', '@a.ts');

    expect(appendContext(once, '@b.ts')).toBe('@a.ts\n@b.ts ');
  });
});
