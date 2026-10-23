/**
 * Loads the kernel capability registry (`capability.list`) and toggles
 * entries through `capability.set` (Task 2.12, spec Appendix D).
 */
import { useCallback, useEffect, useState } from 'react';
import type { CapabilityListResult, CapabilityView } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

/** A failed toggle, scoped to the capability it belongs to. */
export interface ToggleError {
  id: string;
  message: string;
}

/** The state returned by {@link useCapabilities}. */
export interface CapabilitiesState {
  loading: boolean;
  error: string | null;
  capabilities: CapabilityView[];
  pending: string | null;
  toggleError: ToggleError | null;
  setEnabled: (id: string, enabled: boolean, autoEnableDeps?: boolean) => Promise<void>;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The kernel capability registry, with an optimistic-free toggle path. */
export function useCapabilities(kernel: KernelClient): CapabilitiesState {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [capabilities, setCapabilities] = useState<CapabilityView[]>([]);
  const [pending, setPending] = useState<string | null>(null);
  const [toggleError, setToggleError] = useState<ToggleError | null>(null);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void (async () => {
      try {
        const result = await kernel.call<CapabilityListResult>('capability.list');
        if (active) {
          setCapabilities(result.capabilities);
          setError(null);
        }
      } catch (cause) {
        if (active) {
          setError(messageOf(cause));
        }
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel]);

  const setEnabled = useCallback(
    async (id: string, enabled: boolean, autoEnableDeps = false): Promise<void> => {
      setPending(id);
      setToggleError(null);
      try {
        const view = await kernel.call<CapabilityView | null>('capability.set', {
          id,
          enabled,
          auto_enable_deps: autoEnableDeps,
        });
        setCapabilities((prev) =>
          prev.map((capability) =>
            capability.id === id
              ? view && view.id === id
                ? view
                : { ...capability, enabled }
              : capability,
          ),
        );
      } catch (cause) {
        setToggleError({ id, message: messageOf(cause) });
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  return { loading, error, capabilities, pending, toggleError, setEnabled };
}
