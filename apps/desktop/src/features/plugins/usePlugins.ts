/**
 * Installed plugins: list, enable/disable, reload, uninstall, and metrics.
 *
 * Only reachable while `capability.plugins` is on; the kernel rejects the
 * calls otherwise and the error is surfaced by the view.
 */
import { useCallback, useEffect, useState } from 'react';

import type { KernelClient } from '../../lib/kernel';

import { sortPlugins, type PluginListResult, type PluginStats, type PluginView } from './model';

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The state returned by {@link usePlugins}. */
export interface PluginsController {
  loading: boolean;
  error: string | null;
  plugins: PluginView[];
  /** The id of the plugin with an in-flight action, or `list`. */
  pending: string | null;
  refresh(): Promise<void>;
  setEnabled(id: string, enabled: boolean): Promise<void>;
  reloadPlugin(id: string): Promise<void>;
  uninstall(id: string, removeStorage?: boolean): Promise<void>;
  loadStats(id: string): Promise<PluginStats>;
}

/** The installed-plugin registry. */
export function usePlugins(kernel: KernelClient): PluginsController {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [plugins, setPlugins] = useState<PluginView[]>([]);
  const [pending, setPending] = useState<string | null>(null);

  const refresh = useCallback(async (): Promise<void> => {
    setPending('list');
    try {
      const result = await kernel.call<PluginListResult>('plugin.list');
      setPlugins(sortPlugins(result.plugins));
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
        const view = await kernel.call<PluginView>(enabled ? 'plugin.enable' : 'plugin.disable', {
          id,
        });
        setPlugins((previous) =>
          sortPlugins(previous.map((item) => (item.id === view.id ? view : item))),
        );
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  const reloadPlugin = useCallback(
    async (id: string): Promise<void> => {
      setPending(id);
      setError(null);
      try {
        const view = await kernel.call<PluginView>('plugin.reload', { id });
        setPlugins((previous) =>
          sortPlugins(previous.map((item) => (item.id === view.id ? view : item))),
        );
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  const uninstall = useCallback(
    async (id: string, removeStorage = false): Promise<void> => {
      setPending(id);
      setError(null);
      try {
        await kernel.call('plugin.uninstall', { id, remove_storage: removeStorage });
        setPlugins((previous) => previous.filter((item) => item.id !== id));
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  const loadStats = useCallback(
    (id: string): Promise<PluginStats> => kernel.call<PluginStats>('plugin.stats', { id }),
    [kernel],
  );

  return {
    loading,
    error,
    plugins,
    pending,
    refresh,
    setEnabled,
    reloadPlugin,
    uninstall,
    loadStats,
  };
}
