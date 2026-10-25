/**
 * The agent-run controller (task 3.14): starts a spec/plan/tasks workflow in a
 * project and reviews its artifacts and approval gates. The kernel owns the
 * run; this controller only reflects it.
 */
import { useCallback, useEffect, useState } from 'react';
import type { AutonomyMode, RunGate, SpecGetResult, WorkflowRun } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

/** Starting a run. */
export interface StartRunInput {
  slug: string;
  task: string;
  mode: AutonomyMode;
}

/** Everything the run panel needs. */
export interface RunsController {
  runs: WorkflowRun[];
  /** The open run with its artifact Markdown. */
  active: SpecGetResult | null;
  loading: boolean;
  busy: boolean;
  error: string | null;
  start: (input: StartRunInput) => Promise<void>;
  open: (runId: string) => Promise<void>;
  approve: () => Promise<void>;
  reject: () => Promise<void>;
  refresh: () => Promise<void>;
  dismissError: () => void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Load and drive the spec workflow of the project. */
export function useRuns(
  kernel: KernelClient,
  projectId: string | null,
  trusted: boolean,
): RunsController {
  const [runs, setRuns] = useState<WorkflowRun[]>([]);
  const [active, setActive] = useState<SpecGetResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<WorkflowRun[]> => {
    if (projectId === null) {
      setRuns([]);
      return [];
    }
    const result = await kernel.call<{ runs: WorkflowRun[] }>('run.list', { limit: 20 });
    setRuns(result.runs);
    return result.runs;
  }, [kernel, projectId]);

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    setActive(null);
    void (async () => {
      try {
        const listed = await load();
        if (!mounted) {
          return;
        }
        setError(null);
        if (listed.length > 0) {
          const first = listed[0];
          if (first !== undefined) {
            const detail = await kernel.call<SpecGetResult>('spec.get', { run_id: first.id });
            if (mounted) {
              setActive(detail);
            }
          }
        }
      } catch (cause) {
        if (mounted) {
          setError(messageOf(cause));
        }
      } finally {
        if (mounted) {
          setLoading(false);
        }
      }
    })();
    return () => {
      mounted = false;
    };
  }, [kernel, load]);

  const open = useCallback(
    async (runId: string): Promise<void> => {
      setError(null);
      try {
        const detail = await kernel.call<SpecGetResult>('spec.get', { run_id: runId });
        setActive(detail);
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel],
  );

  const start = useCallback(
    async (input: StartRunInput): Promise<void> => {
      if (projectId === null || !trusted) {
        return;
      }
      setBusy(true);
      setError(null);
      try {
        const run = await kernel.call<WorkflowRun>('spec.start', {
          slug: input.slug,
          task: input.task,
          mode: input.mode,
        });
        const detail = await kernel.call<SpecGetResult>('spec.get', { run_id: run.id });
        setActive(detail);
        await load();
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setBusy(false);
      }
    },
    [kernel, load, projectId, trusted],
  );

  const refresh = useCallback(async (): Promise<void> => {
    const current = active;
    try {
      await load();
      if (current !== null) {
        await open(current.run.id);
      }
    } catch (cause) {
      setError(messageOf(cause));
    }
  }, [active, load, open]);

  const approve = useCallback(async (): Promise<void> => {
    const current = active;
    if (current === null) {
      return;
    }
    const gate: RunGate | null = current.run.pending_gate;
    setBusy(true);
    setError(null);
    try {
      const run = await kernel.call<WorkflowRun>('spec.approve', {
        run_id: current.run.id,
        ...(gate === null ? {} : { gate }),
      });
      const detail = await kernel.call<SpecGetResult>('spec.get', { run_id: run.id });
      setActive(detail);
      await load();
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  }, [active, kernel, load]);

  const reject = useCallback(async (): Promise<void> => {
    const current = active;
    if (current === null) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const run = await kernel.call<WorkflowRun>('run.reject', {
        run_id: current.run.id,
        reason: null,
      });
      const detail = await kernel.call<SpecGetResult>('spec.get', { run_id: run.id });
      setActive(detail);
      await load();
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(false);
    }
  }, [active, kernel, load]);

  const dismissError = useCallback(() => setError(null), []);

  return {
    runs,
    active,
    loading,
    busy,
    error,
    start,
    open,
    approve,
    reject,
    refresh,
    dismissError,
  };
}
