/**
 * Language-server status manager (spec §3, decision D5): lists the configured
 * servers, refreshes their status for the current project, and starts, stops,
 * or restarts them. Servers are never installed automatically.
 */
import { useCallback, useEffect, useState } from 'react';
import type {
  LspServer,
  LspServerListResult,
  LspServerResult,
  LspServerStopResult,
} from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

/** The server manager's state and actions. */
export interface LspServersState {
  servers: LspServer[];
  loading: boolean;
  error: string | null;
  start: (languageId: string) => Promise<void>;
  stop: (languageId: string) => Promise<void>;
  restart: (languageId: string) => Promise<void>;
  reload: () => void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Language servers and their live status for a project. */
export function useLspServers(kernel: KernelClient, projectId: string | null): LspServersState {
  const [servers, setServers] = useState<LspServer[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const list = await kernel.call<LspServerListResult>('lsp.server.list');
        let next = list.servers;
        if (projectId !== null && list.servers.length > 0) {
          next = await Promise.all(
            list.servers.map(async (server) => {
              try {
                const status = await kernel.call<LspServerResult>('lsp.server.status', {
                  project_id: projectId,
                  language_id: server.language_id,
                });
                return status.server;
              } catch {
                // The status call fails for a missing binary; keep the record.
                return server;
              }
            }),
          );
        }
        if (!cancelled) {
          setServers(next);
        }
      } catch (cause) {
        if (!cancelled) {
          setServers([]);
          setError(messageOf(cause));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel, projectId, revision]);

  const act = useCallback(
    async (
      method: 'lsp.server.start' | 'lsp.server.stop' | 'lsp.server.restart',
      languageId: string,
    ) => {
      if (projectId === null) {
        return;
      }
      setError(null);
      try {
        if (method === 'lsp.server.stop') {
          await kernel.call<LspServerStopResult>(method, {
            project_id: projectId,
            language_id: languageId,
          });
          setServers((current) =>
            current.map((server) =>
              server.language_id === languageId ? { ...server, status: 'stopped' } : server,
            ),
          );
          return;
        }
        const result = await kernel.call<LspServerResult>(method, {
          project_id: projectId,
          language_id: languageId,
        });
        setServers((current) =>
          current.map((server) => (server.language_id === languageId ? result.server : server)),
        );
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel, projectId],
  );

  const start = useCallback((languageId: string) => act('lsp.server.start', languageId), [act]);
  const stop = useCallback((languageId: string) => act('lsp.server.stop', languageId), [act]);
  const restart = useCallback((languageId: string) => act('lsp.server.restart', languageId), [act]);
  const reload = useCallback(() => setRevision((value) => value + 1), []);

  return { servers, loading, error, start, stop, restart, reload };
}
