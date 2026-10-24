/**
 * Pure helpers for the source-control panel (UI.md §8.4, task 3.10): grouping
 * status entries, diff statistics, and the rows the inline diff renders.
 */
import type { GitDiffLine, GitFileDiff, GitStatusEntry } from '@fergents/protocol';

/** The two halves of the change list. */
export interface ChangeGroups {
  staged: GitStatusEntry[];
  unstaged: GitStatusEntry[];
}

/** Stable React key for one entry (a path can be both staged and modified). */
export function entryKey(entry: GitStatusEntry): string {
  return `${entry.staged ? 'staged' : 'worktree'}:${entry.kind}:${entry.path}`;
}

/** Split entries into staged and unstaged, each sorted by path. */
export function groupEntries(entries: readonly GitStatusEntry[]): ChangeGroups {
  const staged: GitStatusEntry[] = [];
  const unstaged: GitStatusEntry[] = [];
  for (const entry of entries) {
    (entry.staged ? staged : unstaged).push(entry);
  }
  const byPath = (left: GitStatusEntry, right: GitStatusEntry): number =>
    left.path.localeCompare(right.path);
  return { staged: [...staged].sort(byPath), unstaged: [...unstaged].sort(byPath) };
}

/** The parent directory of a project-relative path (`""` at the root). */
export function parentPath(path: string): string {
  const index = path.lastIndexOf('/');
  return index < 0 ? '' : path.slice(0, index);
}

/** Added/removed line counts for one file diff. */
export interface LineCount {
  additions: number;
  deletions: number;
}

/** Count added and removed lines across hunks. */
export function countLines(file: GitFileDiff): LineCount {
  const count: LineCount = { additions: 0, deletions: 0 };
  for (const hunk of file.hunks) {
    for (const line of hunk.lines) {
      if (line.kind === 'add') {
        count.additions += 1;
      } else if (line.kind === 'del') {
        count.deletions += 1;
      }
    }
  }
  return count;
}

/** Total added/removed lines across files. */
export function diffStats(files: readonly GitFileDiff[]): LineCount {
  const total: LineCount = { additions: 0, deletions: 0 };
  for (const file of files) {
    const count = countLines(file);
    total.additions += count.additions;
    total.deletions += count.deletions;
  }
  return total;
}

/** Which line an entry corresponds to in the diff list. */
export type DiffRowKind = 'context' | 'add' | 'del';

/** One rendered line of a diff. */
export interface DiffRow {
  key: string;
  kind: DiffRowKind;
  text: string;
}

const PREFIX: Record<DiffRowKind, string> = { context: ' ', add: '+', del: '-' };

/** Flatten a file's hunks into display rows (`+`/`-` prefix included). */
export function fileDiffRows(file: GitFileDiff): DiffRow[] {
  const rows: DiffRow[] = [];
  file.hunks.forEach((hunk, hunkIndex) => {
    hunk.lines.forEach((line: GitDiffLine, lineIndex: number) => {
      rows.push({
        key: `${file.path}:${hunkIndex}:${lineIndex}`,
        kind: line.kind,
        text: `${PREFIX[line.kind]}${line.content}`,
      });
    });
  });
  return rows;
}

/** A commit timestamp (seconds) as a locale date-time. */
export function commitTime(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString();
}
