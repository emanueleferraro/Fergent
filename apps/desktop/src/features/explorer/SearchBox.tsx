/**
 * The inline brain search box (UI.md §5.2).
 *
 * Owns its query text and reports submissions on Enter; the parent decides
 * where the query goes (the search panel is Task 19).
 */
import { useRef, useState, type FormEvent, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** Props for `SearchBox`. */
export interface SearchBoxProps {
  /** Initial query text. */
  defaultValue?: string | undefined;
  /** Reported when the user submits (Enter). */
  onSubmit?: ((query: string) => void) | undefined;
  /** Reported on every keystroke with the trimmed query (may be empty). */
  onChange?: ((query: string) => void) | undefined;
  /** Focus the input on mount. */
  autoFocus?: boolean | undefined;
}

/** The inline search box. */
export function SearchBox({
  defaultValue = '',
  onSubmit,
  onChange,
  autoFocus = false,
}: SearchBoxProps): ReactElement {
  const [value, setValue] = useState(defaultValue);
  const inputRef = useRef<HTMLInputElement>(null);

  const update = (next: string): void => {
    setValue(next);
    onChange?.(next.trim());
  };

  const submit = (event: FormEvent): void => {
    event.preventDefault();
    onSubmit?.(value.trim());
  };

  return (
    <form className="input-wrap" role="search" onSubmit={submit}>
      <span className="input-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24">
          <circle cx="11" cy="11" r="6" />
          <path d="M20 20l-3.5-3.5" />
        </svg>
      </span>
      <input
        ref={inputRef}
        className="input"
        type="search"
        value={value}
        placeholder={t('explorer.search.placeholder')}
        aria-label={t('explorer.search.label')}
        autoFocus={autoFocus}
        onChange={(event) => update(event.target.value)}
      />
      {value.length > 0 ? (
        <button
          type="button"
          className="input-suffix"
          aria-label={t('explorer.search.clear')}
          onClick={() => {
            update('');
            inputRef.current?.focus();
          }}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M7 7l10 10M17 7L7 17" />
          </svg>
        </button>
      ) : null}
    </form>
  );
}
