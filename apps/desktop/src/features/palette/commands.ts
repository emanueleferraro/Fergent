/**
 * Command registry for the Ctrl+K palette.
 *
 * Each command is a stub that dispatches through the kernel protocol (or a
 * local UI action). Commands are data-driven so they can be filtered and
 * tested without React.
 */
import type { MessageKey } from '../../i18n/en';
import type { KernelClient } from '../../lib/kernel';

/** A settings tab a command can jump to. */
export type SettingsTarget =
  'brain' | 'features' | 'plugins' | 'skills' | 'providers' | 'secrets' | 'mcp' | 'ide';

/** A tab of the plugin hub section. */
export type PluginHubTarget = 'marketplace' | 'dev';

/** Dependencies injected into a command when it runs. */
export interface CommandContext {
  kernel: KernelClient;
  toggleTheme(): void;
  /** Switches the shell to Settings on the given tab. */
  openSettings?(tab: SettingsTarget): void;
  /** Switches the shell to the plugin hub on the given tab. */
  openPluginHub?(tab: PluginHubTarget): void;
}

/** A palette command. */
export interface Command {
  id: string;
  titleKey: MessageKey;
  /** Literal title contributed by a plugin; overrides `titleKey`. */
  title?: string;
  groupKey: MessageKey;
  /** Optional shortcut hint rendered by the palette. */
  shortcut?: string;
  run(context: CommandContext): void | Promise<void>;
}

/** The Phase 0 stub commands. */
export const commands: readonly Command[] = [
  {
    id: 'kernel.health',
    titleKey: 'command.health',
    groupKey: 'palette.group.kernel',
    run: (context) => {
      void context.kernel.health();
    },
  },
  {
    id: 'kernel.ping',
    titleKey: 'command.ping',
    groupKey: 'palette.group.kernel',
    run: (context) => {
      void context.kernel.call('ping');
    },
  },
  {
    id: 'brain.list',
    titleKey: 'command.brainList',
    groupKey: 'palette.group.kernel',
    run: (context) => {
      void context.kernel.call('brain.list');
    },
  },
  {
    id: 'theme.toggle',
    titleKey: 'command.toggleTheme',
    groupKey: 'palette.group.view',
    run: (context) => {
      context.toggleTheme();
    },
  },
  {
    id: 'plugins.install',
    titleKey: 'command.installPlugin',
    groupKey: 'palette.group.plugins',
    run: (context) => {
      context.openSettings?.('plugins');
    },
  },
  {
    id: 'plugins.manage',
    titleKey: 'command.managePlugins',
    groupKey: 'palette.group.plugins',
    run: (context) => {
      context.openSettings?.('plugins');
    },
  },
  {
    id: 'skills.install',
    titleKey: 'command.installSkill',
    groupKey: 'palette.group.plugins',
    run: (context) => {
      context.openSettings?.('skills');
    },
  },
  {
    id: 'plugins.dev',
    titleKey: 'command.pluginDevPanel',
    groupKey: 'palette.group.plugins',
    run: (context) => {
      context.openPluginHub?.('dev');
    },
  },
];

/** Filter commands by a free-text query, matching translated title and group. */
export function filterCommands(
  list: readonly Command[],
  query: string,
  translate: (key: MessageKey) => string,
): Command[] {
  const needle = query.trim().toLowerCase();
  if (needle === '') {
    return [...list];
  }
  return list.filter((command) => {
    const title = command.title ?? translate(command.titleKey);
    const haystack = `${title} ${translate(command.groupKey)}`.toLowerCase();
    return haystack.includes(needle);
  });
}
