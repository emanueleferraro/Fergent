/**
 * Pure helpers for the brain explorer.
 *
 * They turn flat kernel rows (`doc.list`, `doc.outline`) into the nested
 * structures the sidebar renders. Kept free of React so they are trivially
 * unit-testable.
 */
import type { OutlineEntry } from '@fergents/protocol';

/** The minimal shape the file tree needs from a document row. */
export interface TreeSource {
  /** Vault-relative path (`a/b.md`). */
  path: string;
}

/** A node of the vault file tree. */
export interface FileNode {
  /** Display name (last path segment). */
  name: string;
  /** Vault-relative path (`a/b.md`). */
  path: string;
  kind: 'dir' | 'file';
  children: FileNode[];
}

/** A node of a note outline (a heading plus its nested sub-headings). */
export interface OutlineNode extends OutlineEntry {
  children: OutlineNode[];
}

function compareNodes(a: FileNode, b: FileNode): number {
  if (a.kind !== b.kind) {
    return a.kind === 'dir' ? -1 : 1;
  }
  return a.name.localeCompare(b.name);
}

/**
 * Build a nested file tree from flat document rows.
 *
 * Intermediate folders are created implicitly, so a lone `notes/a/b.md`
 * yields `notes/` → `a/` → `b.md`.
 */
export function buildFileTree(documents: readonly TreeSource[]): FileNode[] {
  const roots: FileNode[] = [];

  for (const document of documents) {
    const segments = document.path.split('/').filter((segment) => segment.length > 0);
    if (segments.length === 0) {
      continue;
    }
    let level = roots;
    let prefix = '';
    segments.forEach((segment, index) => {
      prefix = prefix ? `${prefix}/${segment}` : segment;
      const isLeaf = index === segments.length - 1;
      let node = level.find(
        (candidate) => candidate.name === segment && candidate.kind === (isLeaf ? 'file' : 'dir'),
      );
      if (!node) {
        node = {
          name: segment,
          path: isLeaf ? document.path : prefix,
          kind: isLeaf ? 'file' : 'dir',
          children: [],
        };
        level.push(node);
      }
      level = node.children;
    });
  }

  const sortRecursive = (nodes: FileNode[]): void => {
    nodes.sort(compareNodes);
    for (const node of nodes) {
      sortRecursive(node.children);
    }
  };
  sortRecursive(roots);
  return roots;
}

/**
 * Nest a flat outline by heading level.
 *
 * A heading with a greater level than its predecessor becomes its child; an
 * equal or lower level closes the open branch. Levels are normalized relative
 * to the first heading, so an outline that starts at H2 still nests correctly.
 */
export function buildOutline(entries: readonly OutlineEntry[]): OutlineNode[] {
  const roots: OutlineNode[] = [];
  const stack: OutlineNode[] = [];

  for (const entry of entries) {
    const node: OutlineNode = { ...entry, children: [] };
    let parent = stack[stack.length - 1];
    while (parent !== undefined && parent.level >= node.level) {
      stack.pop();
      parent = stack[stack.length - 1];
    }
    if (parent === undefined) {
      roots.push(node);
    } else {
      parent.children.push(node);
    }
    stack.push(node);
  }

  return roots;
}

/** Flatten a file tree depth-first (used for keyboard navigation). */
export function flattenFileTree(nodes: readonly FileNode[]): FileNode[] {
  const out: FileNode[] = [];
  const walk = (list: readonly FileNode[]): void => {
    for (const node of list) {
      out.push(node);
      if (node.children.length > 0) {
        walk(node.children);
      }
    }
  };
  walk(nodes);
  return out;
}

/**
 * Flatten a file tree depth-first, descending only into expanded folders.
 *
 * This is the row order the tree actually renders, and therefore the order
 * keyboard navigation moves through.
 */
export function flattenVisibleTree(
  nodes: readonly FileNode[],
  isExpanded: (path: string) => boolean,
): FileNode[] {
  const out: FileNode[] = [];
  const walk = (list: readonly FileNode[]): void => {
    for (const node of list) {
      out.push(node);
      if (node.kind === 'dir' && isExpanded(node.path) && node.children.length > 0) {
        walk(node.children);
      }
    }
  };
  walk(nodes);
  return out;
}
