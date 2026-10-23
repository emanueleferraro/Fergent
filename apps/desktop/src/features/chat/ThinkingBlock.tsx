/**
 * A collapsible thinking block (UI.md §7.3).
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** Props for `ThinkingBlock`. */
export interface ThinkingBlockProps {
  text: string;
}

/** Renders a reasoning model's thinking, collapsed by default. */
export function ThinkingBlock({ text }: ThinkingBlockProps): ReactElement {
  return (
    <details className="msg-thinking">
      <summary>{t('chat.thinking')}</summary>
      <p>{text}</p>
    </details>
  );
}
