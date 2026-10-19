/**
 * The editor tab bar (UI.md §6). Each tab is a `role="tab"` control with a
 * close button and a modified dot; Delete / Ctrl+W closes the focused tab.
 */
import type { MouseEvent, ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** One open editor tab. */
export interface EditorTab {
  documentId: string;
  title: string;
  /** Whether the tab has unsaved changes. */
  modified?: boolean | undefined;
}

/** Props for `EditorTabs`. */
export interface EditorTabsProps {
  tabs: readonly EditorTab[];
  activeId?: string | undefined;
  onSelect: (documentId: string) => void;
  onClose: (documentId: string) => void;
}

function CloseIcon(): ReactElement {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M7 7l10 10M17 7L7 17" />
    </svg>
  );
}

/** The editor tab bar. */
export function EditorTabs({ tabs, activeId, onSelect, onClose }: EditorTabsProps): ReactElement {
  const close = (event: MouseEvent, documentId: string): void => {
    event.stopPropagation();
    onClose(documentId);
  };

  return (
    <div className="editor-tabs" role="tablist" aria-label={t('editor.tabs.label')}>
      {tabs.map((tab) => {
        const active = tab.documentId === activeId;
        const closeLabel = t('editor.tabs.close', { title: tab.title });
        return (
          <div
            key={tab.documentId}
            role="tab"
            className={`editor-tab${active ? ' is-active' : ''}`}
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            title={tab.title}
            onClick={() => onSelect(tab.documentId)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onSelect(tab.documentId);
              } else if (
                event.key === 'Delete' ||
                (event.key === 'w' && (event.ctrlKey || event.metaKey))
              ) {
                event.preventDefault();
                onClose(tab.documentId);
              }
            }}
          >
            {tab.modified ? <span className="dot-modified" aria-hidden="true" /> : null}
            <span className="tab-label">{tab.title}</span>
            <button
              type="button"
              className="tab-close"
              aria-label={closeLabel}
              title={closeLabel}
              onClick={(event) => close(event, tab.documentId)}
            >
              <CloseIcon />
            </button>
          </div>
        );
      })}
    </div>
  );
}
