/**
 * The IDE main area (UI.md §8): the project tab strip, breadcrumb, outline and
 * the Monaco editor. Documents, dirty state and saving are owned by
 * `useProjectEditor`; this component is presentation plus the LSP outline.
 */
import { useEffect, useRef, useState, type ReactElement } from 'react';
import type { LspRequestResult } from '@fergents/protocol';

import { EditorTabs, type EditorTab } from '../editor/EditorTabs';
import { MonacoHost, type RevealRequest } from '../editor/MonacoHost';
import { isDirty } from '../editor/documents';
import { parseOutline, type OutlineItem } from '../editor/outline';
import type { EditorController } from '../editor/useProjectEditor';
import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { baseName } from '../project/model';

/** Props for `IdeView`. */
export interface IdeViewProps {
  kernel: KernelClient;
  /** The active project id, or `null` when nothing is open. */
  projectId: string | null;
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
export function IdeView({ kernel, projectId, editor, theme, trusted }: IdeViewProps): ReactElement {
  const { documents, active, activePath, conflict } = editor;
  const [outline, setOutline] = useState<OutlineItem[]>([]);
  const [reveal, setReveal] = useState<RevealRequest | null>(null);
  const revealToken = useRef(0);

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

  const revealAt = (line: number): void => {
    revealToken.current += 1;
    setReveal({ line, token: revealToken.current });
  };

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
                    <OutlineList items={outline} onReveal={(item) => revealAt(item.line)} />
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
  );
}
