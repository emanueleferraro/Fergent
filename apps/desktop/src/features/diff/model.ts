/**
 * The diff-preview model (UI.md §8.7, task 3.11): a multi-file proposal is a
 * base text plus unified hunks. The model turns hunks into stable, addressable
 * units so the UI can accept or reject them individually and compute exactly
 * what would be written.
 */
import type { GitDiffHunk, GitDiffLine } from '@fergents/protocol';

/** One accepted or rejected hunk, ready for rendering. */
export interface PreviewHunk {
  /** Stable across renders: `<path>:<index>`. */
  id: string;
  header: string;
  oldStart: number;
  oldLines: number;
  additions: number;
  deletions: number;
  lines: readonly GitDiffLine[];
}

/** One file in a proposal, with its base and fully-applied text. */
export interface PreviewFile {
  path: string;
  /** Monaco language id; `plaintext` when unknown. */
  languageId: string;
  /** The text the proposal was computed against. */
  original: string;
  /** The text with every hunk applied (what Monaco renders as "modified"). */
  proposed: string;
  binary: boolean;
  hunks: readonly PreviewHunk[];
  additions: number;
  deletions: number;
}

/** A file as it should be written after partial approval. */
export interface AppliedFile {
  path: string;
  content: string;
  /** The accepted hunk ids, for audit and the apply payload. */
  hunks: string[];
}

/** Added/removed line counts. */
export interface PreviewStats {
  additions: number;
  deletions: number;
}

/** What a producer (agent edit, inline action, Composer) hands the preview. */
export interface PreviewFileInput {
  path: string;
  languageId?: string | undefined;
  /** The file content on disk before the proposal. */
  original: string;
  binary?: boolean | undefined;
  hunks: readonly GitDiffHunk[];
}

/** The stable id of a hunk within a file. */
export function hunkId(path: string, index: number): string {
  return `${path}:${index}`;
}

/** Count the added and removed lines of one hunk. */
export function hunkStats(hunk: readonly GitDiffLine[]): PreviewStats {
  const stats: PreviewStats = { additions: 0, deletions: 0 };
  for (const line of hunk) {
    if (line.kind === 'add') {
      stats.additions += 1;
    } else if (line.kind === 'del') {
      stats.deletions += 1;
    }
  }
  return stats;
}

/** Split text into lines, keeping a trailing empty line for a final newline. */
function splitContent(text: string): string[] {
  return text === '' ? [] : text.split('\n');
}

/** Build one preview file from its base text and unified hunks. */
export function buildPreviewFile(input: PreviewFileInput): PreviewFile {
  const hunks: PreviewHunk[] = input.hunks.map((hunk, index) => {
    const stats = hunkStats(hunk.lines);
    return {
      id: hunkId(input.path, index),
      header: hunk.header,
      oldStart: hunk.old_start,
      oldLines: hunk.old_lines,
      additions: stats.additions,
      deletions: stats.deletions,
      lines: hunk.lines,
    };
  });
  const all = new Set(hunks.map((hunk) => hunk.id));
  return {
    path: input.path,
    languageId: input.languageId ?? 'plaintext',
    original: input.original,
    proposed: applyHunks(input.original, hunks, all),
    binary: input.binary === true,
    hunks,
    additions: hunks.reduce((total, hunk) => total + hunk.additions, 0),
    deletions: hunks.reduce((total, hunk) => total + hunk.deletions, 0),
  };
}

/** Build a multi-file preview. */
export function buildPreview(inputs: readonly PreviewFileInput[]): PreviewFile[] {
  return inputs.map(buildPreviewFile);
}

/**
 * Apply the accepted hunks to `original`.
 *
 * Hunks are applied in `oldStart` order; rejected hunks keep the original
 * lines. The result is exactly the text a call to `file.write` must receive.
 */
export function applyHunks(
  original: string,
  hunks: readonly PreviewHunk[],
  accepted: ReadonlySet<string>,
): string {
  const lines = splitContent(original);
  const ordered = [...hunks].sort((left, right) => left.oldStart - right.oldStart);
  const output: string[] = [];
  let cursor = 0;
  for (const hunk of ordered) {
    // `-5,0` inserts after line 5; `-1,N` replaces N lines starting at 0.
    const start = Math.max(0, hunk.oldLines === 0 ? hunk.oldStart : hunk.oldStart - 1);
    if (start < cursor) {
      continue;
    }
    if (start > cursor) {
      output.push(...lines.slice(cursor, start));
    }
    if (accepted.has(hunk.id)) {
      output.push(...hunk.lines.filter((line) => line.kind !== 'del').map((line) => line.content));
    } else {
      output.push(...lines.slice(start, start + hunk.oldLines));
    }
    cursor = start + hunk.oldLines;
  }
  output.push(...lines.slice(cursor));
  return output.join('\n');
}

/** Total stats, optionally restricted to accepted hunks. */
export function previewStats(
  files: readonly PreviewFile[],
  accepted?: ReadonlySet<string>,
): PreviewStats {
  const stats: PreviewStats = { additions: 0, deletions: 0 };
  for (const file of files) {
    for (const hunk of file.hunks) {
      if (accepted !== undefined && !accepted.has(hunk.id)) {
        continue;
      }
      stats.additions += hunk.additions;
      stats.deletions += hunk.deletions;
    }
  }
  return stats;
}

/** Every hunk id in a preview. */
export function allHunkIds(files: readonly PreviewFile[]): Set<string> {
  const ids = new Set<string>();
  for (const file of files) {
    for (const hunk of file.hunks) {
      ids.add(hunk.id);
    }
  }
  return ids;
}

/** The files and contents an apply would write; files with no hunks are omitted. */
export function applyPreview(
  files: readonly PreviewFile[],
  accepted: ReadonlySet<string>,
): AppliedFile[] {
  const applied: AppliedFile[] = [];
  for (const file of files) {
    const ids = file.hunks.filter((hunk) => accepted.has(hunk.id));
    if (ids.length === 0 || file.binary) {
      continue;
    }
    applied.push({
      path: file.path,
      content: applyHunks(file.original, file.hunks, accepted),
      hunks: ids.map((hunk) => hunk.id),
    });
  }
  return applied;
}

/** How many hunks of one file are accepted. */
export function acceptedCount(file: PreviewFile, accepted: ReadonlySet<string>): number {
  return file.hunks.filter((hunk) => accepted.has(hunk.id)).length;
}
