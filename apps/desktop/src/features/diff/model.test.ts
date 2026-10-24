import { describe, expect, it } from 'vitest';
import type { GitDiffHunk } from '@fergents/protocol';

import {
  acceptedCount,
  allHunkIds,
  applyHunks,
  applyPreview,
  buildPreview,
  buildPreviewFile,
  hunkStats,
  previewStats,
} from './model';

function hunk(
  header: string,
  oldStart: number,
  oldLines: number,
  lines: GitDiffHunk['lines'],
): GitDiffHunk {
  const additions = lines.filter((line) => line.kind === 'add').length;
  return {
    old_start: oldStart,
    old_lines: oldLines,
    new_start: oldStart,
    new_lines: additions,
    header,
    lines,
  };
}

function replace(header: string, oldStart: number, before: string, after: string): GitDiffHunk {
  return hunk(header, oldStart, 1, [
    { kind: 'del', content: before },
    { kind: 'add', content: after },
  ]);
}

describe('diff-preview model', () => {
  it('builds the proposed text and line stats from hunks', () => {
    const file = buildPreviewFile({
      path: 'src/a.ts',
      languageId: 'typescript',
      original: 'one\ntwo\nthree',
      hunks: [replace('@@ -1 +1 @@', 1, 'one', 'ONE'), replace('@@ -3 +3 @@', 3, 'three', 'THREE')],
    });

    expect(file.proposed).toBe('ONE\ntwo\nTHREE');
    expect(file.additions).toBe(2);
    expect(file.deletions).toBe(2);
    expect(file.hunks.map((item) => item.id)).toEqual(['src/a.ts:0', 'src/a.ts:1']);
  });

  it('applies only the accepted hunks', () => {
    const file = buildPreviewFile({
      path: 'src/a.ts',
      original: 'one\ntwo\nthree',
      hunks: [replace('@@ -1 +1 @@', 1, 'one', 'ONE'), replace('@@ -3 +3 @@', 3, 'three', 'THREE')],
    });

    expect(applyHunks(file.original, file.hunks, new Set(['src/a.ts:0']))).toBe('ONE\ntwo\nthree');
    expect(applyHunks(file.original, file.hunks, new Set(['src/a.ts:1']))).toBe('one\ntwo\nTHREE');
    expect(applyHunks(file.original, file.hunks, new Set())).toBe(file.original);
  });

  it('inserts lines for a zero-length hunk', () => {
    const file = buildPreviewFile({
      path: 'a.txt',
      original: 'a\nb',
      hunks: [hunk('@@ -1,0 +2,1 @@', 1, 0, [{ kind: 'add', content: 'x' }])],
    });

    expect(file.proposed).toBe('a\nx\nb');
  });

  it('builds a new file from a zero-start hunk', () => {
    const file = buildPreviewFile({
      path: 'new.txt',
      original: '',
      hunks: [
        hunk('@@ -0,0 +1,2 @@', 0, 0, [
          { kind: 'add', content: 'first' },
          { kind: 'add', content: 'second' },
        ]),
      ],
    });

    expect(file.proposed).toBe('first\nsecond');
  });

  it('summarizes hunk and preview stats', () => {
    expect(hunkStats(replace('@@', 1, 'a', 'b').lines)).toEqual({ additions: 1, deletions: 1 });

    const files = buildPreview([
      { path: 'a', original: 'a', hunks: [replace('@@', 1, 'a', 'b')] },
      { path: 'b', original: 'b', hunks: [replace('@@', 1, 'b', 'c')] },
    ]);
    expect(previewStats(files)).toEqual({ additions: 2, deletions: 2 });

    const accepted = new Set(['a:0']);
    expect(previewStats(files, accepted)).toEqual({ additions: 1, deletions: 1 });
    expect(allHunkIds(files)).toEqual(new Set(['a:0', 'b:0']));
    expect(acceptedCount(files[0]!, accepted)).toBe(1);
  });

  it('emits only files with accepted hunks and skips binaries', () => {
    const files = buildPreview([
      {
        path: 'a.ts',
        original: 'a',
        hunks: [replace('@@', 1, 'a', 'b')],
      },
      {
        path: 'b.ts',
        original: 'b',
        hunks: [replace('@@', 1, 'b', 'c')],
      },
      {
        path: 'image.png',
        original: '',
        binary: true,
        hunks: [replace('@@', 1, 'a', 'b')],
      },
    ]);

    const applied = applyPreview(files, new Set(['a.ts:0']));
    expect(applied).toEqual([{ path: 'a.ts', content: 'b', hunks: ['a.ts:0'] }]);
  });
});
