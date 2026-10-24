import { describe, expect, it } from 'vitest';

import {
  codeActionsFrom,
  definitionTarget,
  hoverMarkdown,
  pathFromModelUri,
  relativePathFromUri,
  workspaceEditChanges,
} from './lspResult';

const RANGE = {
  start: { line: 11, character: 4 },
  end: { line: 11, character: 8 },
};

const EDIT = { range: RANGE, newText: 'renamed' };

describe('pathFromModelUri', () => {
  it('round-trips the encoded path', () => {
    expect(pathFromModelUri('inmemory://fergents/src%2Fa%20b.ts')).toBe('src/a b.ts');
    expect(pathFromModelUri('file:///tmp/a.ts')).toBeNull();
    expect(pathFromModelUri('inmemory://fergents/%E0%A4%A')).toBeNull();
  });
});

describe('relativePathFromUri', () => {
  it('resolves Windows file URIs against the project root', () => {
    expect(relativePathFromUri('file:///C:/work/demo/src/main.ts', 'C:\\work\\demo')).toBe(
      'src/main.ts',
    );
    expect(relativePathFromUri('file:///C:/work/demo/src/main.ts', 'c:/work/demo')).toBe(
      'src/main.ts',
    );
  });

  it('resolves POSIX file URIs against the project root', () => {
    expect(relativePathFromUri('file:///home/me/project/src/a.ts', '/home/me/project')).toBe(
      'src/a.ts',
    );
  });

  it('rejects paths outside the project and non-file URIs', () => {
    expect(relativePathFromUri('file:///C:/other/a.ts', 'C:/work/demo')).toBeNull();
    expect(relativePathFromUri('untitled:Untitled-1', 'C:/work/demo')).toBeNull();
  });
});

describe('definitionTarget', () => {
  it('reads a Location', () => {
    expect(
      definitionTarget({ uri: 'file:///C:/work/demo/src/a.ts', range: RANGE }, 'C:/work/demo'),
    ).toEqual({ path: 'src/a.ts', line: 12, column: 5 });
  });

  it('reads the first LocationLink and uses targetSelectionRange', () => {
    expect(
      definitionTarget(
        [
          {
            targetUri: 'file:///C:/work/demo/src/b.ts',
            targetRange: RANGE,
            targetSelectionRange: {
              start: { line: 2, character: 0 },
              end: { line: 2, character: 3 },
            },
          },
        ],
        'C:/work/demo',
      ),
    ).toEqual({ path: 'src/b.ts', line: 3, column: 1 });
  });

  it('ignores out-of-project and malformed results', () => {
    expect(
      definitionTarget({ uri: 'file:///C:/elsewhere/a.ts', range: RANGE }, 'C:/work/demo'),
    ).toBeNull();
    expect(definitionTarget(null, 'C:/work/demo')).toBeNull();
    expect(definitionTarget([], 'C:/work/demo')).toBeNull();
  });
});

describe('hoverMarkdown', () => {
  it('reads strings, MarkupContent, MarkedString and arrays', () => {
    expect(hoverMarkdown({ contents: 'plain' })).toBe('plain');
    expect(hoverMarkdown({ contents: { kind: 'markdown', value: '**bold**' } })).toBe('**bold**');
    expect(hoverMarkdown({ contents: { language: 'rust', value: 'fn a()' } })).toBe(
      '```rust\nfn a()\n```',
    );
    expect(hoverMarkdown({ contents: ['one', { value: 'two' }] })).toBe('one\n\ntwo');
    expect(hoverMarkdown({ contents: '' })).toBeNull();
    expect(hoverMarkdown(null)).toBeNull();
  });
});

describe('workspaceEditChanges', () => {
  it('reads the `changes` map and filters invalid edits', () => {
    const changes = workspaceEditChanges(
      {
        changes: {
          'file:///C:/work/demo/src/a.ts': [EDIT, { newText: 'junk' }],
          'file:///C:/elsewhere/b.ts': [EDIT],
        },
      },
      'C:/work/demo',
    );
    expect(changes).toEqual([{ path: 'src/a.ts', edits: [EDIT] }]);
  });

  it('reads `documentChanges`', () => {
    const changes = workspaceEditChanges(
      {
        documentChanges: [
          { textDocument: { uri: 'file:///C:/work/demo/src/a.ts' }, edits: [EDIT] },
        ],
      },
      'C:/work/demo',
    );
    expect(changes).toEqual([{ path: 'src/a.ts', edits: [EDIT] }]);
  });

  it('returns nothing for malformed input', () => {
    expect(workspaceEditChanges(null, 'C:/work/demo')).toEqual([]);
    expect(workspaceEditChanges({}, 'C:/work/demo')).toEqual([]);
  });
});

describe('codeActionsFrom', () => {
  it('keeps titles and inline edits', () => {
    const actions = codeActionsFrom(
      [
        {
          title: 'Add missing import',
          edit: { changes: { 'file:///C:/work/demo/src/a.ts': [EDIT] } },
        },
        { title: 'Run command only', command: 'do.it' },
        { nope: true },
      ],
      'C:/work/demo',
    );
    expect(actions).toEqual([
      { title: 'Add missing import', edits: [{ path: 'src/a.ts', edits: [EDIT] }] },
      { title: 'Run command only', edits: [] },
    ]);
  });
});
