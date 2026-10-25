/**
 * The IDE main area (UI.md §8): the project tab strip, breadcrumb, outline and
 * the Monaco editor, plus the Problems panel and the language-server actions
 * (spec §3). Documents, dirty state and saving are owned by
 * `useProjectEditor`; this component is presentation plus the LSP wiring.
 */
import { useEffect, useMemo, useRef, useState, type ReactElement } from 'react';
import type { ComposerRun, LspRequestResult } from '@fergents/protocol';

import { EditorTabs, type EditorTab } from '../editor/EditorTabs';
import {
  MonacoHost,
  type MonacoHostInline,
  type MonacoHostLsp,
  type MonacoSelection,
  type RevealRequest,
} from '../editor/MonacoHost';
import { isDirty } from '../editor/documents';
import { parseOutline, type OutlineItem } from '../editor/outline';
import type { EditorController } from '../editor/useProjectEditor';
import { ProblemsPanel } from '../lsp/ProblemsPanel';
import type { LspCodeAction } from '../lsp/lspResult';
import { toProblem } from '../lsp/problems';
import { useLspDocuments } from '../lsp/useLspDocuments';
import { useLspEditor } from '../lsp/useLspEditor';
import { useProblems } from '../lsp/useProblems';
import { SourceControlPanel } from '../git/SourceControlPanel';
import { ComposerPanel } from '../composer/ComposerPanel';
import { InlineActionPanel } from '../inline/InlineActionPanel';
import type { InlineActionKind } from '../inline/inlineEngine';
import { INLINE_MAX_TOKENS } from '../inline/inlineModel';
import { useInlineAi } from '../inline/useInlineAi';
import { RunPanel } from '../runs/RunPanel';
import { useCapabilityEnabled } from '../settings/useCapabilityEnabled';
import { TerminalPanel } from '../terminal/TerminalPanel';
import { t } from '../../i18n/i18n';
import type { MessageKey } from '../../i18n/en';
import type { KernelClient } from '../../lib/kernel';
import { baseName } from '../project/model';

/** The inline quick actions on `Ctrl+.` (spec §6). */
const INLINE_ACTIONS: readonly { kind: InlineActionKind; key: MessageKey }[] = [
  { kind: 'explain', key: 'inline.action.explain' },
  { kind: 'refactor', key: 'inline.action.refactor' },
  { kind: 'tests', key: 'inline.action.tests' },
  { kind: 'document', key: 'inline.action.document' },
  { kind: 'fix', key: 'inline.action.fix' },
];

/** Where `Ctrl+.` was pressed: the quick-action menu anchors on this. */
interface QuickContext {
  path: string;
  line: number;
  column: number;
  selection: string;
}

/** Which bottom panel the IDE shows (`none` hides both). */
export type IdePanel = 'none' | 'problems' | 'terminal';

/** Props for `IdeView`. */
export interface IdeViewProps {
  kernel: KernelClient;
  /** The active project id, or `null` when nothing is open. */
  projectId: string | null;
  /** The absolute project root, used to resolve `file://` LSP targets. */
  projectRoot: string;
  /** The code editor controller. */
  editor: EditorController;
  theme: 'dark' | 'light';
  /** Whether the project is trusted (spec D3). */
  trusted: boolean;
  /** The visible bottom panel; owned by the shell so the palette can toggle it. */
  panel: IdePanel;
  onPanelChange: (panel: IdePanel) => void;
  /** Whether the source-control panel is open. */
  showScm: boolean;
  onToggleScm: () => void;
  /** Whether the Composer panel is open. */
  showComposer: boolean;
  onToggleComposer: () => void;
  /** Whether the agent-run panel is open. */
  showRuns: boolean;
  onToggleRuns: () => void;
  /** Reports the editor selection so the shell can attach it to chat. */
  onSelectionChange?: ((selection: MonacoSelection | null) => void) | undefined;
}

/** Render an outline tree, deepest last. */
function OutlineList({
  items,
  onReveal,
  depth = 1,
}: {
  items: readonly OutlineItem[];
  onReveal: (item: OutlineItem) => void;
  depth?: number;
}): ReactElement {
  return (
    <ul className={depth === 1 ? 'ide-outline' : 'ide-outline ide-outline--nested'} role="group">
      {items.map((item) => (
        <li key={`${item.line}:${item.column}:${item.name}`} role="treeitem" aria-level={depth}>
          <button type="button" className="ide-outline__item" onClick={() => onReveal(item)}>
            <span className="ide-outline__name">{item.name}</span>
            {item.detail === null ? null : (
              <span className="ide-outline__detail t-caption">{item.detail}</span>
            )}
          </button>
          {item.children.length === 0 ? null : (
            <OutlineList items={item.children} onReveal={onReveal} depth={depth + 1} />
          )}
        </li>
      ))}
    </ul>
  );
}

