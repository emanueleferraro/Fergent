/**
 * The bridge handed to a plugin surface.
 *
 * A plugin never gets the raw `KernelClient`: every call goes through this
 * wrapper, which stamps the owning plugin id on the payload. The kernel is
 * the security boundary (ADR-021); the bridge only establishes identity so
 * the kernel can permission-gate the call.
 */
import type { KernelClient } from '../../lib/kernel';

import { HOST_API_VERSION } from './version';

/** The kernel access granted to one plugin surface. */
export interface PluginBridge {
  /** The plugin that owns this surface. */
  readonly pluginId: string;
  /** The slot API version the plugin targets. */
  readonly apiVersion: number;
  /** Call a kernel method. `plugin_id` is always the owning plugin. */
  call<T>(method: string, params?: Record<string, unknown>): Promise<T>;
}

/**
 * Build a bridge for `pluginId`.
 *
 * The plugin id is written last, so a plugin cannot spoof another plugin's
 * identity — or its own permissions — through the params.
 */
export function createPluginBridge(
  kernel: KernelClient,
  pluginId: string,
  apiVersion: number = HOST_API_VERSION,
): PluginBridge {
  return {
    pluginId,
    apiVersion,
    call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
      return kernel.call<T>(method, { ...params, plugin_id: pluginId });
    },
  };
}
