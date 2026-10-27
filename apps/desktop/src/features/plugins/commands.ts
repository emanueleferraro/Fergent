/**
 * Palette contributions from installed plugins (`spec.md` §8, `UI.md` §11).
 *
 * Command ids are `plugin.<plugin-id>.<command>`; the first registration
 * wins, mirroring the kernel's conflict rule. `when` predicates are not
 * evaluated yet (no palette modes in Phase 4).
 */
import { useEffect, useMemo, useState } from 'react';

import type { KernelClient } from '../../lib/kernel';
import type { Command } from '../palette/commands';

import type { PluginListResult, PluginView } from './model';

/** Build the palette entries contributed by the installed plugins. */
export function pluginCommands(plugins: readonly PluginView[]): Command[] {
  const commands: Command[] = [];
  const seen = new Set<string>();

  for (const plugin of plugins) {
    if (!plugin.enabled) {
      continue;
    }
    for (const entry of plugin.commands) {
      const id = `plugin.${plugin.id}.${entry.id}`;
      if (seen.has(id)) {
        continue;
      }
      seen.add(id);
      commands.push({
        id,
        titleKey: 'palette.group.plugins',
        title: entry.label,
        groupKey: 'palette.group.plugins',
        ...(entry.shortcut === null ? {} : { shortcut: entry.shortcut }),
        run: (context) => {
          void context.kernel.call('plugin.command.run', {
            plugin_id: plugin.id,
            command: entry.id,
            arguments: {},
          });
        },
      });
    }
  }

  return commands;
}

/** The plugin contributions, loaded once from `plugin.list`. */
export function usePluginCommands(kernel: KernelClient): Command[] {
  const [plugins, setPlugins] = useState<PluginView[]>([]);

  useEffect(() => {
    let active = true;
    void kernel
      .call<PluginListResult>('plugin.list')
      .then((view) => {
        if (active) {
          setPlugins(view.plugins);
        }
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [kernel]);

  return useMemo(() => pluginCommands(plugins), [plugins]);
}