/** The tabbed project file view. */
export function IdeView({
  kernel,
  projectId,
  projectRoot,
  editor,
  theme,
  trusted,
  panel,
  onPanelChange,
  showScm,
  onToggleScm,
  showComposer,
  onToggleComposer,
  showRuns,
  onToggleRuns,
  onSelectionChange,
}: IdeViewProps): ReactElement {
  const { documents, active, activePath, conflict } = editor;
  const [outline, setOutline] = useState<OutlineItem[]>([]);
  const [reveal, setReveal] = useState<RevealRequest | null>(null);
  const revealToken = useRef(0);
  const [actions, setActions] = useState<LspCodeAction[] | null>(null);
  const [quick, setQuick] = useState<QuickContext | null>(null);
  const [inlineRun, setInlineRun] = useState<ComposerRun | null>(null);
  const [lspError, setLspError] = useState<string | null>(null);
  const problems = useProblems(kernel, projectId, trusted);
  const lsp = useLspEditor(kernel, projectId, projectRoot, trusted);
  const inlineEnabled = useCapabilityEnabled(kernel, 'capability.inline-ai');
  const inline = useInlineAi(kernel, projectId, trusted, inlineEnabled);
  useLspDocuments(kernel, projectId, trusted, documents);

  // The outline is best-effort: no server means no outline, never an error.
  useEffect(() => {
    if (projectId === null || activePath === null || !trusted) {
      setOutline([]);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const result = await kernel.call<LspRequestResult>('lsp.symbols', {
          project_id: projectId,
          path: activePath,
        });
        if (!cancelled) {
          setOutline(parseOutline(result.result));
        }
      } catch {
        if (!cancelled) {
          setOutline([]);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel, projectId, activePath, trusted]);

  const markers = useMemo(
    () => (activePath === null ? [] : problems.byPath(activePath).map(toProblem)),
    // Diagnostics for a path change with `groups` (the grouped view of the index).
    [activePath, problems.byPath, problems.groups],
  );

  const revealAt = (line: number, column?: number): void => {
    revealToken.current += 1;
    setReveal({ line, column, token: revealToken.current });
  };

  const openAt = (path: string, line: number, column: number): void => {
    void (async () => {
      await editor.open(path);
      revealAt(line, column);
    })();
  };

  const goToDefinition = (path: string, line: number, column: number): void => {
    void (async () => {
      const target = await lsp.definition(path, line, column);
      if (target === null) {
        setLspError(t('lsp.definition.none'));
        return;
      }
      setLspError(null);
      openAt(target.path, target.line, target.column);
    })();
  };

  const renameSymbol = (path: string, line: number, column: number): void => {
    const next = window.prompt(t('lsp.rename.prompt'));
    if (next === null || next.trim().length === 0) {
      return;
    }
    void (async () => {
      const changes = await lsp.rename(path, line, column, next.trim());
      if (changes.length > 0) {
        await editor.applyWorkspaceEdits(changes);
      }
    })();
  };

  const showQuickActions = (
    path: string,
    line: number,
    column: number,
    selection: string,
  ): void => {
    setQuick({ path, line, column, selection });
    void (async () => {
      setActions(await lsp.codeActions(path, line, column));
    })();
  };

  const closeQuickActions = (): void => {
    setQuick(null);
    setActions(null);
  };

  const runInlineAction = (kind: InlineActionKind): void => {
    const at = quick;
    if (at === null) {
      return;
    }
    closeQuickActions();
    void (async () => {
      const run = await inline.action(kind, at.path, at.selection);
      if (run !== null) {
        setInlineRun(run);
      }
    })();
  };

  const applyAction = (action: LspCodeAction): void => {
    closeQuickActions();
    if (action.edits.length > 0) {
      void editor.applyWorkspaceEdits(action.edits);
    }
  };

  const hostLsp: MonacoHostLsp | null = trusted
    ? {
        hover: lsp.hover,
        onDefinition: goToDefinition,
        onRename: renameSymbol,
        onCodeActions: (path, line, column) => showQuickActions(path, line, column, ''),
      }
    : null;

  const hostInline: MonacoHostInline | null =
    trusted && inlineEnabled
      ? {
          complete: (path, text, offset, languageId, readOnly) =>
            inline.suggest({ path, text, offset, languageId, readOnly }),
          onQuickActions: showQuickActions,
        }
      : null;

  const tabs: EditorTab[] = documents.map((document) => ({
    documentId: document.path,
    title: baseName(document.path),
    modified: isDirty(document),
  }));

  const segments = active === null ? [] : active.path.split('/').filter((part) => part.length > 0);

  return (
    <div className="ide-view">
      <div className="ide-view__workspace">
        <div className="ide-view__column">
          <EditorTabs
            tabs={tabs}
            activeId={activePath ?? undefined}
            onSelect={editor.select}
            onClose={editor.close}
            ariaLabel={t('ide.tabs.label')}
          />
          {active === null ? (
            <div className="empty">
              <div className="empty-title">
                {editor.busy ? t('ide.view.loading') : t('ide.view.empty.title')}
              </div>
              <div className="empty-desc">{t('ide.view.empty.desc')}</div>
            </div>
          ) : (
            <>
              <div className="editor-breadcrumb">
                <span>{t('ide.view.root')}</span>
                {segments.map((segment, index) => (
                  <span
                    key={`${index}:${segment}`}
                    className={index === segments.length - 1 ? 'current' : undefined}
                  >
                    {segment}
                  </span>
                ))}
              </div>

              {!trusted ? (
                <div className="inline-feedback warning">{t('ide.view.untrusted')}</div>
              ) : null}

              {conflict === null ? null : (
                <div className="inline-feedback warning ide-view__banner">
                  <span>
                    {conflict.external ? t('ide.conflict.external') : t('ide.conflict.save')}
                  </span>
                  <button className="btn btn-sm" type="button" onClick={() => void editor.reload()}>
                    {t('ide.conflict.reload')}
                  </button>
                  <button
                    className="btn btn-sm"
                    type="button"
                    onClick={() => void editor.keepMine()}
                  >
                    {t('ide.conflict.keep')}
                  </button>
                  <button
                    className="btn btn-ghost btn-sm"
                    type="button"
                    onClick={editor.dismissConflict}
                  >
                    {t('ide.conflict.dismiss')}
                  </button>
                </div>
              )}

              {editor.error === null ? null : (
                <div className="inline-feedback error">{editor.error}</div>
              )}

              {lspError === null ? null : <div className="inline-feedback info">{lspError}</div>}

              {inline.status.error === null ? null : (
                <div className="inline-feedback error">
                  {t('inline.failed', { error: inline.status.error })}
                </div>
              )}

              {quick === null ? null : (
                <div className="ide-actions" role="menu" aria-label={t('ide.actions.label')}>
                  <span className="panel-title">{t('ide.actions.label')}</span>
                  {inlineEnabled && trusted ? (
                    <>
                      <span className="t-caption">{t('inline.actions.label')}</span>
                      {INLINE_ACTIONS.map(({ kind, key }) => (
                        <button
                          key={kind}
                          role="menuitem"
                          type="button"
                          className="btn btn-sm"
                          onClick={() => runInlineAction(kind)}
                        >
                          {t(key)}
                        </button>
                      ))}
                    </>
                  ) : null}
                  {actions === null ? null : actions.length === 0 ? (
                    <span className="t-caption">{t('lsp.actions.empty')}</span>
                  ) : (
                    actions.map((action) => (
                      <button
                        key={action.title}
                        role="menuitem"
                        type="button"
                        className="btn btn-sm"
                        onClick={() => applyAction(action)}
                      >
                        {action.title}
                      </button>
                    ))
                  )}
                  <button
                    className="btn btn-ghost btn-sm"
                    type="button"
                    onClick={closeQuickActions}
                  >
                    {t('lsp.actions.close')}
                  </button>
                </div>
              )}

              {active.truncated ? (
                <div className="inline-feedback warning">{t('ide.view.truncated')}</div>
              ) : null}
              {active.readOnly && !active.binary && !active.truncated && trusted ? (
                <div className="inline-feedback warning">{t('ide.view.readOnly')}</div>
              ) : null}

              <div className="ide-view__body">
                {active.binary ? (
                  <div className="empty">
                    <div className="empty-title">{t('ide.view.binary.title')}</div>
                    <div className="empty-desc">{t('ide.view.binary.desc')}</div>
                  </div>
                ) : (
                  <div className="ide-editor">
                    {outline.length === 0 ? null : (
                      <aside className="ide-editor__outline" aria-label={t('ide.outline.label')}>
                        <div className="panel-title">{t('ide.outline.label')}</div>
                        <OutlineList
                          items={outline}
                          onReveal={(item) => revealAt(item.line, item.column)}
                        />
                      </aside>
                    )}
                    <div className="ide-editor__main">
                      <MonacoHost
                        path={active.path}
                        content={active.content}
                        languageId={active.languageId}
                        readOnly={active.readOnly || !trusted}
                        theme={theme}
                        openPaths={documents.map((document) => document.path)}
                        reveal={reveal}
                        markers={markers}
                        lsp={hostLsp}
                        inline={hostInline}
                        onSelection={onSelectionChange}
                        onChange={(content) => editor.change(active.path, content)}
                        onSave={() => void editor.save(active.path)}
                      />
                    </div>
                  </div>
                )}
              </div>

              {editor.busy ? <div className="inline-feedback">{t('ide.view.busy')}</div> : null}
            </>
          )}
        </div>

        {showScm ? (
          <SourceControlPanel
            kernel={kernel}
            projectId={projectId}
            trusted={trusted}
            activePath={activePath}
            onClose={onToggleScm}
          />
        ) : null}

        {inlineRun === null ? null : (
          <InlineActionPanel
            key={inlineRun.id}
            kernel={kernel}
            run={inlineRun}
            theme={theme}
            onClose={() => setInlineRun(null)}
            onApplied={(paths) => {
              for (const path of paths) {
                void editor.reload(path);
              }
            }}
          />
        )}

        {inlineRun !== null || showRuns ? null : showComposer ? (
          <ComposerPanel
            kernel={kernel}
            projectId={projectId}
            trusted={trusted}
            activePath={activePath}
            openPaths={documents.map((document) => document.path)}
            theme={theme}
            onClose={onToggleComposer}
            onApplied={(paths) => {
              for (const path of paths) {
                void editor.reload(path);
              }
            }}
          />
        ) : null}

        {showRuns ? (
          <RunPanel
            projectId={projectId}
            trusted={trusted}
            kernel={kernel}
            onClose={onToggleRuns}
          />
        ) : null}
      </div>

      {panel === 'problems' ? (
        <ProblemsPanel
          kernel={kernel}
          projectId={projectId}
          groups={problems.groups}
          counts={problems.counts}
          onOpen={openAt}
          onClose={() => onPanelChange('none')}
        />
      ) : null}

      {panel === 'terminal' ? (
        <TerminalPanel
          kernel={kernel}
          projectId={projectId}
          trusted={trusted}
          theme={theme}
          onClose={() => onPanelChange('none')}
        />
      ) : null}

      <div className="ide-view__status">
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          aria-pressed={panel === 'problems'}
          onClick={() => onPanelChange(panel === 'problems' ? 'none' : 'problems')}
        >
          {t('ide.problems.toggle')}
          <span className="problems-count problem-severity--error">{problems.counts.error}</span>
          <span className="problems-count problem-severity--warning">
            {problems.counts.warning}
          </span>
        </button>
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          aria-pressed={panel === 'terminal'}
          onClick={() => onPanelChange(panel === 'terminal' ? 'none' : 'terminal')}
        >
          {t('ide.terminal.toggle')}
        </button>
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          aria-pressed={showScm}
          onClick={onToggleScm}
        >
          {t('ide.scm.toggle')}
        </button>
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          aria-pressed={showComposer}
          onClick={onToggleComposer}
        >
          {t('ide.composer.toggle')}
        </button>
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          aria-pressed={showRuns}
          onClick={onToggleRuns}
        >
          {t('ide.runs.toggle')}
        </button>
        <span className="t-caption">
          {inlineEnabled ? t('inline.status', { tokens: INLINE_MAX_TOKENS }) : t('inline.off')}
        </span>
        {inline.status.latencyMs === null ? null : (
          <span className="t-caption">{t('inline.latency', { ms: inline.status.latencyMs })}</span>
        )}
        {inline.status.suppressed === 'largeFile' ? (
          <span className="t-caption">{t('inline.suppressed.largeFile')}</span>
        ) : null}
        <span className="t-caption">
          {t('ide.problems.summary', {
            errors: problems.counts.error,
            warnings: problems.counts.warning,
          })}
        </span>
      </div>
    </div>
  );
}
