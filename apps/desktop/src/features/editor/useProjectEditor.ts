/**
 * The code editor controller (spec §2, decisions D2/D16): opens files through
 * `file.read`, tracks dirty state, saves through `file.write` with `base_etag`,
 * and turns a `file_conflict` refusal or a `file.changed` event into a banner
 * with reload / keep-mine actions. No Monaco here, so it is tested directly.
 */
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import type { FileReadResult, FileWriteResult, LspRequestResult } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';
import {
  activeDocument,
  documentFor,
  editorReducer,
  EMPTY_EDITOR_STATE,
  isFileConflict,
  type EditorDocument,
  type EditorState,
} from './documents';
import { applyTextEdits, isTextEdit } from './textEdits';

/** A save that could not proceed because the file changed underneath us. */
export interface EditorConflict {
  path: string;
  message: string;
  /** The change was detected from a `file.changed` event, not a failed save. */
  external: boolean;
}

/** Options for `useProjectEditor`. */
export interface EditorOptions {
  /** Ask the language server to format before writing (default off). */
  formatOnSave?: boolean | undefined;
  /** Untrusted projects open read-only (spec D3). */
  trusted?: boolean | undefined;
}

/** The controller returned by `useProjectEditor`. */
export interface EditorController {
  state: EditorState;
  documents: EditorDocument[];
  activePath: string | null;
  /** The active document, or `null`. */
  active: EditorDocument | null;
  busy: boolean;
  error: string | null;
  conflict: EditorConflict | null;
  /** Open a file in a preview tab (or activate/pin an open one). */
  open: (path: string) => Promise<void>;
  select: (path: string) => void;
  close: (path: string) => void;
  change: (path: string, content: string) => void;
  save: (path?: string) => Promise<void>;
  reload: (path?: string) => Promise<void>;
  keepMine: (path?: string) => Promise<void>;
  dismissConflict: () => void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Read a file through the kernel. */
async function readFile(
  kernel: KernelClient,
  projectId: string,
  path: string,
): Promise<FileReadResult> {
  return kernel.call<FileReadResult>('file.read', { project_id: projectId, path });
}

/** Write a file through the kernel, optionally without an etag check. */
async function writeFile(
  kernel: KernelClient,
  projectId: string,
  path: string,
  content: string,
  baseEtag: string | null,
): Promise<FileWriteResult> {
  return kernel.call<FileWriteResult>('file.write', {
    project_id: projectId,
    path,
    content,
    base_etag: baseEtag,
  });
}

/** Apply the language server's formatting edits, best-effort. */
async function formatText(
  kernel: KernelClient,
  projectId: string,
  path: string,
  content: string,
): Promise<string> {
  try {
    const result = await kernel.call<LspRequestResult>('lsp.format', {
      project_id: projectId,
      path,
      tab_size: 2,
      insert_spaces: true,
    });
    const edits = Array.isArray(result.result) ? result.result.filter(isTextEdit) : [];
    return edits.length > 0 ? applyTextEdits(content, edits) : content;
  } catch {
    // No server, or the server failed: save unformatted rather than block.
    return content;
  }
}

/** The code editor controller for one project. */
export function useProjectEditor(
  kernel: KernelClient,
  projectId: string | null,
  options: EditorOptions = {},
): EditorController {
  const [state, dispatch] = useReducer(editorReducer, EMPTY_EDITOR_STATE);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<EditorConflict | null>(null);
  const stateRef = useRef(state);
  stateRef.current = state;
  const formatOnSave = options.formatOnSave ?? false;
  const trusted = options.trusted ?? true;

  // Paths are project-relative, so switching projects drops every tab.
  useEffect(() => {
    dispatch({ type: 'reset' });
    setError(null);
    setConflict(null);
  }, [projectId]);

  // A write by the agent, git, or another editor must never be lost silently.
  useEffect(() => {
    if (projectId === null) {
      return;
    }
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void kernel
      .onEvent((event) => {
        if (event.kind !== 'file.changed') {
          return;
        }
        const payload = event.payload as { project_id?: unknown; path?: unknown } | null;
        if (
          payload === null ||
          payload.project_id !== projectId ||
          typeof payload.path !== 'string'
        ) {
          return;
        }
        if (documentFor(stateRef.current, payload.path) === null) {
          return;
        }
        setConflict({
          path: payload.path,
          message: '',
          external: true,
        });
      })
      .then((off) => {
        if (cancelled) {
          off();
        } else {
          unlisten = off;
        }
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [kernel, projectId]);

  const open = useCallback(
    async (path: string) => {
      if (projectId === null) {
        return;
      }
      if (documentFor(stateRef.current, path) !== null) {
        dispatch({ type: 'select', path });
        return;
      }
      setBusy(true);
      setError(null);
      try {
        const result = await readFile(kernel, projectId, path);
        dispatch({ type: 'open', snapshot: result.file, preview: true, trusted });
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setBusy(false);
      }
    },
    [kernel, projectId, trusted],
  );

  const select = useCallback((path: string) => {
    dispatch({ type: 'select', path });
  }, []);

  const close = useCallback((path: string) => {
    dispatch({ type: 'close', path });
    setConflict((current) => (current?.path === path ? null : current));
  }, []);

  const change = useCallback((path: string, content: string) => {
    dispatch({ type: 'change', path, content });
  }, []);

  const save = useCallback(
    async (path?: string) => {
      const target = path ?? stateRef.current.activePath;
      if (projectId === null || target === null) {
        return;
      }
      const document = documentFor(stateRef.current, target);
      if (document === null || document.readOnly) {
        return;
      }
      setBusy(true);
      setError(null);
      try {
        const content = formatOnSave
          ? await formatText(kernel, projectId, target, document.content)
          : document.content;
        if (content !== document.content) {
          dispatch({ type: 'change', path: target, content });
        }
        const result = await writeFile(kernel, projectId, target, content, document.etag);
        dispatch({ type: 'saved', snapshot: result.file });
        setConflict(null);
      } catch (cause) {
        if (isFileConflict(cause)) {
          setConflict({ path: target, message: messageOf(cause), external: false });
        } else {
          setError(messageOf(cause));
        }
      } finally {
        setBusy(false);
      }
    },
    [formatOnSave, kernel, projectId],
  );

  const reload = useCallback(
    async (path?: string) => {
      const target = path ?? conflict?.path ?? stateRef.current.activePath;
      if (projectId === null || target === null) {
        return;
      }
      setBusy(true);
      setError(null);
      try {
        const result = await readFile(kernel, projectId, target);
        dispatch({ type: 'reload', snapshot: result.file });
        setConflict(null);
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setBusy(false);
      }
    },
    [conflict?.path, kernel, projectId],
  );

  const keepMine = useCallback(
    async (path?: string) => {
      const target = path ?? conflict?.path ?? stateRef.current.activePath;
      if (projectId === null || target === null) {
        return;
      }
      const document = documentFor(stateRef.current, target);
      if (document === null) {
        return;
      }
      setBusy(true);
      setError(null);
      try {
        // `base_etag: null` is the explicit, user-approved overwrite (D2).
        const result = await writeFile(kernel, projectId, target, document.content, null);
        dispatch({ type: 'saved', snapshot: result.file });
        setConflict(null);
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setBusy(false);
      }
    },
    [conflict?.path, kernel, projectId],
  );

  const dismissConflict = useCallback(() => setConflict(null), []);

  return {
    state,
    documents: state.documents,
    activePath: state.activePath,
    active: activeDocument(state),
    busy,
    error,
    conflict,
    open,
    select,
    close,
    change,
    save,
    reload,
    keepMine,
    dismissConflict,
  };
}
