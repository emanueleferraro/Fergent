/**
 * Source-control state (UI.md §8.4, task 3.10): status, branches, conflicts,
 * log, diffs, and blame for the active project, kept live from git events.
 * Reads are best-effort: a project without a repository simply reports a clean
 * tree instead of an error banner.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type {
  GitBlameLine,
  GitBlameResult,
  GitBranch,
  GitBranchesResult,
  GitCommit,
  GitCommitInfo,
  GitCommitResult,
  GitConflict,
  GitConflictsResult,
  GitDiffResult,
  GitFileDiff,
  GitLogResult,
  GitRepoStatus,
  GitResolveParams,
  GitStatusResult,
} from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

/** Debounce for refresh events (a save can emit several). */
export const STATUS_REFRESH_MS = 300;

/** What the inline diff currently shows. */
export interface GitSelection {
  path: string | null;
  staged: boolean;
  /** When set, the diff belongs to a commit instead of the working tree. */
  commit: string | null;
}

export type ResolveStrategy = GitResolveParams['strategy'];

/** The source-control state and actions. */
export interface GitState {
  status: GitRepoStatus | null;
  branches: GitBranch[];
  conflicts: GitConflict[];
  log: GitCommitInfo[];
  diff: GitFileDiff[];
  selection: GitSelection | null;
  blame: GitBlameLine[];
  blamePath: string | null;
  loading: boolean;
  error: string | null;
  dismissError: () => void;
  refresh: () => void;
  selectFile: (path: string, staged: boolean) => void;
  selectCommit: (id: string) => void;
  clearSelection: () => void;
  loadBlame: (path: string) => void;
  clearBlame: () => void;
  stage: (paths: readonly string[]) => Promise<void>;
  unstage: (paths: readonly string[]) => Promise<void>;
  discard: (paths: readonly string[]) => Promise<void>;
  commit: (message: string) => Promise<GitCommit | null>;
  createBranch: (name: string, checkout: boolean) => Promise<void>;
  switchBranch: (name: string) => Promise<void>;
  resolve: (path: string, strategy: ResolveStrategy) => Promise<void>;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Read a value, or `null` when the call fails (best-effort reads). */
async function attempt<T>(call: () => Promise<T>): Promise<T | null> {
  try {
    return await call();
  } catch {
    return null;
  }
}

/** Status, branches, conflicts, log, diffs, and blame for a project. */
export function useGit(kernel: KernelClient, projectId: string | null, trusted: boolean): GitState {
  const [status, setStatus] = useState<GitRepoStatus | null>(null);
  const [branches, setBranches] = useState<GitBranch[]>([]);
  const [conflicts, setConflicts] = useState<GitConflict[]>([]);
  const [log, setLog] = useState<GitCommitInfo[]>([]);
  const [diff, setDiff] = useState<GitFileDiff[]>([]);
  const [selection, setSelection] = useState<GitSelection | null>(null);
  const [blame, setBlame] = useState<GitBlameLine[]>([]);
  const [blamePath, setBlamePath] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const selectionRef = useRef<GitSelection | null>(null);
  selectionRef.current = selection;

  // Status, branches, conflicts, and the recent log.
  useEffect(() => {
    setStatus(null);
    setBranches([]);
    setConflicts([]);
    setLog([]);
    setDiff([]);
    setSelection(null);
    setBlame([]);
    setBlamePath(null);
    setError(null);
    if (projectId === null || !trusted) {
      return;
    }
    let cancelled = false;
    setLoading(true);
    void (async () => {
      const [statusResult, branchesResult, conflictsResult, logResult] = await Promise.all([
        attempt(() => kernel.call<GitStatusResult>('git.status', { project_id: projectId })),
        attempt(() => kernel.call<GitBranchesResult>('git.branches', { project_id: projectId })),
        attempt(() => kernel.call<GitConflictsResult>('git.conflicts', { project_id: projectId })),
        attempt(() => kernel.call<GitLogResult>('git.log', { project_id: projectId, limit: 50 })),
      ]);
      if (cancelled) {
        return;
      }
      setStatus(statusResult?.status ?? null);
      setBranches(branchesResult?.branches ?? []);
      setConflicts(conflictsResult?.conflicts ?? []);
      setLog(Array.isArray(logResult?.commits) ? logResult.commits : []);
      setLoading(false);
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel, projectId, trusted, revision]);

  // Keep the panel live: saves and git operations both change the status.
  useEffect(() => {
    if (projectId === null || !trusted) {
      return;
    }
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const bump = (): void => {
      if (timer !== null) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => {
        timer = null;
        setRevision((current) => current + 1);
      }, STATUS_REFRESH_MS);
    };
    void kernel
      .onEvent((event) => {
        const payload = event.payload as Record<string, unknown> | null;
        if (typeof payload?.project_id === 'string' && payload.project_id !== projectId) {
          return;
        }
        switch (event.kind) {
          case 'git.status.changed':
          case 'git.commit.created':
          case 'git.conflict':
          case 'file.saved':
            bump();
            break;
          default:
            break;
        }
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
      if (timer !== null) {
        clearTimeout(timer);
      }
    };
  }, [kernel, projectId, trusted]);

  const loadDiff = useCallback(
    async (next: GitSelection) => {
      if (projectId === null || !trusted) {
        return;
      }
      setSelection(next);
      setError(null);
      const params: Record<string, unknown> = { project_id: projectId };
      if (next.path !== null) {
        params.path = next.path;
      }
      if (next.staged) {
        params.staged = true;
      }
      if (next.commit !== null) {
        params.commit = next.commit;
      }
      try {
        const result = await kernel.call<GitDiffResult>('git.diff', params);
        setDiff(Array.isArray(result.files) ? result.files : []);
      } catch (cause) {
        setDiff([]);
        setError(messageOf(cause));
      }
    },
    [kernel, projectId, trusted],
  );

  const clearSelection = useCallback(() => {
    setSelection(null);
    setDiff([]);
  }, []);

  const selectFile = useCallback(
    (path: string, staged: boolean) => {
      const current = selectionRef.current;
      if (current !== null && current.path === path && current.staged === staged) {
        clearSelection();
        return;
      }
      void loadDiff({ path, staged, commit: null });
    },
    [clearSelection, loadDiff],
  );

  const selectCommit = useCallback(
    (id: string) => {
      const current = selectionRef.current;
      if (current !== null && current.commit === id) {
        clearSelection();
        return;
      }
      void loadDiff({ path: null, staged: false, commit: id });
    },
    [clearSelection, loadDiff],
  );

  const mutate = useCallback(
    async (
      method: 'git.stage' | 'git.unstage' | 'git.discard',
      paths: readonly string[],
    ): Promise<void> => {
      if (projectId === null || !trusted || paths.length === 0) {
        return;
      }
      setError(null);
      try {
        await kernel.call(method, { project_id: projectId, paths: [...paths], actor: 'user' });
        setSelection(null);
        setDiff([]);
        setRevision((current) => current + 1);
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel, projectId, trusted],
  );

  const stage = useCallback((paths: readonly string[]) => mutate('git.stage', paths), [mutate]);
  const unstage = useCallback((paths: readonly string[]) => mutate('git.unstage', paths), [mutate]);
  const discard = useCallback((paths: readonly string[]) => mutate('git.discard', paths), [mutate]);

  const commit = useCallback(
    async (message: string): Promise<GitCommit | null> => {
      const trimmed = message.trim();
      if (projectId === null || !trusted || trimmed === '') {
        return null;
      }
      setError(null);
      try {
        const result = await kernel.call<GitCommitResult>('git.commit', {
          project_id: projectId,
          message: trimmed,
          actor: 'user',
        });
        setSelection(null);
        setDiff([]);
        setBlame([]);
        setBlamePath(null);
        setRevision((current) => current + 1);
        return result.commit;
      } catch (cause) {
        setError(messageOf(cause));
        return null;
      }
    },
    [kernel, projectId, trusted],
  );

  const createBranch = useCallback(
    async (name: string, checkout: boolean) => {
      const trimmed = name.trim();
      if (projectId === null || !trusted || trimmed === '') {
        return;
      }
      setError(null);
      try {
        await kernel.call('git.branch.create', {
          project_id: projectId,
          name: trimmed,
          checkout,
          actor: 'user',
        });
        setRevision((current) => current + 1);
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel, projectId, trusted],
  );

  const switchBranch = useCallback(
    async (name: string) => {
      if (projectId === null || !trusted || name === '') {
        return;
      }
      setError(null);
      try {
        await kernel.call('git.branch.switch', {
          project_id: projectId,
          name,
          actor: 'user',
        });
        setRevision((current) => current + 1);
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel, projectId, trusted],
  );

  const resolve = useCallback(
    async (path: string, strategy: ResolveStrategy) => {
      if (projectId === null || !trusted) {
        return;
      }
      setError(null);
      try {
        await kernel.call('git.resolve', {
          project_id: projectId,
          path,
          strategy,
          actor: 'user',
        });
        setRevision((current) => current + 1);
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel, projectId, trusted],
  );

  const loadBlame = useCallback(
    async (path: string) => {
      if (projectId === null || !trusted) {
        return;
      }
      setBlamePath(path);
      setError(null);
      try {
        const result = await kernel.call<GitBlameResult>('git.blame', {
          project_id: projectId,
          path,
        });
        setBlame(Array.isArray(result.lines) ? result.lines : []);
      } catch (cause) {
        setBlame([]);
        setError(messageOf(cause));
      }
    },
    [kernel, projectId, trusted],
  );

  const clearBlame = useCallback(() => {
    setBlamePath(null);
    setBlame([]);
  }, []);

  const refresh = useCallback(() => setRevision((current) => current + 1), []);
  const dismissError = useCallback(() => setError(null), []);

  return {
    status,
    branches,
    conflicts,
    log,
    diff,
    selection,
    blame,
    blamePath,
    loading,
    error,
    dismissError,
    refresh,
    selectFile,
    selectCommit,
    clearSelection,
    loadBlame,
    clearBlame,
    stage,
    unstage,
    discard,
    commit,
    createBranch,
    switchBranch,
    resolve,
  };
}
