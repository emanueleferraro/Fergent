/**
 * The pure document model behind the code editor (spec D16): open tabs, dirty
 * tracking, preview-vs-pinned semantics, and the read-only guards. All Monaco
 * interaction lives behind the adapter in `monacoAdapter.ts`, so this module is
 * plain data and fully unit-testable.
 */
import type { FileSnapshot } from '@fergents/protocol';

import { applyTextEdits, type TextEdit } from './textEdits';

/** Files larger than this open read-only (spec §2, default 5 MB). */
export const MAX_EDITABLE_BYTES = 5 * 1024 * 1024;

/** One file open in the code editor. */
export interface EditorDocument {
  /** Project-relative path. */
  path: string;
  /** Monaco language id reported by the kernel. */
  languageId: string;
  /** The buffer content, including unsaved edits. */
  content: string;
  /** The content as last read from or written to disk. */
  savedContent: string;
  /** The etag of `savedContent`, sent as `base_etag` on save (spec D2). */
  etag: string;
  /** Size on disk, in bytes. */
  size: number;
  /** The kernel refused to read the whole file. */
  truncated: boolean;
  /** The kernel classified the file as binary. */
  binary: boolean;
  /** The document may not be edited (binary, truncated, too large, untrusted). */
  readOnly: boolean;
  /** Preview tabs are replaced by the next preview open; pinned ones are not. */
  preview: boolean;
}

/** The editor's tab state. */
export interface EditorState {
  documents: EditorDocument[];
  activePath: string | null;
}

/** The empty editor state (no tabs). */
export const EMPTY_EDITOR_STATE: EditorState = { documents: [], activePath: null };

/** Whether a document has unsaved changes. */
export function isDirty(document: EditorDocument): boolean {
  return document.content !== document.savedContent;
}

/** Whether a snapshot must open read-only. */
export function isReadOnlySnapshot(snapshot: FileSnapshot): boolean {
  return snapshot.binary || snapshot.truncated || snapshot.size > MAX_EDITABLE_BYTES;
}

/** Build a document from a kernel snapshot. */
export function documentFromSnapshot(
  snapshot: FileSnapshot,
  preview: boolean,
  trusted = true,
): EditorDocument {
  return {
    path: snapshot.path,
    languageId: snapshot.language_id,
    content: snapshot.content,
    savedContent: snapshot.content,
    etag: snapshot.etag,
    size: snapshot.size,
    truncated: snapshot.truncated,
    binary: snapshot.binary,
    readOnly: !trusted || isReadOnlySnapshot(snapshot),
    preview,
  };
}

/** The active document, or `null`. */
export function activeDocument(state: EditorState): EditorDocument | null {
  if (state.activePath === null) {
    return null;
  }
  return state.documents.find((document) => document.path === state.activePath) ?? null;
}

/** The document for a path, or `null`. */
export function documentFor(state: EditorState, path: string): EditorDocument | null {
  return state.documents.find((document) => document.path === path) ?? null;
}

/** Editor actions. */
export type EditorAction =
  | { type: 'open'; snapshot: FileSnapshot; preview: boolean; trusted: boolean }
  | { type: 'select'; path: string }
  | { type: 'close'; path: string }
  | { type: 'change'; path: string; content: string }
  | { type: 'applyEdits'; path: string; edits: readonly TextEdit[] }
  | { type: 'saved'; snapshot: FileSnapshot }
  | { type: 'reload'; snapshot: FileSnapshot }
  | { type: 'reset' };

/** Path following `path` in `documents` when it closes, preferring the next tab. */
function neighbourPath(documents: readonly EditorDocument[], path: string): string | null {
  const index = documents.findIndex((document) => document.path === path);
  const next = documents[index + 1] ?? documents[index - 1];
  return next?.path ?? null;
}

/** Apply an editor action. */
export function editorReducer(state: EditorState, action: EditorAction): EditorState {
  switch (action.type) {
    case 'open': {
      const existing = documentFor(state, action.snapshot.path);
      if (existing !== null) {
        return {
          documents:
            existing.preview && !action.preview
              ? state.documents.map((document) =>
                  document.path === existing.path ? { ...document, preview: false } : document,
                )
              : state.documents,
          activePath: existing.path,
        };
      }
      const opened = documentFromSnapshot(action.snapshot, action.preview, action.trusted);
      const previewIndex = action.preview
        ? state.documents.findIndex((document) => document.preview)
        : -1;
      const documents =
        previewIndex >= 0
          ? state.documents.map((document, index) => (index === previewIndex ? opened : document))
          : [...state.documents, opened];
      return { documents, activePath: opened.path };
    }
    case 'select':
      return documentFor(state, action.path) === null
        ? state
        : { ...state, activePath: action.path };
    case 'close': {
      if (documentFor(state, action.path) === null) {
        return state;
      }
      return {
        documents: state.documents.filter((document) => document.path !== action.path),
        activePath:
          state.activePath === action.path
            ? neighbourPath(state.documents, action.path)
            : state.activePath,
      };
    }
    case 'change':
      return {
        ...state,
        documents: state.documents.map((document) =>
          document.path === action.path && !document.readOnly
            ? { ...document, content: action.content, preview: false }
            : document,
        ),
      };
    case 'applyEdits':
      return {
        ...state,
        documents: state.documents.map((document) =>
          document.path === action.path && !document.readOnly && action.edits.length > 0
            ? {
                ...document,
                content: applyTextEdits(document.content, action.edits),
                preview: false,
              }
            : document,
        ),
      };
    case 'saved':
      return {
        ...state,
        documents: state.documents.map((document) =>
          document.path === action.snapshot.path
            ? {
                ...document,
                content: action.snapshot.content,
                savedContent: action.snapshot.content,
                etag: action.snapshot.etag,
                size: action.snapshot.size,
                truncated: action.snapshot.truncated,
                binary: action.snapshot.binary,
                readOnly: document.readOnly,
              }
            : document,
        ),
      };
    case 'reload': {
      const existing = documentFor(state, action.snapshot.path);
      if (existing === null) {
        return state;
      }
      const reloaded = documentFromSnapshot(action.snapshot, existing.preview);
      return {
        ...state,
        documents: state.documents.map((document) =>
          document.path === existing.path ? { ...reloaded, readOnly: existing.readOnly } : document,
        ),
      };
    }
    case 'reset':
      return EMPTY_EDITOR_STATE;
  }
}

/**
 * Whether a kernel error is the `file_conflict` refusal of `file.write`
 * (spec D2). The webview sees the JSON-RPC error as a string such as
 * `rpc error -32008: file conflict: src/main.ts`.
 */
export function isFileConflict(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes('file conflict') || message.includes('-32008');
}
