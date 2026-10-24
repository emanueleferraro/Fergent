/**
 * The editor-facing LSP operations (spec §3, task 3.8): hover text, definition
 * targets, rename edits and quick fixes. Positions arrive one-based from
 * Monaco and are converted to the LSP's zero-based convention here; every
 * failure is soft, because no server is a normal state.
 */
import { useMemo } from 'react';
import type { LspRequestResult } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import {
  codeActionsFrom,
  definitionTarget,
  hoverMarkdown,
  workspaceEditChanges,
  type DefinitionTarget,
  type FileTextChanges,
  type LspCodeAction,
} from './lspResult';

/** The LSP operations the editor can call. Positions are one-based. */
export interface LspEditorActions {
  hover: (path: string, line: number, column: number) => Promise<string | null>;
  definition: (path: string, line: number, column: number) => Promise<DefinitionTarget | null>;
  rename: (
    path: string,
    line: number,
    column: number,
    newName: string,
  ) => Promise<FileTextChanges[]>;
  codeActions: (path: string, line: number, column: number) => Promise<LspCodeAction[]>;
}

/** One-based position as the LSP `Position` shape (zero-based). */
function position(line: number, column: number): { line: number; character: number } {
  return { line: Math.max(0, line - 1), character: Math.max(0, column - 1) };
}

/** The LSP operations for one project (no-ops when untrusted or closed). */
export function useLspEditor(
  kernel: KernelClient,
  projectId: string | null,
  projectRoot: string,
  trusted: boolean,
): LspEditorActions {
  return useMemo(() => {
    const request = async (method: string, params: Record<string, unknown>): Promise<unknown> => {
      if (projectId === null || !trusted) {
        return null;
      }
      try {
        const result = await kernel.call<LspRequestResult>(method, {
          project_id: projectId,
          ...params,
        });
        return result.result;
      } catch {
        return null;
      }
    };

    return {
      async hover(path, line, column) {
        return hoverMarkdown(await request('lsp.hover', { path, ...position(line, column) }));
      },

      async definition(path, line, column) {
        return definitionTarget(
          await request('lsp.definition', { path, ...position(line, column) }),
          projectRoot,
        );
      },

      async rename(path, line, column, newName) {
        return workspaceEditChanges(
          await request('lsp.rename', { path, ...position(line, column), new_name: newName }),
          projectRoot,
        );
      },

      async codeActions(path, line, column) {
        const start = position(line, column);
        return codeActionsFrom(
          await request('lsp.code_action', {
            path,
            range: { start, end: start },
            diagnostics: [],
          }),
          projectRoot,
        );
      },
    };
  }, [kernel, projectId, projectRoot, trusted]);
}
