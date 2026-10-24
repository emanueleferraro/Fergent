/**
 * Parsing of raw LSP results (spec §3): the kernel returns the server's JSON
 * verbatim, so hover contents, definition targets, workspace edits and code
 * actions are validated and converted here. Pure functions; unit-tested.
 */
import { isTextEdit, type FileEdits } from '../editor/textEdits';

/** A one-based target inside the project. */
export interface DefinitionTarget {
  path: string;
  line: number;
  column: number;
}

/** The edits for one file. */
export type FileTextChanges = FileEdits;

/** One code action offered by the server. */
export interface LspCodeAction {
  title: string;
  edits: FileTextChanges[];
}

/** One-based position from an LSP `Position`. */
function toPosition(value: unknown): { line: number; column: number } | null {
  if (typeof value !== 'object' || value === null) {
    return null;
  }
  const candidate = value as Record<string, unknown>;
  const { line, character } = candidate;
  if (typeof line !== 'number' || typeof character !== 'number') {
    return null;
  }
  return { line: line + 1, column: character + 1 };
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null;
}

/** Decode the path back out of a `modelUri` (`inmemory://fergents/<encoded>`). */
export { pathFromModelUri } from '../editor/monacoAdapter';

/** Normalise a filesystem path for comparison (slashes, trailing slash). */
function pathKey(path: string): string {
  const slashed = path.replace(/\\/g, '/').replace(/\/+$/, '');
  return /^[A-Za-z]:/.test(slashed) ? slashed.toLowerCase() : slashed;
}

/** Turn a `file://` URI into a project-relative path, or `null` when outside. */
export function relativePathFromUri(uri: string, projectRoot: string): string | null {
  if (!uri.startsWith('file://')) {
    return null;
  }
  let file: string;
  try {
    file = decodeURIComponent(uri.slice('file://'.length));
  } catch {
    return null;
  }
  // `file:///C:/x` keeps the drive letter; POSIX `file:///home/...` keeps `/`.
  if (/^\/[A-Za-z]:/.test(file)) {
    file = file.slice(1);
  }
  const fileKey = pathKey(file);
  const rootKey = pathKey(projectRoot);
  if (fileKey === rootKey) {
    return '';
  }
  const prefix = `${rootKey}/`;
  if (!fileKey.startsWith(prefix)) {
    return null;
  }
  // Slice the original case-preserving path by the same length.
  return file.replace(/\\/g, '/').slice(prefix.length);
}

/** The URI of a Location (`uri`) or a LocationLink (`targetUri`). */
function locationUri(location: Record<string, unknown>): string | null {
  if (typeof location.uri === 'string') {
    return location.uri;
  }
  return typeof location.targetUri === 'string' ? location.targetUri : null;
}

function locationPosition(
  location: Record<string, unknown>,
): { line: number; column: number } | null {
  const range =
    asRecord(location.targetSelectionRange) ??
    asRecord(location.targetRange) ??
    asRecord(location.range);
  return toPosition(range?.start);
}

/** The first definition target from `textDocument/definition`, if in-project. */
export function definitionTarget(result: unknown, projectRoot: string): DefinitionTarget | null {
  const candidate = Array.isArray(result) ? result[0] : result;
  const location = asRecord(candidate);
  if (location === null) {
    return null;
  }
  const uri = locationUri(location);
  const position = locationPosition(location);
  if (uri === null || position === null) {
    return null;
  }
  const path = relativePathFromUri(uri, projectRoot);
  if (path === null || path.length === 0) {
    return null;
  }
  return { path, line: position.line, column: position.column };
}

function hoverText(contents: unknown): string {
  if (typeof contents === 'string') {
    return contents;
  }
  if (Array.isArray(contents)) {
    return contents
      .map(hoverText)
      .filter((part) => part.length > 0)
      .join('\n\n');
  }
  const record = asRecord(contents);
  if (record === null || typeof record.value !== 'string') {
    return '';
  }
  return typeof record.language === 'string'
    ? `\`\`\`${record.language}\n${record.value}\n\`\`\``
    : record.value;
}

/** Flatten hover contents (MarkupContent, MarkedString, or arrays) to markdown. */
export function hoverMarkdown(result: unknown): string | null {
  const hover = asRecord(result);
  const contents = hover !== null && 'contents' in hover ? hover.contents : result;
  const text = hoverText(contents).trim();
  return text.length > 0 ? text : null;
}

/** Workspace-edit changes (`changes` or `documentChanges`) for in-project files. */
export function workspaceEditChanges(result: unknown, projectRoot: string): FileTextChanges[] {
  const edit = asRecord(result);
  if (edit === null) {
    return [];
  }
  const changes: FileTextChanges[] = [];
  const push = (uri: unknown, edits: unknown): void => {
    if (typeof uri !== 'string' || !Array.isArray(edits)) {
      return;
    }
    const path = relativePathFromUri(uri, projectRoot);
    if (path === null || path.length === 0) {
      return;
    }
    const valid = edits.filter(isTextEdit);
    if (valid.length > 0) {
      changes.push({ path, edits: valid });
    }
  };

  const byUri = asRecord(edit.changes);
  if (byUri !== null) {
    for (const [uri, edits] of Object.entries(byUri)) {
      push(uri, edits);
    }
  }
  if (Array.isArray(edit.documentChanges)) {
    for (const change of edit.documentChanges) {
      const record = asRecord(change);
      const document = asRecord(record?.textDocument);
      push(document?.uri, record?.edits);
    }
  }
  return changes;
}

/** Code actions with their inline edits (command-only actions have none). */
export function codeActionsFrom(result: unknown, projectRoot: string): LspCodeAction[] {
  if (!Array.isArray(result)) {
    return [];
  }
  const actions: LspCodeAction[] = [];
  for (const candidate of result) {
    const action = asRecord(candidate);
    if (action === null || typeof action.title !== 'string') {
      continue;
    }
    actions.push({
      title: action.title,
      edits: workspaceEditChanges(action.edit, projectRoot),
    });
  }
  return actions;
}
