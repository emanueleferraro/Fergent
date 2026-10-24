/**
 * The IDE main area (UI.md §8): the project tab strip plus a read-only file
 * preview. Monaco replaces the preview in task 3.7; until then the tab strip,
 * breadcrumb, and file reads are the real thing, wired to the kernel.
 */
import { useEffect, useState, type ReactElement } from 'react';
import type { FileReadResult } from '@fergents/protocol';

import { EditorTabs, type EditorTab } from '../editor/EditorTabs';
import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { baseName } from '../project/model';

/** Props for `IdeView`. */
export interface IdeViewProps {
  kernel: KernelClient;
  /** The active project id, or `null` when nothing is open. */
  projectId: string | null;
  /** Open file paths, in tab order. */
  openPaths: readonly string[];
  /** The active file path. */
  activePath?: string | undefined;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
}

interface LoadedFile {
  path: string;
  content: string;
  binary: boolean;
  truncated: boolean;
}

/** The tabbed project file view. */
export function IdeView({
  kernel,
  projectId,
  openPaths,
  activePath,
  onSelect,
  onClose,
}: IdeViewProps): ReactElement {
  const [file, setFile] = useState<LoadedFile | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (projectId === null || activePath === undefined) {
      setFile(null);
      setError(null);
      return;
    }
    let cancelled = false;
    setError(null);
    setFile(null);
    void (async () => {
      try {
        const result = await kernel.call<FileReadResult>('file.read', {
          project_id: projectId,
          path: activePath,
        });
        if (!cancelled) {
          setFile({
            path: result.file.path,
            content: result.file.content,
            binary: result.file.binary,
            truncated: result.file.truncated,
          });
        }
      } catch (cause) {
        if (!cancelled) {
          setFile(null);
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel, projectId, activePath]);

  const tabs: EditorTab[] = openPaths.map((path) => ({
    documentId: path,
    title: baseName(path),
  }));

  const body = (): ReactElement => {
    if (file !== null) {
      if (file.binary) {
        return (
          <div className="empty">
            <div className="empty-title">{t('ide.view.binary.title')}</div>
            <div className="empty-desc">{t('ide.view.binary.desc')}</div>
          </div>
        );
      }
      return (
        <>
          <pre className="ide-view__preview t-mono" aria-label={activePath}>
            {file.content}
          </pre>
          {file.truncated ? (
            <div className="inline-feedback warning">{t('ide.view.truncated')}</div>
          ) : null}
        </>
      );
    }
    if (error !== null) {
      return (
        <div className="empty">
          <div className="empty-title">{t('ide.view.error.title')}</div>
          <div className="empty-desc">{error}</div>
        </div>
      );
    }
    return (
      <div className="empty">
        <div className="empty-title">{t('ide.view.loading')}</div>
      </div>
    );
  };

  return (
    <div className="ide-view">
      <EditorTabs
        tabs={tabs}
        activeId={activePath}
        onSelect={onSelect}
        onClose={onClose}
        ariaLabel={t('ide.tabs.label')}
      />
      {activePath !== undefined ? (
        <>
          <div className="editor-breadcrumb">
            <span>{t('ide.view.root')}</span>
            <span className="current">{activePath}</span>
          </div>
          <div className="ide-view__body">{body()}</div>
        </>
      ) : (
        <div className="empty">
          <div className="empty-title">{t('ide.view.empty.title')}</div>
          <div className="empty-desc">{t('ide.view.empty.desc')}</div>
        </div>
      )}
    </div>
  );
}
