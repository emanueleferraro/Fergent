/**
 * The plugin slot registry (`UI.md` §22).
 *
 * The shell exposes 16 named regions. A plugin contributes a React surface to
 * one of them; the host mounts that surface inside a shadow root (Task 4.5),
 * so plugin markup and CSS can never leak into the shell.
 *
 * The registry is deliberately UI-only: it holds no plugin code and performs
 * no permission checks. The kernel remains the security boundary.
 */
import {
  SLOTS,
  isSlotName as isCatalogueSlot,
  type SlotName as CatalogueSlotName,
} from '@fergents/plugin-sdk';
import type { ComponentType } from 'react';

import type { PluginBridge } from './bridge';
import { isApiCompatible } from './version';

/** The 16 slots of `UI.md` §22 (slot API v1), shared with the SDK. */
export const SLOT_NAMES = SLOTS;

/** A named slot. */
export type SlotName = CatalogueSlotName;

/** Whether `value` is one of the 16 slots. */
export function isSlotName(value: string): value is SlotName {
  return isCatalogueSlot(value);
}

/** Props every plugin surface receives. */
export interface SlotSurfaceProps {
  /** Kernel access bound to the owning plugin. */
  bridge: PluginBridge;
}

/** One plugin's contribution to a slot. */
export interface SlotContribution {
  /** Contribution id, unique within the plugin for this slot. */
  id: string;
  /** The owning plugin. */
  pluginId: string;
  /** The slot to render into. */
  slot: SlotName;
  /** The slot API version the plugin targets. */
  apiVersion: number;
  /** Lower renders first; ties break on plugin id, then contribution id. */
  order?: number;
  /** The surface mounted inside the slot's shadow root. */
  render: ComponentType<SlotSurfaceProps>;
}

/** Drop contributions the shell cannot render. */
export function filterByApiVersion(
  contributions: readonly SlotContribution[],
  hostApiVersion?: number,
): SlotContribution[] {
  return contributions.filter((item) => isApiCompatible(item.apiVersion, hostApiVersion));
}

/** Stable ordering: explicit order, then plugin id, then contribution id. */
export function sortContributions(contributions: readonly SlotContribution[]): SlotContribution[] {
  return [...contributions].sort((left, right) => {
    const order = (left.order ?? 0) - (right.order ?? 0);
    if (order !== 0) {
      return order;
    }
    const plugin = left.pluginId.localeCompare(right.pluginId);
    return plugin !== 0 ? plugin : left.id.localeCompare(right.id);
  });
}

/** The DOM id of one surface, stable across remounts. */
export function surfaceKey(contribution: SlotContribution): string {
  return `${contribution.pluginId}::${contribution.slot}::${contribution.id}`;
}

function contributionKey(contribution: Pick<SlotContribution, 'pluginId' | 'id'>): string {
  return `${contribution.pluginId}::${contribution.id}`;
}

/**
 * Registered slot contributions.
 *
 * Subscriptions are synchronous; `getSlot` returns the same array instance
 * until the registry changes, so it can back `useSyncExternalStore`.
 */
export class SlotRegistry {
  private readonly contributions = new Map<string, SlotContribution>();
  private readonly listeners = new Set<() => void>();
  private readonly snapshot = new Map<string, readonly SlotContribution[]>();

  /** Register a contribution; returns the unregister function. */
  register(contribution: SlotContribution): () => void {
    if (!isSlotName(contribution.slot)) {
      throw new Error(`unknown plugin slot: ${contribution.slot}`);
    }
    const key = contributionKey(contribution);
    this.contributions.set(key, contribution);
    this.invalidate();
    return () => {
      if (this.contributions.get(key) === contribution) {
        this.contributions.delete(key);
        this.invalidate();
      }
    };
  }

  /** Remove every contribution of one plugin (enable/disable, uninstall). */
  unregisterPlugin(pluginId: string): void {
    let changed = false;
    for (const [key, contribution] of this.contributions) {
      if (contribution.pluginId === pluginId) {
        this.contributions.delete(key);
        changed = true;
      }
    }
    if (changed) {
      this.invalidate();
    }
  }

  /** The renderable contributions for a slot, ordered. */
  getSlot(slot: SlotName, hostApiVersion?: number): readonly SlotContribution[] {
    const cacheKey = `${slot}@${hostApiVersion ?? 'host'}`;
    const cached = this.snapshot.get(cacheKey);
    if (cached !== undefined) {
      return cached;
    }
    const items = sortContributions(
      filterByApiVersion(
        [...this.contributions.values()].filter((item) => item.slot === slot),
        hostApiVersion,
      ),
    );
    this.snapshot.set(cacheKey, items);
    return items;
  }

  /** Every registered contribution (used by tests and diagnostics). */
  list(): readonly SlotContribution[] {
    return [...this.contributions.values()];
  }

  /** Subscribe to registry changes. */
  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Remove every contribution. */
  clear(): void {
    this.contributions.clear();
    this.invalidate();
  }

  private invalidate(): void {
    this.snapshot.clear();
    for (const listener of this.listeners) {
      listener();
    }
  }
}
