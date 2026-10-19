import { describe, expect, it } from 'vitest';

import { buildFileTree, buildOutline, flattenFileTree, flattenVisibleTree } from './tree';

describe('buildFileTree', () => {
  it('creates intermediate folders implicitly and nests leaves', () => {
    const tree = buildFileTree([{ path: 'notes/deep/a.md' }]);

    expect(tree.map((node) => node.name)).toEqual(['notes']);
    expect(tree[0]?.kind).toBe('dir');
    expect(tree[0]?.children[0]?.name).toBe('deep');
    expect(tree[0]?.children[0]?.children[0]).toMatchObject({
      name: 'a.md',
      path: 'notes/deep/a.md',
      kind: 'file',
    });
  });

  it('sorts folders before files, then by name', () => {
    const tree = buildFileTree([{ path: 'z.md' }, { path: 'a.md' }, { path: 'notes/b.md' }]);

    expect(tree.map((node) => node.name)).toEqual(['notes', 'a.md', 'z.md']);
  });

  it('ignores empty paths', () => {
    expect(buildFileTree([{ path: '' }])).toEqual([]);
  });
});

describe('buildOutline', () => {
  it('nests headings by level', () => {
    const outline = buildOutline([
      { level: 1, text: 'Title' },
      { level: 2, text: 'Section' },
      { level: 3, text: 'Sub' },
      { level: 2, text: 'Next' },
    ]);

    expect(outline.map((node) => node.text)).toEqual(['Title']);
    expect(outline[0]?.children.map((node) => node.text)).toEqual(['Section', 'Next']);
    expect(outline[0]?.children[0]?.children.map((node) => node.text)).toEqual(['Sub']);
  });

  it('normalizes an outline that starts below H1', () => {
    const outline = buildOutline([
      { level: 3, text: 'A' },
      { level: 3, text: 'B' },
    ]);

    expect(outline.map((node) => node.text)).toEqual(['A', 'B']);
    expect(outline[0]?.children).toEqual([]);
  });
});

describe('flatten helpers', () => {
  const nested = buildFileTree([{ path: 'a/b.md' }, { path: 'a/c.md' }]);

  it('flattens the whole tree depth-first', () => {
    expect(flattenFileTree(nested).map((node) => node.name)).toEqual(['a', 'b.md', 'c.md']);
  });

  it('stops at collapsed folders', () => {
    expect(flattenVisibleTree(nested, () => false).map((node) => node.name)).toEqual(['a']);
  });

  it('descends into expanded folders', () => {
    expect(flattenVisibleTree(nested, (path) => path === 'a').map((node) => node.name)).toEqual([
      'a',
      'b.md',
      'c.md',
    ]);
  });
});
