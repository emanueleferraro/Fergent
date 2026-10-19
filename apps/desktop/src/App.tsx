/**
 * The desktop shell: rail + sidebar + main area + status bar (UI.md §3),
 * with the Ctrl+K command palette and the dark/light theme toggle.
 *
 * Every element is composed from the `style/` design system only.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from 'react';
import { version as uiVersion } from '@fergents/ui';

import { CommandPalette } from './features/palette/CommandPalette';
import { commands } from './features/palette/commands';
import { EditorArea } from './features/editor/EditorArea';
import { BrainExplorer } from './features/explorer/BrainExplorer';
import { GraphView } from './features/graph/GraphView';
import { SearchPanel } from './features/search/SearchPanel';
import { useTheme } from './features/theme/theme';
import { useKernel, type KernelStatus } from './hooks/useKernel';
import { t } from './i18n/i18n';
import type { MessageKey } from './i18n/en';
import { createKernelClient } from './lib/kernel';

const RAIL_ITEMS = [
  { id: 'chat', titleKey: 'rail.chat' },
  { id: 'brain', titleKey: 'rail.brain' },
  { id: 'ide', titleKey: 'rail.ide' },
  { id: 'workflow', titleKey: 'rail.workflow' },
  { id: 'dashboard', titleKey: 'rail.dashboard' },
  { id: 'agents', titleKey: 'rail.agents' },
  { id: 'plugins', titleKey: 'rail.plugins' },
  { id: 'settings', titleKey: 'rail.settings' },
] as const satisfies readonly { id: string; titleKey: MessageKey }[];

type RailId = (typeof RAIL_ITEMS)[number]['id'];

const RAIL_ICONS: Record<RailId, string> = {
  chat: 'M4 5h16v11H9l-5 3z',
  brain:
    'M12 4a3 3 0 0 0-3 3 3 3 0 0 0-1 5 3 3 0 0 0 2 4 3 3 0 0 0 6-1 3 3 0 0 0 1-5 3 3 0 0 0-5-6z',
  ide: 'M9 8l-4 4 4 4M15 8l4 4-4 4',
  workflow: 'M4 6h6v4H4zM14 14h6v4h-6zM10 8h4v8',
  dashboard: 'M4 4h7v7H4zM13 4h7v4h-7zM13 10h7v10h-7zM4 13h7v7H4z',
  agents: 'M12 3l7 4v5c0 4-3 7-7 9-4-2-7-5-7-9V7z',
  plugins: 'M9 4h6v3h3v6h-3v7H9v-3H6V9h3z',
  settings: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 3v2M12 19v2M3 12h2M19 12h2',
};

const SUBSCRIBE_TOPICS = ['kernel.'] as const;

function statusKey(status: KernelStatus): MessageKey {
  switch (status) {
    case 'connected':
      return 'kernel.connected';
    case 'error':
      return 'kernel.error';
    default:
      return 'kernel.connecting';
  }
}

function statusBadge(status: KernelStatus): string {
  switch (status) {
    case 'connected':
      return 'badge-green';
    case 'error':
      return 'badge-red';
    default:
      return 'badge-yellow';
  }
}

function RailIcon({ id }: { id: RailId }): ReactElement {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d={RAIL_ICONS[id]} />
    </svg>
  );
}

/** The application root. */
export function App(): ReactElement {
  const { theme, toggle } = useTheme();
  const kernel = useMemo(() => createKernelClient(), []);
  const kernelState = useKernel(kernel, SUBSCRIBE_TOPICS);
  const [activeId, setActiveId] = useState<RailId>('chat');
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [openIds, setOpenIds] = useState<string[]>([]);
  const [activeDocId, setActiveDocId] = useState<string | undefined>(undefined);
  const [searchQuery, setSearchQuery] = useState<string | undefined>(undefined);
  const [brainView, setBrainView] = useState<'notes' | 'graph'>('notes');
  const openIdsRef = useRef(openIds);
  openIdsRef.current = openIds;

  const openDocument = useCallback((documentId: string) => {
    setOpenIds((prev) => (prev.includes(documentId) ? prev : [...prev, documentId]));
    setActiveDocId(documentId);
  }, []);

  const closeDocument = useCallback((documentId: string) => {
    setOpenIds((prev) => prev.filter((id) => id !== documentId));
    setActiveDocId((current) =>
      current === documentId
        ? openIdsRef.current.filter((id) => id !== documentId).at(-1)
        : current,
    );
  }, []);

  const toggleTheme = useCallback(() => {
    toggle();
  }, [toggle]);

  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent): void => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setPaletteOpen((open) => !open);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const activeItem = RAIL_ITEMS.find((item) => item.id === activeId);

  return (
    <div className="shell">
      <nav className="shell__rail" aria-label={t('rail.label')}>
        {RAIL_ITEMS.map((item) => (
          <button
            key={item.id}
            type="button"
            className="rail-item"
            title={t(item.titleKey)}
            aria-current={activeId === item.id ? 'page' : undefined}
            onClick={() => setActiveId(item.id)}
          >
            <RailIcon id={item.id} />
          </button>
        ))}
      </nav>

      <aside className="shell__sidebar">
        {activeId === 'brain' ? (
          <BrainExplorer
            kernel={kernel}
            activeDocumentId={activeDocId}
            onOpenDocument={openDocument}
            onSearch={(query) => setSearchQuery(query.length > 0 ? query : undefined)}
          />
        ) : (
          <>
            <div className="panel-title">{activeItem ? t(activeItem.titleKey) : ''}</div>
            <p className="t-caption">{t('sidebar.placeholder')}</p>
            <div className="empty">
              <div className="empty-title">{t('sidebar.empty.title')}</div>
              <div className="empty-desc">
                {t('sidebar.empty.desc', {
                  section: activeItem ? t(activeItem.titleKey) : '',
                })}
              </div>
            </div>
          </>
        )}
      </aside>

      <main className={activeId === 'brain' ? 'shell__main shell__main--flush' : 'shell__main'}>
        {activeId === 'brain' ? (
          <div className="brain-view">
            <div className="brain-view__bar">
              <div className="tabs" role="tablist" aria-label={t('brain.view.label')}>
                <button
                  type="button"
                  role="tab"
                  className={brainView === 'notes' ? 'tab active' : 'tab'}
                  aria-selected={brainView === 'notes'}
                  onClick={() => setBrainView('notes')}
                >
                  {t('brain.view.notes')}
                </button>
                <button
                  type="button"
                  role="tab"
                  className={brainView === 'graph' ? 'tab active' : 'tab'}
                  aria-selected={brainView === 'graph'}
                  onClick={() => setBrainView('graph')}
                >
                  {t('brain.view.graph')}
                </button>
              </div>
            </div>
            <div className="brain-view__body">
              {searchQuery ? (
                <SearchPanel
                  kernel={kernel}
                  query={searchQuery}
                  onOpen={openDocument}
                  onClose={() => setSearchQuery(undefined)}
                />
              ) : brainView === 'graph' ? (
                <GraphView kernel={kernel} onOpen={openDocument} />
              ) : (
                <EditorArea
                  kernel={kernel}
                  openIds={openIds}
                  activeId={activeDocId}
                  onSelect={setActiveDocId}
                  onClose={closeDocument}
                  onOpen={openDocument}
                />
              )}
            </div>
          </div>
        ) : (
          <>
            <h1 className="t-h1">{t('app.name')}</h1>
            <p className="t-body-sm">{t('main.subtitle')}</p>
            <div className="panel panel-stack">
              <div className="panel-title">{t('main.kernel.title')}</div>
              <div className="panel-row">
                <span className={`badge ${statusBadge(kernelState.status)}`}>
                  <span className="dot" />
                  {t(statusKey(kernelState.status))}
                </span>
                {kernelState.health ? (
                  <span className="t-mono">
                    {t('main.kernel.version', { version: kernelState.health.kernel_version })}
                  </span>
                ) : null}
              </div>
              {kernelState.error ? (
                <div className="inline-feedback error">{kernelState.error}</div>
              ) : null}
              <button className="btn btn-sm" type="button" onClick={() => setPaletteOpen(true)}>
                {t('main.openPalette')}
              </button>
            </div>
          </>
        )}
      </main>

      <footer className="shell__status">
        <span>{t('status.kernel', { status: t(statusKey(kernelState.status)) })}</span>
        <span className="t-mono">{t('status.ui', { version: uiVersion })}</span>
        <span className="spacer" />
        <span>
          {t('status.theme', { theme: t(theme === 'dark' ? 'theme.dark' : 'theme.light') })}
        </span>
        <button className="btn btn-ghost btn-sm" type="button" onClick={toggleTheme}>
          {t('theme.toggle')}
        </button>
        <span className="t-mono">{t('status.paletteHint')}</span>
      </footer>

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        kernel={kernel}
        commands={commands}
        toggleTheme={toggleTheme}
      />
    </div>
  );
}
