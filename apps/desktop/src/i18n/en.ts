/**
 * English message catalog (default locale).
 *
 * All UI strings are externalized here; components resolve them through
 * `t()` in `i18n.ts` and never inline literals.
 */
export const en = {
  'app.name': 'Fergents',

  'rail.label': 'Sections',
  'rail.chat': 'Chat',
  'rail.brain': 'Brain',
  'rail.ide': 'IDE',
  'rail.workflow': 'Workflow',
  'rail.dashboard': 'Dashboard',
  'rail.agents': 'Agents',
  'rail.plugins': 'Plugins',
  'rail.settings': 'Settings',

  'sidebar.placeholder': 'This panel will host the contents of the active section.',
  'sidebar.empty.title': 'Nothing here yet',
  'sidebar.empty.desc': 'The {section} section is empty in Phase 0.',

  'main.subtitle': 'A local-first, agent-native workspace.',
  'main.kernel.title': 'Kernel',
  'main.kernel.version': 'v{version}',
  'main.openPalette': 'Open command palette (Ctrl+K)',

  'status.kernel': 'Kernel: {status}',
  'status.theme': 'Theme: {theme}',
  'status.ui': 'UI v{version}',
  'status.paletteHint': 'Ctrl+K',

  'kernel.connecting': 'Connecting',
  'kernel.connected': 'Connected',
  'kernel.error': 'Unavailable',

  'theme.dark': 'Dark',
  'theme.light': 'Light',
  'theme.toggle': 'Toggle theme',

  'palette.label': 'Command palette',
  'palette.placeholder': 'Search actions…',
  'palette.empty': 'No matching commands',
  'palette.hint': 'ESC',
  'palette.group.kernel': 'Kernel',
  'palette.group.view': 'View',

  'command.health': 'Show kernel health',
  'command.ping': 'Ping the kernel',
  'command.brainList': 'List brains',
  'command.toggleTheme': 'Toggle theme',
} as const;

/** A key of the English message catalog. */
export type MessageKey = keyof typeof en;
