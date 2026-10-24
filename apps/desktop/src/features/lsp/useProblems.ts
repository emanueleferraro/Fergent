/**
 * Diagnostics for the current project: loaded once from the kernel, then kept
 * live from `lsp.diagnostics` events. Diagnostics are only requested for
 * trusted projects (spec decision D3).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { LspDiagnosticsResult, LspDiagnostic } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import {
  countProblems,
  flattenDiagnostics,
  groupProblems,
  indexDiagnostics,
  mergeDiagnostics,
  type ProblemGroup,
  type ProblemSeverity,
} from './problems';

/** Diagnostics for a project, grouped and live-updated. */
export interface DiagnosticsState {
  groups: ProblemGroup[];
  counts: Record<ProblemSeverity, number>;
  /** Raw diagnostics for one path (used as quick-fix context). */
  byPath: (path: string) => LspDiagnostic[];
  refresh: () => void;
}

/** Diagnostics for a project, grouped and live-updated. */
export function useProblems(
  kernel: KernelClient,
  projectId: string | null,
  trusted: boolean,
): DiagnosticsState {
  const [index, setIndex] = useState<Record<string, LspDiagnostic[]>>({});
  const [revision, setRevision] = useState(0);
  const indexRef = useRef(index);
  indexRef.current = index;

  useEffect(() => {
    setIndex({});
    if (projectId === null || !trusted) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const result = await kernel.call<LspDiagnosticsResult>('lsp.diagnostics.list', {
          project_id: projectId,
        });
        if (!cancelled) {
          setIndex(indexDiagnostics(result.diagnostics));
        }
      } catch {
        if (!cancelled) {
          setIndex({});
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel, projectId, trusted, revision]);

  useEffect(() => {
    if (projectId === null || !trusted) {
      return;
    }
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void kernel
      .onEvent((event) => {
        if (event.kind !== 'lsp.diagnostics') {
          return;
        }
        const payload = event.payload as {
          diagnostics?: unknown;
          path?: unknown;
          project_id?: unknown;
        } | null;
        if (payload === null || payload.project_id !== projectId) {
          return;
        }
        if (typeof payload.path !== 'string' || !Array.isArray(payload.diagnostics)) {
          return;
        }
        setIndex((current) =>
          mergeDiagnostics(current, payload.path as string, payload.diagnostics as LspDiagnostic[]),
        );
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
  }, [kernel, projectId, trusted]);

  const groups = useMemo(() => groupProblems(flattenDiagnostics(index)), [index]);
  const counts = useMemo(() => countProblems(groups.flatMap((group) => group.problems)), [groups]);
  const byPath = useCallback((path: string) => indexRef.current[path] ?? [], []);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);

  return { groups, counts, byPath, refresh };
}
