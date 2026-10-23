/**
 * An inline tool-call card (UI.md §7.4): name, status, collapsible arguments.
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

import type { ToolCallView } from './model';

/** Props for `ToolCard`. */
export interface ToolCardProps {
  call: ToolCallView;
}

function renderArguments(value: unknown): string {
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/** One tool invocation. */
export function ToolCard({ call }: ToolCardProps): ReactElement {
  const failed = call.error !== null;
  return (
    <div className="tool-card">
      <div className="tool-card__head">
        <span className="tool-card__name t-mono">{call.name}</span>
        <span className={`badge ${failed ? 'badge-red' : 'badge-green'}`}>
          {failed ? t('chat.tool.error') : t('chat.tool.ok')}
        </span>
      </div>
      <details className="tool-card__args">
        <summary>{t('chat.tool.arguments')}</summary>
        <pre>{renderArguments(call.arguments)}</pre>
      </details>
      {failed ? <div className="tool-card__error">{call.error}</div> : null}
    </div>
  );
}
