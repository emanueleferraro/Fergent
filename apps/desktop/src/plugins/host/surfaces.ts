/**
 * Persistent plugin surfaces.
 *
 * Each surface is a `<div class="plugin-surface">` with an **open shadow
 * root**. The design-system stylesheets are injected into that root, so a
 * plugin surface gets the same tokens and components as the shell while its
 * own CSS can never reach the shell (and vice versa).
 *
 * A surface is keyed by plugin + slot + contribution and kept alive across
 * unmounts (D20): hiding a slot must not reset a plugin's UI state.
 */
import { createRoot, type Root } from 'react-dom/client';
import type { ReactNode } from 'react';

import componentsCss from '../../styles/components.css?raw';
import tokensCss from '../../styles/tokens.css?raw';

/** The styles every plugin shadow root receives, in mandatory order. */
export const SHADOW_STYLES = `${tokensCss}\n${componentsCss}`;

interface Surface {
  element: HTMLDivElement;
  root: Root;
}

const surfaces = new Map<string, Surface>();

/** A live surface handle. */
export interface SurfaceHandle {
  /** The host element to attach to the slot DOM. */
  element: HTMLElement;
  /** Render (or re-render) the plugin tree. */
  render(node: ReactNode): void;
}

/**
 * Get or create the surface for `key`.
 *
 * Creating a surface attaches its shadow root and starts a React root. Later
 * calls reuse the same root, so state survives detach/attach cycles.
 */
export function acquireSurface(key: string): SurfaceHandle {
  let surface = surfaces.get(key);
  if (surface === undefined) {
    const element = document.createElement('div');
    element.className = 'plugin-surface';
    element.dataset.fergentsSurface = key;
    const shadow = element.attachShadow({ mode: 'open' });
    const style = document.createElement('style');
    style.dataset.fergentsShadow = '';
    style.textContent = SHADOW_STYLES;
    shadow.appendChild(style);
    const container = document.createElement('div');
    shadow.appendChild(container);
    surface = { element, root: createRoot(container) };
    surfaces.set(key, surface);
  }
  const live = surface;
  return {
    element: live.element,
    render(node: ReactNode): void {
      live.root.render(node);
    },
  };
}

/** How many live surfaces exist (diagnostics and tests). */
export function surfaceCount(): number {
  return surfaces.size;
}

/** Tear down every surface. Tests call this between cases. */
export function resetPluginSurfaces(): void {
  for (const surface of surfaces.values()) {
    surface.root.unmount();
    surface.element.remove();
  }
  surfaces.clear();
}
