/**
 * The vault file tree (UI.md §5.2). Renders `buildFileTree` output as nested
 * `.tree-row`s; folders are collapsible, files are selectable.
 *
 * Keyboard navigation follows the standard tree pattern: Up/Down move between
 * visible rows, Right expands (or descends into a folder), Left collapses (or
 * ascends to the parent), Enter/Space activates. Focus is roving: exactly one
 * row is tabbable at a time.
 */
import {
  useCallback,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactElement,
} from 'react';

import { t } from '../../i18n/i18n';
import { buildFileTree, flattenVisibleTree, type FileNode, type TreeSource } from './tree';

/** A changed-file marker rendered next to a file row (UI.md §8.2). */
export interface TreeBadge {
  /** One-letter marker (`M`, `A`, `D`, …). */
  letter: string;
  /** Tone class suffix (added, modified, deleted, …). */
  tone: string;
  /** Accessible description, e.g. "Modified". */
  title?: string | undefined;
}

/** Props for `FileTree`. */
export interface FileTreeProps {
  /** Flat document rows; the tree is derived internally. */
  documents: readonly TreeSource[];
  /** Currently selected document path. */
  activePath?: string | undefined;
  /** Called with the vault-relative path of a selected file. */
  onSelect?: ((path: string) => void) | undefined;
  /** Folder paths expanded on first render. */
  defaultExpanded?: readonly string[] | undefined;
  /** Badges for changed files, keyed by path. */
  status?: Readonly<Record<string, TreeBadge>> | undefined;
  /** Accessible name of the tree. */
  label?: string | undefined;
}

function depthStyle(depth: number): CSSProperties {
  return { '--depth': depth } as CSSProperties;
}

function parentPath(path: string): string | undefined {
  const index = path.lastIndexOf('/');
  return index <= 0 ? undefined : path.slice(0, index);
}

function FileIcon({ folder }: { folder: boolean }): ReactElement {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d={folder ? 'M3 7h6l2 2h8v9H3z' : 'M6 3h8l4 4v14H6zM14 3v4h4'} />
    </svg>
  );
}

function Caret({ open }: { open: boolean }): ReactElement {
  return (
    <svg viewBox="0 0 12 12" aria-hidden="true">
      <path d={open ? 'M3 4l3 4 3-4' : 'M4 3l4 3-4 3'} />
    </svg>
  );
}

/** The collapsible vault file tree. */
export function FileTree({
  documents,
  activePath,
  onSelect,
  defaultExpanded = [],
  status,
  label,
}: FileTreeProps): ReactElement {
  const nodes = useMemo(() => buildFileTree(documents), [documents]);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set(defaultExpanded));
  const [focusPath, setFocusPath] = useState<string | undefined>(undefined);
  const rowRefs = useRef(new Map<string, HTMLDivElement>());

  const isExpanded = useCallback((path: string) => expanded.has(path), [expanded]);

  const toggle = useCallback((path: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const visible = useMemo(() => flattenVisibleTree(nodes, isExpanded), [nodes, isExpanded]);

  const focusRow = useCallback((path: string | undefined) => {
    setFocusPath(path);
    if (path) {
      rowRefs.current.get(path)?.focus();
    }
  }, []);

  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>, node: FileNode): void => {
    const folder = node.kind === 'dir';
    const open = folder && expanded.has(node.path);
    const index = visible.findIndex((candidate) => candidate.path === node.path);

    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        focusRow(visible[Math.min(index + 1, visible.length - 1)]?.path);
        break;
      case 'ArrowUp':
        event.preventDefault();
        focusRow(visible[Math.max(index - 1, 0)]?.path);
        break;
      case 'Home':
        event.preventDefault();
        focusRow(visible[0]?.path);
        break;
      case 'End':
        event.preventDefault();
        focusRow(visible[visible.length - 1]?.path);
        break;
      case 'ArrowRight':
        if (folder && !open) {
          event.preventDefault();
          toggle(node.path);
        } else if (folder) {
          event.preventDefault();
          focusRow(visible[index + 1]?.path);
        }
        break;
      case 'ArrowLeft':
        if (folder && open) {
          event.preventDefault();
          toggle(node.path);
        } else {
          event.preventDefault();
          focusRow(parentPath(node.path));
        }
        break;
      case 'Enter':
      case ' ':
        event.preventDefault();
        if (folder) {
          toggle(node.path);
        } else {
          onSelect?.(node.path);
        }
        break;
      default:
        break;
    }
  };

  const renderNodes = (list: readonly FileNode[], depth: number): ReactElement[] =>
    list.map((node) => {
      const folder = node.kind === 'dir';
      const open = folder && expanded.has(node.path);
      const selected = !folder && node.path === activePath;
      const badge = folder ? undefined : status?.[node.path];
      const tabbable = focusPath ? node.path === focusPath : node.path === visible[0]?.path;
      return (
        <div key={node.path} className="tree-node">
          <div
            ref={(element) => {
              if (element) {
                rowRefs.current.set(node.path, element);
              } else {
                rowRefs.current.delete(node.path);
              }
            }}
            className={['tree-row', folder ? 'is-dir' : '', selected ? 'is-selected' : '']
              .filter(Boolean)
              .join(' ')}
            style={depthStyle(depth)}
            role="treeitem"
            aria-expanded={folder ? open : undefined}
            aria-selected={selected ? true : undefined}
            data-path={node.path}
            tabIndex={tabbable ? 0 : -1}
            onClick={() => (folder ? toggle(node.path) : onSelect?.(node.path))}
            onFocus={() => setFocusPath(node.path)}
            onKeyDown={(event) => onKeyDown(event, node)}
          >
            <span className={`tree-caret${open ? ' is-open' : ''}`}>
              {folder ? <Caret open={open} /> : null}
            </span>
            <span className={`tree-icon${folder ? ' is-dir' : ''}`}>
              <FileIcon folder={folder} />
            </span>
            <span className="tree-label">{node.name}</span>
            {badge ? (
              <span
                className={`tree-badge tree-badge--${badge.tone}`}
                aria-label={badge.title}
                title={badge.title}
              >
                {badge.letter}
              </span>
            ) : null}
          </div>
          {open && node.children.length > 0 ? renderNodes(node.children, depth + 1) : null}
        </div>
      );
    });

  return (
    <div className="tree" role="tree" aria-label={label ?? t('explorer.files.label')}>
      {visible.length === 0 ? (
        <div className="empty">
          <div className="empty-title">{t('explorer.files.empty.title')}</div>
          <div className="empty-desc">{t('explorer.files.empty.desc')}</div>
        </div>
      ) : (
        renderNodes(nodes, 0)
      )}
    </div>
  );
}
