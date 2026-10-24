import { describe, expect, it } from 'vitest';

import { parseOutline } from './outline';

describe('parseOutline', () => {
  it('normalises hierarchical DocumentSymbol results', () => {
    const items = parseOutline([
      {
        name: 'main',
        detail: 'fn()',
        kind: 12,
        range: { start: { line: 0, character: 0 }, end: { line: 4, character: 1 } },
        selectionRange: { start: { line: 0, character: 3 }, end: { line: 0, character: 7 } },
        children: [
          {
            name: 'helper',
            kind: 12,
            range: { start: { line: 6, character: 0 }, end: { line: 6, character: 20 } },
            selectionRange: { start: { line: 6, character: 3 }, end: { line: 6, character: 9 } },
            children: [],
          },
        ],
      },
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]?.name).toBe('main');
    expect(items[0]?.detail).toBe('fn()');
    expect(items[0]?.line).toBe(1);
    expect(items[0]?.column).toBe(4);
    expect(items[0]?.children[0]?.line).toBe(7);
  });

  it('normalises flat SymbolInformation results', () => {
    const items = parseOutline([
      {
        name: 'Thing',
        kind: 5,
        location: {
          uri: 'file:///a.ts',
          range: { start: { line: 9, character: 2 }, end: { line: 12, character: 0 } },
        },
      },
    ]);

    expect(items[0]).toMatchObject({ name: 'Thing', detail: null, line: 10, column: 3, kind: 5 });
  });

  it('drops malformed entries and non-arrays', () => {
    expect(parseOutline(null)).toEqual([]);
    expect(parseOutline({ result: [] })).toEqual([]);
    expect(
      parseOutline([
        null,
        42,
        { name: 'no-range' },
        { name: 'ok', kind: 1, range: { start: { line: 0, character: 0 } } },
      ]),
    ).toEqual([{ name: 'ok', detail: null, kind: 1, line: 1, column: 1, children: [] }]);
  });
});
