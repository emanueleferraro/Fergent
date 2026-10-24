/**
 * IDE commands for the Ctrl+K palette (task 3.15, UI.md §11 and §19.2).
 *
 * Commands are plain data built from the IDE's current actions, so they can be
 * filtered and tested without React. Document-specific commands are omitted
 * when no editor tab is open, which keeps the palette honest.
 */
import type { Command } from '../palette/commands';

/** The IDE actions the palette and the global shortcuts can invoke. */
export interface IdeActions {
  /** Whether an editor tab is open. */
  hasDocument: boolean;
  save(): void;
  formatDocument(): void;
  closeTab(): void;
  nextTab(): void;
  previousTab(): void;
  /** Jump to the `index`-th open tab (Ctrl+1..9). */
  jumpToTab(index: number): void;
  toggleProblems(): void;
  toggleTerminal(): void;
  toggleScm(): void;
  /** Show or hide the chat pane beside the IDE (`Ctrl+\`). */
  toggleChat(): void;
  /** Show or hide the Composer panel. */
  toggleComposer(): void;
  /** Append the active file to the chat draft (`@path`). */
  addToChat(): void;
}

/**
 * The path of the tab `delta` positions away from `activePath`, wrapping
 * around. Returns `null` when there are no tabs.
 */
export function cycleTabPath(
  paths: readonly string[],
  activePath: string | null,
  delta: number,
): string | null {
  if (paths.length === 0) {
    return null;
  }
  const index = activePath === null ? -1 : paths.indexOf(activePath);
  const from = index === -1 ? (delta > 0 ? -1 : 0) : index;
  return paths[(((from + delta) % paths.length) + paths.length) % paths.length] ?? null;
}

/** The IDE command list for the current editor state. */
export function ideCommands(actions: IdeActions): Command[] {
  const view: Command[] = [
    {
      id: 'ide.toggleProblems',
      titleKey: 'ide.command.problems',
      groupKey: 'palette.group.ide',
      run: () => actions.toggleProblems(),
    },
    {
      id: 'ide.toggleTerminal',
      titleKey: 'ide.command.terminal',
      groupKey: 'palette.group.ide',
      run: () => actions.toggleTerminal(),
    },
    {
      id: 'ide.toggleScm',
      titleKey: 'ide.command.scm',
      groupKey: 'palette.group.ide',
      run: () => actions.toggleScm(),
    },
    {
      id: 'ide.toggleChat',
      titleKey: 'ide.command.chat',
      groupKey: 'palette.group.ide',
      shortcut: 'Ctrl+\\',
      run: () => actions.toggleChat(),
    },
    {
      id: 'ide.toggleComposer',
      titleKey: 'ide.command.composer',
      groupKey: 'palette.group.ide',
      run: () => actions.toggleComposer(),
    },
  ];
  if (!actions.hasDocument) {
    return view;
  }
  return [
    ...view,
    {
      id: 'ide.save',
      titleKey: 'ide.command.save',
      groupKey: 'palette.group.editor',
      shortcut: 'Ctrl+S',
      run: () => actions.save(),
    },
    {
      id: 'ide.format',
      titleKey: 'ide.command.format',
      groupKey: 'palette.group.editor',
      shortcut: 'Shift+Alt+F',
      run: () => actions.formatDocument(),
    },
    {
      id: 'ide.addToChat',
      titleKey: 'ide.command.addToChat',
      groupKey: 'palette.group.editor',
      run: () => actions.addToChat(),
    },
    {
      id: 'ide.closeTab',
      titleKey: 'ide.command.closeTab',
      groupKey: 'palette.group.editor',
      shortcut: 'Ctrl+W',
      run: () => actions.closeTab(),
    },
    {
      id: 'ide.previousTab',
      titleKey: 'ide.command.previousTab',
      groupKey: 'palette.group.editor',
      shortcut: 'Ctrl+Shift+Tab',
      run: () => actions.previousTab(),
    },
    {
      id: 'ide.nextTab',
      titleKey: 'ide.command.nextTab',
      groupKey: 'palette.group.editor',
      shortcut: 'Ctrl+Tab',
      run: () => actions.nextTab(),
    },
  ];
}
