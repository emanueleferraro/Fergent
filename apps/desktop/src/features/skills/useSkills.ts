/**
 * Installed skills: list, enable/disable, test, duplicate, delete, metrics.
 */
import { useCallback, useEffect, useState } from 'react';

import type { KernelClient } from '../../lib/kernel';

import { sortSkills, type SkillListResult, type SkillStats, type SkillView } from './model';

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The state returned by {@link useSkills}. */
export interface SkillsController {
  loading: boolean;
  error: string | null;
  skills: SkillView[];
  pending: string | null;
  refresh(): Promise<void>;
  setEnabled(id: string, enabled: boolean): Promise<void>;
  test(id: string, input: unknown): Promise<unknown>;
  duplicate(id: string, newId: string): Promise<SkillView | null>;
  remove(id: string, removeStorage?: boolean): Promise<void>;
  loadStats(id: string): Promise<SkillStats>;
}

/** The installed-skill registry. */
export function useSkills(kernel: KernelClient): SkillsController {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [skills, setSkills] = useState<SkillView[]>([]);
  const [pending, setPending] = useState<string | null>(null);

  const refresh = useCallback(async (): Promise<void> => {
    setPending('list');
    try {
      const result = await kernel.call<SkillListResult>('skill.list');
      setSkills(sortSkills(result.skills));
      setError(null);
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setLoading(false);
      setPending(null);
    }
  }, [kernel]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const setEnabled = useCallback(
    async (id: string, enabled: boolean): Promise<void> => {
      setPending(id);
      setError(null);
      try {
        const view = await kernel.call<SkillView>(enabled ? 'skill.enable' : 'skill.disable', {
          id,
        });
        setSkills((previous) =>
          sortSkills(previous.map((item) => (item.id === view.id ? view : item))),
        );
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  const test = useCallback(
    (id: string, input: unknown): Promise<unknown> =>
      kernel.call<{ output: unknown }>('skill.test', { id, input }).then((result) => result.output),
    [kernel],
  );

  const duplicate = useCallback(
    async (id: string, newId: string): Promise<SkillView | null> => {
      setPending(id);
      setError(null);
      try {
        const view = await kernel.call<SkillView>('skill.duplicate', { id, new_id: newId });
        setSkills((previous) => sortSkills([...previous, view]));
        return view;
      } catch (cause) {
        setError(messageOf(cause));
        return null;
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  const remove = useCallback(
    async (id: string, removeStorage = false): Promise<void> => {
      setPending(id);
      setError(null);
      try {
        await kernel.call('skill.delete', { id, remove_storage: removeStorage });
        setSkills((previous) => previous.filter((item) => item.id !== id));
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  const loadStats = useCallback(
    (id: string): Promise<SkillStats> => kernel.call<SkillStats>('skill.invocations', { id }),
    [kernel],
  );

  return {
    loading,
    error,
    skills,
    pending,
    refresh,
    setEnabled,
    test,
    duplicate,
    remove,
    loadStats,
  };
}
