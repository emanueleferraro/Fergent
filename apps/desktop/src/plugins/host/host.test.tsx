/**
 * Plugin UI host tests (Task 4.5): registry ordering and versioning, shadow
 * root isolation, persistent surfaces, and plugin-id binding on the bridge.
 */
import { act, useState } from 'react';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { MockKernelClient } from '../../lib/kernel';

import { createPluginBridge, type PluginBridge } from './bridge';
import { PluginSlot } from './ShadowHost';
import {
  SLOT_NAMES,
  SlotRegistry,
  filterByApiVersion,
  sortContributions,
  type SlotContribution,
  type SlotName,
  type SlotSurfaceProps,
} from './slots';
import { resetPluginSurfaces, surfaceCount } from './surfaces';
import { HOST_API_VERSION, isApiCompatible } from './version';

afterEach(() => {
  cleanup();
  act(() => {
    resetPluginSurfaces();
  });
});

function contribution(overrides: Partial<SlotContribution> = {}): SlotContribution {
  return {
    id: 'main',
    pluginId: 'com.example.plugin',
    slot: 'rail.item',
    apiVersion: 1,
    render: Surface,
    ...overrides,
  };
}

function Surface({ bridge }: SlotSurfaceProps) {
  return <span>hello surface from {bridge.pluginId}</span>;
}

function Counter({ bridge }: SlotSurfaceProps) {
  const [count, setCount] = useState(0);
  return (
    <div>
      <span>count {count}</span>
      <button type="button" onClick={() => setCount((value) => value + 1)}>
        increment
      </button>
      <button type="button" onClick={() => void bridge.call('plugin.tool.call', {})}>
        call kernel
      </button>
    </div>
  );
}

function shadowOf(selector = '.plugin-surface'): ShadowRoot | null {
  return document.querySelector<HTMLElement>(selector)?.shadowRoot ?? null;
}

