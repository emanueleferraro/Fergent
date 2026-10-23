/**
 * Model selector (Task 2.18): pins the model a session runs on.
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** Props for `ModelSelector`. */
export interface ModelSelectorProps {
  /** Currently pinned model (`null` means the kernel default). */
  value: string | null;
  /** Models offered by the configured providers. */
  models: string[];
  onChange: (model: string | null) => void;
}

/** A `<select>` of the available models plus the default option. */
export function ModelSelector({ value, models, onChange }: ModelSelectorProps): ReactElement {
  return (
    <select
      className="select"
      aria-label={t('chat.model.label')}
      value={value ?? ''}
      onChange={(event) => onChange(event.target.value === '' ? null : event.target.value)}
    >
      <option value="">{t('chat.model.default')}</option>
      {models.map((model) => (
        <option key={model} value={model}>
          {model}
        </option>
      ))}
      {value && !models.includes(value) ? <option value={value}>{value}</option> : null}
    </select>
  );
}
