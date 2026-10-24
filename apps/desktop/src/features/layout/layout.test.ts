import { describe, expect, it } from 'vitest';

import {
  DEFAULT_LAYOUT,
  MAX_RATIO,
  MIN_RATIO,
  findSplit,
  parseLayout,
  resizeRatio,
  rootRatio,
  setRatio,
  type LayoutSplit,
} from './layout';

const STORED: LayoutSplit = {
  kind: 'split',
  id: 'root',
  direction: 'row',
  ratio: 0.3,
  first: { kind: 'pane', id: 'chat', view: 'chat' },
  second: { kind: 'pane', id: 'ide', view: 'ide' },
};

describe('parseLayout', () => {
  it('restores a valid persisted tree', () => {
    expect(parseLayout(STORED)).toEqual(STORED);
  });

  it('nests splits and clamps each ratio', () => {
    const nested = {
      ...STORED,
      ratio: 0.01,
      first: {
        kind: 'split',
        id: 'inner',
        direction: 'column',
        ratio: 0.99,
        first: { kind: 'pane', id: 'editor', view: 'editor' },
        second: { kind: 'pane', id: 'terminal', view: 'terminal' },
      },
    };
    const layout = parseLayout(nested);
    expect(rootRatio(layout)).toBe(MIN_RATIO);
    const inner = findSplit(layout, 'inner');
    expect(inner?.direction).toBe('column');
    expect(inner?.ratio).toBe(MAX_RATIO);
  });

  it('falls back to the default layout for missing or malformed trees', () => {
    const malformed: unknown[] = [
      null,
      undefined,
      42,
      'split',
      {},
      { kind: 'pane' },
      { kind: 'pane', id: 'chat', view: 'settings' },
      { kind: 'split', id: 'root', direction: 'row', ratio: 0.5 },
      {
        kind: 'split',
        id: 'root',
        direction: 'diagonal',
        ratio: 0.5,
        first: { kind: 'pane', id: 'chat', view: 'chat' },
        second: { kind: 'pane', id: 'ide', view: 'ide' },
      },
      {
        kind: 'split',
        id: 'root',
        direction: 'row',
        ratio: 'half',
        first: { kind: 'pane', id: 'chat', view: 'chat' },
        second: { kind: 'pane', id: 'ide', view: 'ide' },
      },
    ];
    for (const raw of malformed) {
      expect(parseLayout(raw)).toEqual(DEFAULT_LAYOUT);
    }
  });

  it('rejects trees nested beyond the allowed depth', () => {
    let node: unknown = { kind: 'pane', id: 'chat', view: 'chat' };
    for (let depth = 0; depth < 6; depth += 1) {
      node = {
        kind: 'split',
        id: `split-${depth}`,
        direction: 'row',
        ratio: 0.5,
        first: node,
        second: { kind: 'pane', id: 'ide', view: 'ide' },
      };
    }
    expect(parseLayout(node)).toEqual(DEFAULT_LAYOUT);
  });
});

describe('setRatio', () => {
  it('updates only the target split', () => {
    const nested: LayoutSplit = {
      ...STORED,
      first: {
        kind: 'split',
        id: 'inner',
        direction: 'column',
        ratio: 0.5,
        first: { kind: 'pane', id: 'editor', view: 'editor' },
        second: { kind: 'pane', id: 'terminal', view: 'terminal' },
      },
    };
    const moved = setRatio(nested, 'inner', 0.7);
    expect(moved).not.toBe(nested);
    expect(rootRatio(moved)).toBe(0.3);
    expect(findSplit(moved, 'inner')?.ratio).toBe(0.7);
  });

  it('clamps out-of-range ratios', () => {
    expect(findSplit(setRatio(STORED, 'root', 5), 'root')?.ratio).toBe(MAX_RATIO);
    expect(findSplit(setRatio(STORED, 'root', -1), 'root')?.ratio).toBe(MIN_RATIO);
  });

  it('leaves a pane unchanged', () => {
    const pane = { kind: 'pane', id: 'chat', view: 'chat' } as const;
    expect(setRatio(pane, 'root', 0.2)).toBe(pane);
  });
});

describe('resizeRatio', () => {
  it('moves by one step and clamps at the bounds', () => {
    expect(resizeRatio(0.5, 'ArrowRight')).toBeCloseTo(0.55);
    expect(resizeRatio(0.5, 'ArrowDown')).toBeCloseTo(0.55);
    expect(resizeRatio(0.5, 'ArrowLeft')).toBeCloseTo(0.45);
    expect(resizeRatio(0.5, 'ArrowUp')).toBeCloseTo(0.45);
    expect(resizeRatio(MAX_RATIO, 'ArrowRight')).toBe(MAX_RATIO);
    expect(resizeRatio(MIN_RATIO, 'ArrowLeft')).toBe(MIN_RATIO);
  });

  it('jumps to the bounds with Home and End', () => {
    expect(resizeRatio(0.5, 'Home')).toBe(MIN_RATIO);
    expect(resizeRatio(0.5, 'End')).toBe(MAX_RATIO);
  });

  it('ignores unrelated keys', () => {
    expect(resizeRatio(0.4, 'Enter')).toBeCloseTo(0.4);
  });
});