describe('slot registry', () => {
  it('exposes exactly the 16 UI.md slots', () => {
    expect(SLOT_NAMES).toHaveLength(16);
    expect(SLOT_NAMES).toContain('theme.tokens');
    expect(SLOT_NAMES).toContain('command.palette');
  });

  it('orders by order, then plugin id, then contribution id', () => {
    const registry = new SlotRegistry();
    registry.register(contribution({ id: 'b', pluginId: 'com.b', order: 1 }));
    registry.register(contribution({ id: 'a', pluginId: 'com.a', order: 1 }));
    registry.register(contribution({ id: 'z', pluginId: 'com.z', order: 0 }));

    expect(registry.getSlot('rail.item').map((item) => item.id)).toEqual(['z', 'a', 'b']);
  });

  it('rejects unknown slots', () => {
    const registry = new SlotRegistry();
    expect(() => registry.register(contribution({ slot: 'not.a.slot' as SlotName }))).toThrow(
      'unknown plugin slot',
    );
  });

  it('unregisters only its own contribution', () => {
    const registry = new SlotRegistry();
    const first = registry.register(contribution({ id: 'one' }));
    registry.register(contribution({ id: 'two' }));

    first();

    expect(registry.getSlot('rail.item').map((item) => item.id)).toEqual(['two']);
  });

  it('notifies subscribers and returns stable snapshots', () => {
    const registry = new SlotRegistry();
    const listener = vi.fn();
    const unsubscribe = registry.subscribe(listener);
    const before = registry.getSlot('statusbar.item');

    registry.register(contribution({ slot: 'statusbar.item', id: 'clock' }));
    const after = registry.getSlot('statusbar.item');

    expect(listener).toHaveBeenCalledTimes(1);
    expect(after).not.toBe(before);
    expect(registry.getSlot('statusbar.item')).toBe(after);

    unsubscribe();
    registry.register(contribution({ slot: 'statusbar.item', id: 'second' }));
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('removes every contribution of a plugin', () => {
    const registry = new SlotRegistry();
    registry.register(contribution({ id: 'one' }));
    registry.register(contribution({ id: 'two', slot: 'statusbar.item' }));
    registry.register(contribution({ id: 'three', pluginId: 'com.other' }));

    registry.unregisterPlugin('com.example.plugin');

    expect(registry.list().map((item) => item.id)).toEqual(['three']);
  });
});

describe('slot versioning', () => {
  it('accepts the current API and N-1, rejects the rest', () => {
    expect(HOST_API_VERSION).toBe(1);
    expect(isApiCompatible(1, 1)).toBe(true);
    expect(isApiCompatible(0, 1)).toBe(true);
    expect(isApiCompatible(2, 1)).toBe(false);
    expect(isApiCompatible(3, 1)).toBe(false);
  });

  it('filters contributions for the shell version', () => {
    const items = [
      contribution({ id: 'current', apiVersion: 1 }),
      contribution({ id: 'future', apiVersion: 2 }),
    ];
    expect(filterByApiVersion(items, 1).map((item) => item.id)).toEqual(['current']);
  });

  it('sorts without mutating the input', () => {
    const items = [contribution({ id: 'b', order: 2 }), contribution({ id: 'a', order: 1 })];
    expect(sortContributions(items).map((item) => item.id)).toEqual(['a', 'b']);
    expect(items.map((item) => item.id)).toEqual(['b', 'a']);
  });
});

describe('plugin bridge', () => {
  it('stamps the owning plugin id, overriding spoofed params', async () => {
    const kernel = new MockKernelClient();
    const bridge = createPluginBridge(kernel, 'com.example.rss-reader');

    await bridge.call('plugin.tool.call', {
      plugin_id: 'com.evil.spoof',
      id: 'com.example.rss-reader',
      tool: 'refresh',
    });

    expect(kernel.calls[0]).toEqual({
      method: 'plugin.tool.call',
      params: {
        plugin_id: 'com.example.rss-reader',
        id: 'com.example.rss-reader',
        tool: 'refresh',
      },
    });
  });

  it('exposes the targeted API version', () => {
    const bridge: PluginBridge = createPluginBridge({} as never, 'com.example.plugin', 1);
    expect(bridge.pluginId).toBe('com.example.plugin');
    expect(bridge.apiVersion).toBe(1);
  });
});

describe('ShadowHost', () => {
  it('renders the design system and the surface inside a shadow root', async () => {
    const registry = new SlotRegistry();
    registry.register(contribution());

    render(<PluginSlot kernel={new MockKernelClient()} registry={registry} slot="rail.item" />);

    const shadow = shadowOf();
    expect(shadow).not.toBeNull();
    expect(shadow?.querySelector('style')?.textContent).toContain('--color-accent');
    await waitFor(() => {
      expect(shadow?.textContent).toContain('hello surface from com.example.plugin');
    });
  });

  it('binds the plugin id on calls made through the surface bridge', async () => {
    const kernel = new MockKernelClient();
    const registry = new SlotRegistry();
    registry.register(
      contribution({
        id: 'chat',
        pluginId: 'com.fergents.weather',
        slot: 'chat.composer.extras',
        render: Counter,
      }),
    );

    render(<PluginSlot kernel={kernel} registry={registry} slot="chat.composer.extras" />);
    const button = await waitFor(() => {
      const found = Array.from(shadowOf()?.querySelectorAll('button') ?? []).find(
        (item) => item.textContent === 'call kernel',
      );
      expect(found).toBeDefined();
      return found as HTMLButtonElement;
    });
    fireEvent.click(button);

    await waitFor(() => {
      expect(kernel.calls).toEqual([
        { method: 'plugin.tool.call', params: { plugin_id: 'com.fergents.weather' } },
      ]);
    });
  });

  it('preserves surface state across detach and re-attach', async () => {
    const registry = new SlotRegistry();
    registry.register(contribution({ render: Counter }));
    const first = render(
      <PluginSlot kernel={new MockKernelClient()} registry={registry} slot="rail.item" />,
    );

    const host = document.querySelector<HTMLElement>('.plugin-surface');
    const shadow = host?.shadowRoot;
    await waitFor(() => {
      expect(shadow?.textContent).toContain('count 0');
    });
    const increment = Array.from(shadow?.querySelectorAll('button') ?? []).find(
      (item) => item.textContent === 'increment',
    );
    expect(increment).toBeDefined();
    fireEvent.click(increment as HTMLButtonElement);
    await waitFor(() => {
      expect(shadow?.textContent).toContain('count 1');
    });

    first.unmount();
    expect(document.querySelector('.plugin-surface')).toBeNull();
    expect(surfaceCount()).toBe(1);

    render(<PluginSlot kernel={new MockKernelClient()} registry={registry} slot="rail.item" />);
    expect(document.querySelector('.plugin-surface')).toBe(host);
    expect(shadow?.textContent).toContain('count 1');
  });

  it('skips contributions for an incompatible API version', () => {
    const registry = new SlotRegistry();
    registry.register(contribution({ slot: 'statusbar.item', apiVersion: 2, render: Counter }));

    render(
      <PluginSlot kernel={new MockKernelClient()} registry={registry} slot="statusbar.item" />,
    );

    expect(document.querySelector('.plugin-slot__item')).toBeNull();
    expect(surfaceCount()).toBe(0);
  });
});
