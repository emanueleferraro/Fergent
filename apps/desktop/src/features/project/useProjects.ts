/**
 * Project state for the IDE: the open workspaces, the active one, its file
 * index, and its git badges.
 *
 * The kernel is the source of truth for all of it. Reads are best-effort:
 * a project without a repository (or with `capability.git` off) simply has
 * no badges, and an unreadable index renders an empty tree rather than an
 * error banner.
 */
import { useCallback, useEffect, useState } from 'react';
import type {
  GitChangeKind,
  GitStatusResult,
  Project,
  ProjectFilesResult,
  ProjectListResult,
  ProjectOpenResult,
  ProjectTrustResult,
} from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import { statusMap } from './model';

/** Upper bound on the files the explorer asks for (the kernel caps at 500). */
export const MAX_PROJECT_FILES = 500;

/** Debounce for badge refreshes (a save can emit several events). */
export const BADGE_REFRESH_MS = 300;

/** The project state exposed to the IDE surfaces. */
export interface ProjectsController {
  /** Every known project, most recently opened first. */
  projects: Project[];
  /** The active project, if any. */
  active: Project | null;
  /** The active project's indexed files. */
  files: string[];
  /** Changed paths of the active project, keyed by path. */
  statuses: Record<string, GitChangeKind>;
  /** The last user-facing error (open failures). */
  error: string | null;
  /** Whether the initial project list is still loading. */
  loading: boolean;
  /** Open a folder (idempotent for an already-known path). */
  open: (path: string) => Promise<void>;
  /** Switch to a project. */
  select: (id: string) => void;
  /** Grant or revoke trust for the active project. */
  trust: (trusted: boolean) => Promise<void>;
  /** Re-read the file index and git status. */
  refresh: () => void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Load and track the project list, index, and git status. */
export function useProjects(kernel: KernelClient): ProjectsController {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [files, setFiles] = useState<string[]>([]);
  const [statuses, setStatuses] = useState<Record<string, GitChangeKind>>({});
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);

  const loadList = useCallback(async (): Promise<Project[]> => {
    try {
      const result = await kernel.call<ProjectListResult>('project.list');
      const list = Array.isArray(result?.projects) ? result.projects : [];
      setProjects(list);
      setActiveId((current) =>
        current !== null && list.some((project) => project.id === current)
          ? current
          : (list[0]?.id ?? null),
      );
      return list;
    } catch (cause) {
      setError(messageOf(cause));
      return [];
    } finally {
      setLoading(false);
    }
  }, [kernel]);

  useEffect(() => {
    void loadList();
  }, [loadList]);

  useEffect(() => {
    if (activeId === null) {
      setFiles([]);
      setStatuses({});
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const result = await kernel.call<ProjectFilesResult>('project.files', {
          project_id: activeId,
          limit: MAX_PROJECT_FILES,
        });
        if (!cancelled) {
          setFiles(Array.isArray(result?.files) ? result.files : []);
        }
      } catch {
        if (!cancelled) {
          setFiles([]);
        }
      }
      try {
        const result = await kernel.call<GitStatusResult>('git.status', { project_id: activeId });
        if (!cancelled) {
          // The kernel wraps the status: `git.status` → `{ status }`.
          setStatuses(statusMap(result.status));
        }
      } catch {
        if (!cancelled) {
          setStatuses({});
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel, activeId, revision]);

  // Saves and git operations change the tree badges; refresh after the burst.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | null = null;
    void kernel
      .onEvent((event) => {
        if (
          event.kind !== 'file.saved' &&
          event.kind !== 'git.status.changed' &&
          event.kind !== 'git.commit.created'
        ) {
          return;
        }
        if (timer !== null) {
          clearTimeout(timer);
        }
        timer = setTimeout(() => {
          timer = null;
          setRevision((current) => current + 1);
        }, BADGE_REFRESH_MS);
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
  }, [kernel]);

  const open = useCallback(
    async (path: string) => {
      const trimmed = path.trim();
      if (trimmed === '') {
        return;
      }
      try {
        const result = await kernel.call<ProjectOpenResult>('project.open', { path: trimmed });
        await loadList();
        setActiveId(result.project.id);
        setError(null);
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel, loadList],
  );

  const select = useCallback((id: string) => {
    setActiveId(id);
  }, []);

  const trust = useCallback(
    async (trusted: boolean) => {
      if (activeId === null) {
        return;
      }
      try {
        const result = await kernel.call<ProjectTrustResult>('project.trust', {
          project_id: activeId,
          trusted,
        });
        setProjects((current) =>
          current.map((project) => (project.id === result.project.id ? result.project : project)),
        );
        setError(null);
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel, activeId],
  );

  const refresh = useCallback(() => {
    setRevision((current) => current + 1);
  }, []);

  const active = projects.find((project) => project.id === activeId) ?? null;

  return { projects, active, files, statuses, error, loading, open, select, trust, refresh };
}
