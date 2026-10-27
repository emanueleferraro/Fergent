/**
 * `useSlotContributions` — subscribe a component to one slot.
 */
import { useSyncExternalStore } from 'react';

import type { SlotContribution, SlotName, SlotRegistry } from './slots';

/** The live, ordered contributions for `slot`. */
export function useSlotContributions(
  registry: SlotRegistry,
  slot: SlotName,
  hostApiVersion?: number,
): readonly SlotContribution[] {
  return useSyncExternalStore(
    registry.subscribe,
    () => registry.getSlot(slot, hostApiVersion),
    () => registry.getSlot(slot, hostApiVersion),
  );
}
