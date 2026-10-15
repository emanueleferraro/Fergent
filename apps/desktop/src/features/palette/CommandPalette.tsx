/**
 * Ctrl+K command palette (`#cmdk` pattern from `style/patterns.html`).
 *
 * Keyboard accessible: ArrowUp/ArrowDown move the selection, Enter runs the
 * active command, Escape closes. Runs anywhere on the page while open.
 */
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { filterCommands, type Command } from './commands';

/** Props for the palette. */
export interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  kernel: KernelClient;
  commands: readonly Command[];
  toggleTheme: () => void;
}

/** The Ctrl+K command palette. */
export function CommandPalette({
  open,
  onClose,
  kernel,
  commands,
  toggleTheme,
}: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const matches = useMemo(() => filterCommands(commands, query, t), [commands, query]);

  useEffect(() => {
    if (open) {
      setQuery('');
      setActive(0);
      inputRef.current?.focus();
    }
  }, [open]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  if (!open) {
    return null;
  }

  const runCommand = (command: Command | undefined): void => {
    if (!command) {
      return;
    }
    void Promise.resolve(command.run({ kernel, toggleTheme })).catch(() => undefined);
    onClose();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>): void => {
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        setActive((index) => Math.min(index + 1, matches.length - 1));
        break;
      case 'ArrowUp':
        event.preventDefault();
        setActive((index) => Math.max(index - 1, 0));
        break;
      case 'Enter':
        event.preventDefault();
        runCommand(matches[active]);
        break;
      case 'Escape':
        event.preventDefault();
        onClose();
        break;
      default:
        break;
    }
  };

  return (
    <div className="palette-overlay" onClick={onClose}>
      <div className="cmdk-overlay" onClick={(event) => event.stopPropagation()}>
        <div className="cmdk" role="dialog" aria-modal="true" aria-label={t('palette.label')}>
          <div className="cmdk-input-row">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="11" cy="11" r="7" />
              <path d="M20 20l-3.5-3.5" />
            </svg>
            <input
              ref={inputRef}
              className="cmdk-input"
              type="text"
              value={query}
              aria-label={t('palette.label')}
              placeholder={t('palette.placeholder')}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={onKeyDown}
            />
            <span className="cmdk-kbd">{t('palette.hint')}</span>
          </div>
          <div className="cmdk-list palette-list" role="listbox">
            {matches.length === 0 ? (
              <div className="cmdk-group">{t('palette.empty')}</div>
            ) : (
              matches.map((command, index) => (
                <div
                  key={command.id}
                  role="option"
                  aria-selected={index === active}
                  className={index === active ? 'cmdk-item active' : 'cmdk-item'}
                  onMouseEnter={() => setActive(index)}
                  onClick={() => runCommand(command)}
                >
                  <span className="ic">
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M5 12l4 4 10-10" />
                    </svg>
                  </span>
                  <span>{t(command.titleKey)}</span>
                  {command.shortcut ? (
                    <span className="kbd">
                      <span>{command.shortcut}</span>
                    </span>
                  ) : null}
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
