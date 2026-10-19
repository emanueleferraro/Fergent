/**
 * Editor area (UI.md §6): the tab bar plus a split source/preview pane for the
 * active note. Loads notes through `doc.get` and persists edits through
 * `doc.write` (debounced), so the kernel stays the source of truth.
 */
import type { Document } from '@fergents/protocol';
import { useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { CodeEditor } from './CodeEditor';
import { EditorTabs, type EditorTab } from './EditorTabs';
import { MarkdownPreview } from './MarkdownPreview';

/** How long to wait after the last keystroke before saving. */
const SAVE_DELAY_MS = 800;

interface LoadedDocument {
  document: Document;
  content: string;
  savedContent: string;
}

/** Props for `EditorArea`. */
export interface EditorAreaProps {
  /** The connected kernel client. */
  kernel: KernelClient;
  /** Open document ids, in tab order. */
  openIds: readonly string[];
  /** The active document id. */
  activeId?: string | undefined;
  /** Select a tab. */
  onSelect: (documentId: string) => void;
  /** Close a tab. */
  onClose: (documentId: string) => void;
  /** Open a note (used by wiki-link navigation), by document id. */
  onOpen: (documentId: string) => void;
}

/** The tabbed markdown editor with live preview. */
export function EditorArea({
  kernel,
  openIds,
  activeId,
  onSelect,
  onClose,
  onOpen,
}: EditorAreaProps): ReactElement {
  const [docs, setDocs] = useState<Record<string, LoadedDocument>>({});
  const [paths, setPaths] = useState<Record<string, string>>({});
  const docsRef = useRef(docs);
  docsRef.current = docs;
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  useEffect(() => {
    let active = true;
    void (async () => {
      for (const id of openIds) {
        if (docsRef.current[id]) {
          continue;
        }
        try {
          const result = await kernel.call<{ document: Document; content: string }>('doc.get', {
            document_id: id,
          });
          if (!active) {
            return;
          }
          setDocs((prev) => ({
            ...prev,
            [id]: {
              document: result.document,
              content: result.content,
              savedContent: result.content,
            },
          }));
        } catch {
          // A missing note simply does not open; the tab remains empty.
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel, openIds]);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const result = await kernel.call<{ documents?: Document[] }>('doc.list');
        if (!active || !Array.isArray(result.documents)) {
          return;
        }
        const map: Record<string, string> = {};
        for (const doc of result.documents) {
          map[doc.path] = doc.id;
        }
        setPaths(map);
      } catch {
        // Navigation by path is best-effort.
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel]);

  const save = useCallback(
    async (documentId: string, content: string) => {
      try {
        await kernel.call('doc.write', { document_id: documentId, content });
        setDocs((prev) => {
          const entry = prev[documentId];
          return entry ? { ...prev, [documentId]: { ...entry, savedContent: content } } : prev;
        });
      } catch {
        // Keep the dirty marker; a later edit retries.
      }
    },
    [kernel],
  );

  const updateContent = useCallback(
    (documentId: string, content: string) => {
      setDocs((prev) => {
        const entry = prev[documentId];
        return entry ? { ...prev, [documentId]: { ...entry, content } } : prev;
      });
      const existing = timers.current.get(documentId);
      if (existing) {
        clearTimeout(existing);
      }
      timers.current.set(
        documentId,
        setTimeout(() => {
          timers.current.delete(documentId);
          void save(documentId, content);
        }, SAVE_DELAY_MS),
      );
    },
    [save],
  );

  useEffect(
    () => () => {
      for (const timer of timers.current.values()) {
        clearTimeout(timer);
      }
      timers.current.clear();
    },
    [],
  );

  const tabs = useMemo<EditorTab[]>(
    () =>
      openIds.map((id) => {
        const entry = docs[id];
        return {
          documentId: id,
          title: entry?.document.title ?? id,
          modified: entry ? entry.content !== entry.savedContent : false,
        };
      }),
    [openIds, docs],
  );

  const active = activeId ? docs[activeId] : undefined;

  const handleOpenLink = useCallback(
    (path: string) => {
      const id = paths[path];
      if (id) {
        onOpen(id);
      }
    },
    [paths, onOpen],
  );

  return (
    <div className="editor-area">
      <EditorTabs tabs={tabs} activeId={activeId} onSelect={onSelect} onClose={onClose} />
      {active && activeId ? (
        <>
          <div className="editor-breadcrumb">
            <span>{active.document.path}</span>
            <span className="current">{active.document.title}</span>
          </div>
          <div className="editor-split">
            <div className="editor-pane editor-pane--source">
              <CodeEditor
                value={active.content}
                onChange={(value) => updateContent(activeId, value)}
                ariaLabel={t('editor.source.label', { title: active.document.title })}
              />
            </div>
            <div className="editor-pane editor-preview">
              <MarkdownPreview source={active.content} onOpenLink={handleOpenLink} />
            </div>
          </div>
        </>
      ) : (
        <div className="empty">
          <div className="empty-title">{t('editor.empty.title')}</div>
          <div className="empty-desc">{t('editor.empty.desc')}</div>
        </div>
      )}
    </div>
  );
}
