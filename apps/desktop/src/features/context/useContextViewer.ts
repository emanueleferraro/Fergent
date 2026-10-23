/**
 * The Context Viewer controller (Task 2.14): inspects the assembled context
 * for a session and toggles individual sections.
 */
import { useCallback, useEffect, useState } from 'react';
import type { ContextSnapshot } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

/** Everything the Context Viewer needs. */
export interface ContextViewerController {
  snapshot: ContextSnapshot | null;
  loading: boolean;
  error: string | null;
  /** The section currently being toggled, if any. */
  pending: string | null;
  inspect: () => Promise<void>;
  setSection: (name: string, enabled: boolean) => Promise<void>;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Inspect and toggle the context sections of `sessionId`. */
export function useContextViewer(
  kernel: KernelClient,
  sessionId: string | null,
): ContextViewerController {
  const [snapshot, setSnapshot] = useState<ContextSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);

  const inspect = useCallback(async (): Promise<void> => {
    if (!sessionId) {
      setSnapshot(null);
      return;
    }
    setLoading(true);
    try {
      const result = await kernel.call<ContextSnapshot>('context.inspect', {
        session_id: sessionId,
      });
      setSnapshot(result);
      setError(null);
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setLoading(false);
    }
  }, [kernel, sessionId]);

  useEffect(() => {
    void inspect();
  }, [inspect]);

  const setSection = useCallback(
    async (name: string, enabled: boolean): Promise<void> => {
      setPending(name);
      setError(null);
      try {
        await kernel.call('context.set_section', { section: name, enabled });
        await inspect();
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setPending(null);
      }
    },
    [kernel, inspect],
  );

  return { snapshot, loading, error, pending, inspect, setSection };
}
