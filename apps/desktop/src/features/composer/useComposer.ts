/**
 * The Composer controller (task 3.13): one proposal at a time, produced by
 * `composer.*` in the kernel. The kernel is authoritative; this hook only
 * mirrors the run and forwards hunk ids on apply.
 */
import { useCallback, useState } from 'react';
import type { ComposerAppliedFile, ComposerRun } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';
import { buildPreview, type PreviewFile } from '../diff/model';

/** What the composer knows about the editor when the user asks for a change. */
export interface ComposerContext {
  /** Files to read as context, in addition to `activePath`. */
  paths: readonly string[];
  /** The file the user is looking at. */
  activePath: string | null;
  /** The selected text, when the request came from a selection. */
  selection: string | null;
}

/** The Composer's state and actions. */
export interface ComposerController {
  run: ComposerRun | null;
  files: PreviewFile[];
  busy: boolean;
  error: string | null;
  start(prompt: string, context: ComposerContext): Promise<void>;
  apply(accepted: readonly string[]): Promise<ComposerAppliedFile[]>;
  reject(): Promise<void>;
  reset(): void;
  dismissError(): void;
}

/** Turn a run into the diff preview model, preserving the server text. */
export function runFiles(run: ComposerRun | null): PreviewFile[] {
  if (run === null) {
    return [];
  }
  return buildPreview(
    run.files.map((file) => ({
      path: file.path,
      languageId: file.language_id,
      original: file.original,
      proposed: file.proposed,
      binary: file.binary,
      hunks: file.hunks,
    })),
  );
}

/** Drive `composer.start`, `composer.apply`, and `composer.reject`. */
export function useComposer(
  kernel: KernelClient,
  projectId: string | null,
  trusted: boolean,
): ComposerController {
  const [run, setRun] = useState<ComposerRun | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const start = useCallback(
    async (prompt: string, context: ComposerContext): Promise<void> => {
      const trimmed = prompt.trim();
      if (projectId === null || !trusted || trimmed === '' || busy) {
        return;
      }
      setBusy(true);
      setError(null);
      try {
        const result = await kernel.call<{ run: ComposerRun }>('composer.start', {
          project_id: projectId,
          prompt: trimmed,
          paths: [...context.paths],
          active_path: context.activePath,
          selection: context.selection,
        });
        setRun(result.run);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setBusy(false);
      }
    },
    [busy, kernel, projectId, trusted],
  );

  const apply = useCallback(
    async (accepted: readonly string[]): Promise<ComposerAppliedFile[]> => {
      if (run === null || busy) {
        return [];
      }
      setBusy(true);
      setError(null);
      try {
        const result = await kernel.call<{
          run: ComposerRun;
          applied: ComposerAppliedFile[];
        }>('composer.apply', { run_id: run.id, accepted: [...accepted] });
        setRun(result.run);
        return result.applied;
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
        return [];
      } finally {
        setBusy(false);
      }
    },
    [busy, kernel, run],
  );

  const reject = useCallback(async (): Promise<void> => {
    if (run === null || busy) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await kernel.call<{ run: ComposerRun }>('composer.reject', { run_id: run.id });
      setRun(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }, [busy, kernel, run]);

  const reset = useCallback(() => {
    setRun(null);
    setError(null);
  }, []);

  const dismissError = useCallback(() => setError(null), []);

  return {
    run,
    files: runFiles(run),
    busy,
    error,
    start,
    apply,
    reject,
    reset,
    dismissError,
  };
}
