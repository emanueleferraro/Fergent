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

  'explorer.brain.title': 'Brain',
  'explorer.files.label': 'Files',
  'explorer.files.empty.title': 'No notes yet',
  'explorer.files.empty.desc': 'Notes in this brain will appear here.',
  'explorer.search.label': 'Search notes',
  'explorer.search.placeholder': 'Search notes…',
  'explorer.search.clear': 'Clear search',
  'explorer.tags.label': 'Tags',
  'explorer.tags.empty': 'No tags yet',
  'explorer.outline.label': 'Outline',
  'explorer.outline.empty': 'No headings',
  'explorer.backlinks.label': 'Backlinks',
  'explorer.backlinks.empty': 'No backlinks',
  'explorer.mentions.label': 'Unlinked mentions',
  'explorer.mentions.empty': 'No unlinked mentions',
} as const;

/** A key of the English message catalog. */
export type MessageKey = keyof typeof en;
