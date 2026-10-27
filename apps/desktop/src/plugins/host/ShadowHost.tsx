/**
 * `<ShadowHost>` and `<PluginSlot>`.
 *
 * `ShadowHost` mounts one contribution into an isolated shadow root.
 * `PluginSlot` renders every compatible contribution for a named slot, in
 * registry order.
 */
import { createElement, useLayoutEffect, useRef, type ReactElement } from 'react';

import type { KernelClient } from '../../lib/kernel';

import { createPluginBridge } from './bridge';
import { surfaceKey, type SlotContribution, type SlotName, type SlotRegistry } from './slots';
import { acquireSurface } from './surfaces';
import { useSlotContributions } from './useSlots';
import { HOST_API_VERSION, isApiCompatible } from './version';

/** Props for `ShadowHost`. */
export interface ShadowHostProps {
  /** The kernel client the surface's bridge calls through. */
  kernel: KernelClient;
  /** The contribution to mount. */
  contribution: SlotContribution;
  /** The slot API version of this shell. */
  hostApiVersion?: number;
}

/** Mount one plugin surface inside its own shadow root. */
export function ShadowHost({
  kernel,
  contribution,
  hostApiVersion = HOST_API_VERSION,
}: ShadowHostProps): ReactElement | null {
  const holderRef = useRef<HTMLDivElement>(null);
  const { pluginId, slot, id, apiVersion, render } = contribution;
  const compatible = isApiCompatible(apiVersion, hostApiVersion);
  const key = surfaceKey(contribution);

  useLayoutEffect(() => {
    if (!compatible || holderRef.current === null) {
      return;
    }
    const handle = acquireSurface(key);
    holderRef.current.appendChild(handle.element);
    handle.render(
      createElement(render, {
        bridge: createPluginBridge(kernel, pluginId, apiVersion),
      }),
    );
    return () => {
      // The React root stays alive: detaching preserves the plugin's state
      // when the slot is hidden and shown again (D20).
      handle.element.remove();
    };
  }, [compatible, kernel, key, pluginId, apiVersion, render]);

  if (!compatible) {
    return null;
  }
  return (
    <div
      ref={holderRef}
      className="plugin-slot__item"
      data-plugin-id={pluginId}
      data-slot={slot}
      data-contribution={id}
    />
  );
}

/** Props for `PluginSlot`. */
export interface PluginSlotProps {
  /** The kernel client passed to every surface bridge. */
  kernel: KernelClient;
  /** The registry to read contributions from. */
  registry: SlotRegistry;
  /** The slot to render. */
  slot: SlotName;
  /** Extra class names for the slot wrapper. */
  className?: string;
  /** The slot API version of this shell. */
  hostApiVersion?: number;
}

/** Render every contribution for a slot. */
export function PluginSlot({
  kernel,
  registry,
  slot,
  className,
  hostApiVersion = HOST_API_VERSION,
}: PluginSlotProps): ReactElement {
  const contributions = useSlotContributions(registry, slot, hostApiVersion);
  return (
    <div className={className ? `plugin-slot ${className}` : 'plugin-slot'} data-slot={slot}>
      {contributions.map((contribution) => (
        <ShadowHost
          key={surfaceKey(contribution)}
          kernel={kernel}
          contribution={contribution}
          hostApiVersion={hostApiVersion}
        />
      ))}
    </div>
  );
}
