/**
 * Command registry for the Ctrl+K palette.
 *
 * Each command is a stub that dispatches through the kernel protocol (or a
 * local UI action). Commands are data-driven so they can be filtered and
 * tested without React.
 */
import type { MessageKey } from '../../i18n/en';
import type { KernelClient } from '../../lib/kernel';

/** Dependencies injected into a command when it runs. */
export interface CommandContext {
  kernel: KernelClient;
  toggleTheme(): void;
}

/** A palette command. */
export interface Command {
  id: string;
  titleKey: MessageKey;
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
    const haystack = `${translate(command.titleKey)} ${translate(command.groupKey)}`.toLowerCase();
    return haystack.includes(needle);
  });
}
