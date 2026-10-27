import { describe, expect, it } from 'vitest';

import {
  API_VERSION,
  SLOTS,
  definePlugin,
  isApiCompatible,
  isSlotName,
  type SlotName,
  type SlotProps,
} from './index.js';

function Panel(props: SlotProps): SlotProps {
  return props;
}

describe('slot catalogue', () => {
  it('exposes the 16 slots of UI.md §22', () => {
    expect(SLOTS).toHaveLength(16);
    expect(SLOTS).toContain('rail.item');
    expect(SLOTS).toContain('theme.tokens');
  });

  it('recognizes only catalogue entries', () => {
    expect(isSlotName('dashboard.widget')).toBe(true);
    expect(isSlotName('sidebar.widget')).toBe(false);
  });

  it('matches the kernel N-1 compatibility rule', () => {
    expect(API_VERSION).toBe(1);
    expect(isApiCompatible(1)).toBe(true);
    expect(isApiCompatible(0)).toBe(true);
    expect(isApiCompatible(2)).toBe(false);
    expect(isApiCompatible(1, 2)).toBe(true);
    expect(isApiCompatible(3, 2)).toBe(false);
  });
});

describe('definePlugin', () => {
  it('fills in the defaults', () => {
    const definition = definePlugin({ id: 'com.example.weather' });

    expect(definition).toEqual({
      id: 'com.example.weather',
      apiVersion: API_VERSION,
      slots: [],
      commands: [],
    });
  });

  it('keeps slot contributions and commands', () => {
    const slot: SlotName = 'rail.item';
    const definition = definePlugin({
      id: 'com.example.weather',
      slots: [{ id: 'rail', slot, order: 10, render: Panel }],
      commands: [{ id: 'refresh', label: 'Refresh weather', shortcut: 'mod+shift+w' }],
    });

    expect(definition.slots).toHaveLength(1);
    expect(definition.slots[0]?.slot).toBe('rail.item');
    expect(definition.commands[0]?.label).toBe('Refresh weather');
  });

  it('rejects an invalid id', () => {
    expect(() => definePlugin({ id: '' })).toThrow(/invalid plugin id/);
    expect(() => definePlugin({ id: 'Com.Example' })).toThrow(/invalid plugin id/);
    expect(() => definePlugin({ id: '.hidden' })).toThrow(/invalid plugin id/);
  });

  it('rejects an unsupported api version', () => {
    expect(() => definePlugin({ id: 'com.example.a', apiVersion: 2 })).toThrow(
      /api_version 2 is not supported/,
    );
  });

  it('rejects an unknown slot, a duplicate contribution, and a malformed command', () => {
    const badSlot = 'nope' as SlotName;
    expect(() =>
      definePlugin({ id: 'com.example.a', slots: [{ id: 'x', slot: badSlot, render: Panel }] }),
    ).toThrow(/unknown plugin slot/);

    expect(() =>
      definePlugin({
        id: 'com.example.a',
        slots: [
          { id: 'x', slot: 'rail.item', render: Panel },
          { id: 'x', slot: 'rail.item', render: Panel },
        ],
      }),
    ).toThrow(/duplicate slot contribution/);

    expect(() =>
      definePlugin({ id: 'com.example.a', commands: [{ id: 'run', label: '  ' }] }),
    ).toThrow(/needs a label/);
  });
});
