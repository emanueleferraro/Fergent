/**
 * The IDE main area (UI.md §8): the project tab strip, breadcrumb, outline and
 * the Monaco editor, plus the Problems panel and the language-server actions
 * (spec §3). Documents, dirty state and saving are owned by
 * `useProjectEditor`; this component is presentation plus the LSP wiring.
 */
import { useEffect, useMemo, useRef, useState, type ReactElement } from 'react';
import type { LspRequestResult } from '@fergents/protocol';

import { EditorTabs, type EditorTab } from '../editor/EditorTabs';
import { MonacoHost, type MonacoHostLsp, type RevealRequest } from '../editor/MonacoHost';
import { isDirty } from '../editor/documents';
import { parseOutline, type OutlineItem } from '../editor/outline';
import type { EditorController } from '../editor/useProjectEditor';
import { ProblemsPanel } from '../lsp/ProblemsPanel';
import type { LspCodeAction } from '../lsp/lspResult';
import { toProblem } from '../lsp/problems';
import { useLspDocuments } from '../lsp/useLspDocuments';
import { useLspEditor } from '../lsp/useLspEditor';
import { useProblems } from '../lsp/useProblems';
import { TerminalPanel } from '../terminal/TerminalPanel';
import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { baseName } from '../project/model';

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
}: IdeViewProps): ReactElement {
  const { documents, active, activePath, conflict } = editor;
  const [outline, setOutline] = useState<OutlineItem[]>([]);
  const [reveal, setReveal] = useState<RevealRequest | null>(null);
  const revealToken = useRef(0);
  const [showProblems, setShowProblems] = useState(false);
  const [showTerminal, setShowTerminal] = useState(false);
  const [actions, setActions] = useState<LspCodeAction[] | null>(null);
  const [lspError, setLspError] = useState<string | null>(null);
  const problems = useProblems(kernel, projectId, trusted);
  const lsp = useLspEditor(kernel, projectId, projectRoot, trusted);
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

  const showCodeActions = (path: string, line: number, column: number): void => {
    void (async () => {
      setActions(await lsp.codeActions(path, line, column));
    })();
  };

  const applyAction = (action: LspCodeAction): void => {
    setActions(null);
    if (action.edits.length > 0) {
      void editor.applyWorkspaceEdits(action.edits);
    }
  };

  const hostLsp: MonacoHostLsp | null = trusted
    ? {
        hover: lsp.hover,
        onDefinition: goToDefinition,
        onRename: renameSymbol,
        onCodeActions: showCodeActions,
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
              <span>{conflict.external ? t('ide.conflict.external') : t('ide.conflict.save')}</span>
              <button className="btn btn-sm" type="button" onClick={() => void editor.reload()}>
                {t('ide.conflict.reload')}
              </button>
              <button className="btn btn-sm" type="button" onClick={() => void editor.keepMine()}>
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

          {actions === null ? null : (
            <div className="ide-actions" role="menu" aria-label={t('lsp.actions.label')}>
              <span className="panel-title">{t('lsp.actions.label')}</span>
              {actions.length === 0 ? (
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
                onClick={() => setActions(null)}
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

      {showProblems ? (
        <ProblemsPanel
          kernel={kernel}
          projectId={projectId}
          groups={problems.groups}
          counts={problems.counts}
          onOpen={openAt}
          onClose={() => setShowProblems(false)}
        />
      ) : null}

      {showTerminal ? (
        <TerminalPanel
          kernel={kernel}
          projectId={projectId}
          trusted={trusted}
          theme={theme}
          onClose={() => setShowTerminal(false)}
        />
      ) : null}

      <div className="ide-view__status">
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          aria-pressed={showProblems}
          onClick={() => {
            setShowProblems((shown) => !shown);
            setShowTerminal(false);
          }}
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
          aria-pressed={showTerminal}
          onClick={() => {
            setShowTerminal((shown) => !shown);
            setShowProblems(false);
          }}
        >
          {t('ide.terminal.toggle')}
        </button>
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
