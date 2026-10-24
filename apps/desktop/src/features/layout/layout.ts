/**
 * The layout tree (UI.md §10, spec decision D14).
 *
 * A layout is either a pane (a single view) or a split (two children laid out
 * horizontally or vertically, with a ratio). The tree is persisted per
 * project through `editor.layout.get` / `editor.layout.set`; a missing or
 * invalid tree falls back to the default chat + IDE split.
 *
 * Everything here is pure so it can be unit-tested without React.
 */

/** The views a pane can host in Phase 3. */
export type PaneView = 'chat' | 'ide' | 'editor' | 'terminal';

/** The pane views accepted from a stored layout. */
export const PANE_VIEWS: readonly PaneView[] = ['chat', 'ide', 'editor', 'terminal'];

/** Smallest share of the space a pane may occupy. */
export const MIN_RATIO = 0.15;
/** Largest share of the space a pane may occupy. */
export const MAX_RATIO = 0.85;
/** How much one arrow-key press moves the divider. */
export const KEYBOARD_STEP = 0.05;
/** How deep a persisted tree may nest before it is rejected. */
const MAX_DEPTH = 4;

/** A leaf of the layout tree: one view filling its pane. */
export interface LayoutPane {
  kind: 'pane';
  id: string;
  view: PaneView;
}

/** A split of the layout tree: two children sharing the space. */
export interface LayoutSplit {
  kind: 'split';
  id: string;
  direction: 'row' | 'column';
  /** Share of the space given to `first`, clamped to [`MIN_RATIO`, `MAX_RATIO`]. */
  ratio: number;
  first: LayoutNode;
  second: LayoutNode;
}

/** A node of the layout tree. */
export type LayoutNode = LayoutPane | LayoutSplit;

/** The default Phase 3 layout: chat left, IDE right (spec §7). */
export const DEFAULT_LAYOUT: LayoutSplit = {
  kind: 'split',
  id: 'root',
  direction: 'row',
  ratio: 0.5,
  first: { kind: 'pane', id: 'chat', view: 'chat' },
  second: { kind: 'pane', id: 'ide', view: 'ide' },
};

/** Clamp a ratio into the allowed range; non-finite input becomes 0.5. */
export function clampRatio(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_LAYOUT.ratio;
  }
  return Math.min(MAX_RATIO, Math.max(MIN_RATIO, value));
}

function nonEmptyId(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function parseNode(raw: unknown, depth: number): LayoutNode | null {
  if (depth > MAX_DEPTH || typeof raw !== 'object' || raw === null) {
    return null;
  }
  const record = raw as Record<string, unknown>;
  const id = nonEmptyId(record.id);
  if (id === null) {
    return null;
  }
  if (record.kind === 'pane') {
    const view = PANE_VIEWS.find((candidate) => candidate === record.view);
    return view ? { kind: 'pane', id, view } : null;
  }
  if (record.kind === 'split') {
    const direction =
      record.direction === 'row' || record.direction === 'column' ? record.direction : null;
    const ratio = typeof record.ratio === 'number' ? record.ratio : Number.NaN;
    if (direction === null || !Number.isFinite(ratio)) {
      return null;
    }
    const first = parseNode(record.first, depth + 1);
    const second = parseNode(record.second, depth + 1);
    if (!first || !second) {
      return null;
    }
    return { kind: 'split', id, direction, ratio: clampRatio(ratio), first, second };
  }
  return null;
}

/**
 * Validate a stored layout, falling back to `DEFAULT_LAYOUT`.
 *
 * A stored tree is accepted only when every node is well formed; anything
 * else (null, a stale shape, a bad ratio, too much nesting) restores defaults
 * rather than rendering a half-broken workspace.
 */
export function parseLayout(raw: unknown): LayoutNode {
  return parseNode(raw, 0) ?? DEFAULT_LAYOUT;
}

/** Immutably set the ratio of the split identified by `id`. */
export function setRatio(node: LayoutNode, id: string, ratio: number): LayoutNode {
  if (node.kind === 'pane') {
    return node;
  }
  if (node.id === id) {
    return { ...node, ratio: clampRatio(ratio) };
  }
  return {
    ...node,
    first: setRatio(node.first, id, ratio),
    second: setRatio(node.second, id, ratio),
  };
}

/** Move a ratio for a keyboard event; keys that do not resize return it unchanged. */
export function resizeRatio(ratio: number, key: string, step: number = KEYBOARD_STEP): number {
  switch (key) {
    case 'ArrowLeft':
    case 'ArrowUp':
      return clampRatio(ratio - step);
    case 'ArrowRight':
    case 'ArrowDown':
      return clampRatio(ratio + step);
    case 'Home':
      return MIN_RATIO;
    case 'End':
      return MAX_RATIO;
    default:
      return clampRatio(ratio);
  }
}

/** Find a split by id (the root when omitted). */
export function findSplit(node: LayoutNode, id: string = DEFAULT_LAYOUT.id): LayoutSplit | null {
  if (node.kind === 'pane') {
    return null;
  }
  if (node.id === id) {
    return node;
  }
  return findSplit(node.first, id) ?? findSplit(node.second, id);
}

/** The ratio of the root split (0.5 for a single pane). */
export function rootRatio(node: LayoutNode): number {
  return node.kind === 'split' ? node.ratio : DEFAULT_LAYOUT.ratio;
}
