/**
 * The desktop shell: rail + sidebar + main area + status bar (UI.md §3),
 * with the Ctrl+K command palette and the dark/light theme toggle.
 *
 * Every element is composed from the `style/` design system only.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from 'react';
import { version as uiVersion } from '@fergents/ui';
import type { LspRequestResult } from '@fergents/protocol';

import { CommandPalette } from './features/palette/CommandPalette';
import { commands } from './features/palette/commands';
import { ChatPanel } from './features/chat/ChatPanel';
import { ChatSidebar } from './features/chat/ChatSidebar';
import { useChat } from './features/chat/useChat';
import { AgentsView } from './features/agents/AgentsView';
import { EditorArea } from './features/editor/EditorArea';
import { isTextEdit } from './features/editor/textEdits';
import { useProjectEditor } from './features/editor/useProjectEditor';
import { BrainExplorer } from './features/explorer/BrainExplorer';
import { GraphView } from './features/graph/GraphView';
import { IdeView, type IdePanel } from './features/ide/IdeView';
import { usePluginCommands } from './features/plugins/commands';
import { PluginHub, type PluginTab } from './features/plugins/PluginHub';
import {
  appendContext,
  selectionPayload,
  type SelectionAttachment,
} from './features/ide/chatContext';
import { cycleTabPath, ideCommands, type IdeActions } from './features/ide/ideCommands';
import { SplitPane } from './features/layout/SplitPane';
import { useIdeLayout } from './features/layout/useIdeLayout';
import { ProjectExplorer } from './features/project/ProjectExplorer';
import { useProjects } from './features/project/useProjects';
import { SearchPanel } from './features/search/SearchPanel';
import { SettingsView, type SettingsTab } from './features/settings/SettingsView';
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

// An empty topic list subscribes to every event: the IDE needs `file.changed`,
// `lsp.diagnostics`, and `term.*` alongside the kernel's own events.
const SUBSCRIBE_TOPICS: readonly string[] = [];

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
  const chat = useChat(kernel);
  const projects = useProjects(kernel);
  const projectId = projects.active?.id ?? null;
  const trusted = projects.active?.trust === 'trusted';
  const ideLayout = useIdeLayout(kernel, projectId);
  const editor = useProjectEditor(kernel, projectId, { trusted });
  const [activeId, setActiveId] = useState<RailId>('chat');
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [idePanel, setIdePanel] = useState<IdePanel>('none');
  const [showScm, setShowScm] = useState(false);
  const [showComposer, setShowComposer] = useState(false);
  const [showRuns, setShowRuns] = useState(false);
  const [showChat, setShowChat] = useState(true);
  const [openIds, setOpenIds] = useState<string[]>([]);
  const [activeDocId, setActiveDocId] = useState<string | undefined>(undefined);
  const [searchQuery, setSearchQuery] = useState<string | undefined>(undefined);
  const [brainView, setBrainView] = useState<'notes' | 'graph'>('notes');
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('brain');
  const [pluginTab, setPluginTab] = useState<PluginTab>('marketplace');
  const openIdsRef = useRef(openIds);
  openIdsRef.current = openIds;
  const selectionRef = useRef<SelectionAttachment | null>(null);

  const openSettings = useCallback((tab: SettingsTab) => {
    setSettingsTab(tab);
    setActiveId('settings');
  }, []);

  const openPluginHub = useCallback((tab: PluginTab) => {
    setPluginTab(tab);
    setActiveId('plugins');
  }, []);

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

  const openNoteFromIde = useCallback(
    (documentId: string) => {
      openDocument(documentId);
      setActiveId('brain');
    },
    [openDocument],
  );

  const toggleTheme = useCallback(() => {
    toggle();
  }, [toggle]);

  const toggleIdePanel = useCallback((panel: Exclude<IdePanel, 'none'>) => {
    setIdePanel((current) => (current === panel ? 'none' : panel));
  }, []);

  // The right rail shows one panel at a time.
  const toggleScm = useCallback(() => {
    setShowScm((shown) => !shown);
    setShowComposer(false);
    setShowRuns(false);
  }, []);

  const toggleComposer = useCallback(() => {
    setShowComposer((shown) => !shown);
    setShowScm(false);
    setShowRuns(false);
  }, []);

  const toggleRuns = useCallback(() => {
    setShowRuns((shown) => !shown);
    setShowScm(false);
    setShowComposer(false);
  }, []);

  const selectTab = useCallback(
    (delta: number) => {
      const next = cycleTabPath(
        editor.documents.map((document) => document.path),
        editor.activePath,
        delta,
      );
      if (next !== null) {
        editor.select(next);
      }
    },
    [editor],
  );

  const jumpToTab = useCallback(
    (index: number) => {
      const document = editor.documents[index];
      if (document !== undefined) {
        editor.select(document.path);
      }
    },
    [editor],
  );

  const formatDocument = useCallback(() => {
    const path = editor.activePath;
    if (projectId === null || path === null || !trusted) {
      return;
    }
    void (async () => {
      try {
        const result = await kernel.call<LspRequestResult>('lsp.format', {
          project_id: projectId,
          path,
          tab_size: 2,
          insert_spaces: true,
        });
        const edits = Array.isArray(result.result) ? result.result.filter(isTextEdit) : [];
        if (edits.length > 0) {
          editor.applyEdits(path, edits);
        }
      } catch {
        // No language server for this file: the buffer stays as it is.
      }
    })();
  }, [editor, kernel, projectId, trusted]);

  // The editor reports its selection so "Add to chat" can attach the range.
  const trackSelection = useCallback((selection: SelectionAttachment | null) => {
    selectionRef.current = selection;
  }, []);

  // "Add to chat" grounds the conversation in the open file (`@path`), or in
  // the selection when there is one (`@path#Lx-Ly` plus the code).
  const addToChat = useCallback(() => {
    const path = editor.activePath;
    if (path === null) {
      return;
    }
    const selection = selectionRef.current;
    const payload =
      selection !== null && selection.path === path ? selectionPayload(selection) : `@${path}`;
    chat.setDraft(appendContext(chat.draft, payload));
    setActiveId('ide');
  }, [chat, editor.activePath]);

  const ideActions = useMemo<IdeActions>(
    () => ({
      hasDocument: editor.activePath !== null,
      save: () => void editor.save(),
      formatDocument,
      closeTab: () => {
        if (editor.activePath !== null) {
          editor.close(editor.activePath);
        }
      },
      nextTab: () => selectTab(1),
      previousTab: () => selectTab(-1),
      jumpToTab,
      toggleProblems: () => toggleIdePanel('problems'),
      toggleTerminal: () => toggleIdePanel('terminal'),
      toggleScm,
      toggleChat: () => setShowChat((shown) => !shown),
      toggleComposer,
      toggleRuns,
      addToChat,
    }),
    [
      addToChat,
      editor,
      formatDocument,
      jumpToTab,
      selectTab,
      toggleComposer,
      toggleIdePanel,
      toggleRuns,
      toggleScm,
    ],
  );

  const pluginPaletteCommands = usePluginCommands(kernel);

  const paletteCommands = useMemo(
    () => [...commands, ...ideCommands(ideActions), ...pluginPaletteCommands],
    [ideActions, pluginPaletteCommands],
  );

  const ideActionsRef = useRef(ideActions);
  ideActionsRef.current = ideActions;

  // Global shortcuts (UI.md §19.2). Editor-local keys are Monaco's; the
  // handler only reacts when nobody else consumed the event.
  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent): void => {
      if (event.defaultPrevented) {
        return;
      }
      const mod = event.ctrlKey || event.metaKey;
      const ide = ideActionsRef.current;
      if (mod && !event.altKey && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setPaletteOpen((open) => !open);
        return;
      }
      if (mod && !event.altKey && event.key === 'Tab') {
        event.preventDefault();
        if (event.shiftKey) {
          ide.previousTab();
        } else {
          ide.nextTab();
        }
        return;
      }
      if (mod && !event.altKey && event.key === '\\') {
        event.preventDefault();
        ide.toggleChat();
        return;
      }
      if (mod && !event.altKey && /^[1-9]$/.test(event.key) && ide.hasDocument) {
        event.preventDefault();
        ide.jumpToTab(Number(event.key) - 1);
        return;
      }
      if (
        mod &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === 'w' &&
        ide.hasDocument
      ) {
        event.preventDefault();
        ide.closeTab();
        return;
      }
      if (
        mod &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === 's' &&
        ide.hasDocument
      ) {
        event.preventDefault();
        ide.save();
        return;
      }
      if (event.altKey && event.shiftKey && event.code === 'KeyF' && ide.hasDocument) {
        event.preventDefault();
        ide.formatDocument();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const activeItem = RAIL_ITEMS.find((item) => item.id === activeId);

  const ideView = (
    <IdeView
      kernel={kernel}
      projectId={projectId}
      projectRoot={projects.active?.path ?? ''}
      editor={editor}
      theme={theme}
      trusted={trusted}
      panel={idePanel}
      onPanelChange={setIdePanel}
      showScm={showScm}
      onToggleScm={toggleScm}
      showComposer={showComposer}
      onToggleComposer={toggleComposer}
      showRuns={showRuns}
      onToggleRuns={toggleRuns}
      onSelectionChange={trackSelection}
    />
  );

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
        ) : activeId === 'chat' ? (
          <ChatSidebar chat={chat} />
        ) : activeId === 'ide' ? (
          <ProjectExplorer
            kernel={kernel}
            projects={projects.projects}
            active={projects.active}
            files={projects.files}
            statuses={projects.statuses}
            activePath={editor.activePath ?? undefined}
            error={projects.error}
            onSelectProject={projects.select}
            onOpenProject={(path) => void projects.open(path)}
            onTrust={(trustedNow) => void projects.trust(trustedNow)}
            onOpenFile={(path) => void editor.open(path)}
            onOpenNote={openNoteFromIde}
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

      <main
        className={
          activeId === 'brain' || activeId === 'ide'
            ? 'shell__main shell__main--flush'
            : 'shell__main'
        }
      >
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
        ) : activeId === 'chat' ? (
          <ChatPanel kernel={kernel} chat={chat} />
        ) : activeId === 'ide' ? (
          showChat ? (
            <SplitPane
              direction="row"
              ratio={ideLayout.ratio}
              label={t('ide.split.label')}
              onRatioChange={ideLayout.resize}
              onReset={ideLayout.reset}
              first={
                <div className="ide-workspace__chat">
                  <ChatPanel kernel={kernel} chat={chat} />
                </div>
              }
              second={ideView}
            />
          ) : (
            ideView
          )
        ) : activeId === 'settings' ? (
          <SettingsView kernel={kernel} tab={settingsTab} onTabChange={setSettingsTab} />
        ) : activeId === 'plugins' ? (
          <PluginHub kernel={kernel} tab={pluginTab} onTabChange={setPluginTab} />
        ) : activeId === 'agents' ? (
          <AgentsView kernel={kernel} />
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
        <span>
          {t('status.project', {
            name: projects.active?.name ?? t('status.project.none'),
          })}
        </span>
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
        commands={paletteCommands}
        toggleTheme={toggleTheme}
        openSettings={openSettings}
        openPluginHub={openPluginHub}
      />
    </div>
  );
}
