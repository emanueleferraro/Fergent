/**
 * Reads one capability's on/off state from the kernel registry. The kernel is
 * authoritative: a disabled capability never receives a request.
 */
import { useEffect, useState } from 'react';
import type { CapabilityListResult } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

/** Whether `id` is enabled right now (false while loading or on error). */
export function useCapabilityEnabled(kernel: KernelClient, id: string): boolean {
  const [enabled, setEnabled] = useState(false);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const result = await kernel.call<CapabilityListResult>('capability.list');
        const capability = result.capabilities.find((entry) => entry.id === id);
        if (active) {
          setEnabled(capability?.enabled ?? false);
        }
      } catch {
        if (active) {
          setEnabled(false);
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel, id]);

  return enabled;
}
