import { describe, expect, it } from 'vitest';
import type { GitFileDiff, GitStatusEntry } from '@fergents/protocol';

import {
  commitTime,
  countLines,
  diffStats,
  entryKey,
  fileDiffRows,
  groupEntries,
  parentPath,
} from './gitModel';

function entry(
  path: string,
  staged: boolean,
  kind: GitStatusEntry['kind'] = 'modified',
): GitStatusEntry {
  return { path, kind, staged };
}

function file(path: string, additions: string[], deletions: string[]): GitFileDiff {
  return {
    path,
    binary: false,
    hunks: [
      {
        old_start: 1,
        old_lines: deletions.length,
        new_start: 1,
        new_lines: additions.length,
        header: '@@',
        lines: [
          ...deletions.map((content) => ({ kind: 'del' as const, content })),
          ...additions.map((content) => ({ kind: 'add' as const, content })),
        ],
      },
    ],
  };
}

describe('source-control model', () => {
  it('keys an entry by where the change lives', () => {
    expect(entryKey(entry('src/a.ts', false))).toBe('worktree:modified:src/a.ts');
    expect(entryKey(entry('src/a.ts', true, 'added'))).toBe('staged:added:src/a.ts');
  });

  it('splits staged and unstaged entries, sorted by path', () => {
    const groups = groupEntries([
      entry('src/z.ts', false),
      entry('README.md', true),
      entry('src/a.ts', false),
      entry('src/b.ts', true),
    ]);

    expect(groups.staged.map((item) => item.path)).toEqual(['README.md', 'src/b.ts']);
    expect(groups.unstaged.map((item) => item.path)).toEqual(['src/a.ts', 'src/z.ts']);
  });

  it('reports the parent directory', () => {
    expect(parentPath('src/features/a.ts')).toBe('src/features');
    expect(parentPath('README.md')).toBe('');
  });

  it('counts added and removed lines across hunks', () => {
    const one = file('a.ts', ['new', 'lines'], ['old']);
    expect(countLines(one)).toEqual({ additions: 2, deletions: 1 });

    const totals = diffStats([one, file('b.ts', ['x'], [])]);
    expect(totals).toEqual({ additions: 3, deletions: 1 });
  });

  it('flattens a diff into prefixed rows with stable keys', () => {
    const rows = fileDiffRows(file('a.ts', ['new'], ['old']));

    expect(rows.map((row) => row.text)).toEqual(['-old', '+new']);
    expect(rows.map((row) => row.kind)).toEqual(['del', 'add']);
    expect(new Set(rows.map((row) => row.key)).size).toBe(2);
  });

  it('formats a commit timestamp as a locale date-time', () => {
    expect(commitTime(1_700_000_000)).toBe(new Date(1_700_000_000_000).toLocaleString());
  });
});
