/**
 * A multiple-choice question driven by the `ask_choice` tool call: A/B/C
 * buttons plus an "Other" free-text field (Task 2.13).
 */
import { useId, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';

import type { ChoicePrompt as Choice } from './model';

/** Props for `ChoicePrompt`. */
export interface ChoicePromptProps {
  choice: Choice;
  disabled: boolean;
  onAnswer: (answer: string) => void;
}

/** Renders a question with lettered options and an other-field. */
export function ChoicePrompt({ choice, disabled, onAnswer }: ChoicePromptProps): ReactElement {
  const [other, setOther] = useState('');
  const otherId = useId();

  return (
    <div className="msg-choice" role="group" aria-label={t('chat.choice.label')}>
      <div className="msg-choice__question">{choice.question}</div>
      <div className="msg-choice__options">
        {choice.options.map((option, index) => (
          <button
            key={`${option.value}-${index}`}
            type="button"
            className="btn btn-sm"
            disabled={disabled}
            onClick={() => onAnswer(option.value)}
          >
            <span className="msg-choice__letter">{String.fromCharCode(65 + index)}</span>
            {option.label}
          </button>
        ))}
      </div>
      <div className="msg-choice__other">
        <label className="settings-hint" htmlFor={otherId}>
          {t('chat.choice.other')}
        </label>
        <input
          id={otherId}
          className="input"
          type="text"
          value={other}
          disabled={disabled}
          onChange={(event) => setOther(event.target.value)}
        />
        <button
          type="button"
          className="btn btn-primary btn-sm"
          disabled={disabled || other.trim().length === 0}
          onClick={() => onAnswer(other.trim())}
        >
          {t('chat.choice.send')}
        </button>
      </div>
    </div>
  );
}
