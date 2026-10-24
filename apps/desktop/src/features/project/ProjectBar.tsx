/**
 * Project switcher and trust control (UI.md §5.3, spec decision D3).
 *
 * Projects are opened by absolute path: the kernel records them untrusted,
 * so this bar is also where the user grants or revokes trust.
 */
import { useState, type FormEvent, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** Props for `ProjectBar`. */
export interface ProjectBarProps {
  projects: readonly { id: string; name: string; path: string; trust: 'trusted' | 'untrusted' }[];
  active: { id: string; name: string; path: string; trust: 'trusted' | 'untrusted' } | null;
  onSelect: (id: string) => void;
  onOpen: (path: string) => void;
  onTrust: (trusted: boolean) => void;
  error?: string | null | undefined;
}

/** The project switcher, folder opener, and trust button. */
export function ProjectBar({
  projects,
  active,
  onSelect,
  onOpen,
  onTrust,
  error,
}: ProjectBarProps): ReactElement {
  const [path, setPath] = useState('');
  const trusted = active?.trust === 'trusted';

  const submit = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    onOpen(path);
    setPath('');
  };

  return (
    <div className="project-bar">
      <div className="panel-title">{t('ide.project.label')}</div>
      {projects.length > 0 ? (
        <select
          className="select"
          aria-label={t('ide.project.label')}
          value={active?.id ?? ''}
          onChange={(event) => onSelect(event.target.value)}
        >
          {projects.map((project) => (
            <option key={project.id} value={project.id}>
              {project.name}
            </option>
          ))}
        </select>
      ) : (
        <span className="t-caption">{t('ide.project.none')}</span>
      )}

      <form className="project-bar__row" onSubmit={submit}>
        <input
          className="input"
          type="text"
          aria-label={t('ide.project.openPlaceholder')}
          placeholder={t('ide.project.openPlaceholder')}
          value={path}
          onChange={(event) => setPath(event.target.value)}
        />
        <button className="btn btn-sm" type="submit" disabled={path.trim() === ''}>
          {t('ide.project.open')}
        </button>
      </form>

      {active ? (
        <>
          <div className="project-bar__row">
            <span className={`badge ${trusted ? 'badge-green' : 'badge-yellow'}`}>
              <span className="dot" />
              {t(trusted ? 'ide.project.trusted' : 'ide.project.untrusted')}
            </span>
            <span className="t-mono project-bar__path" title={active.path}>
              {active.name}
            </span>
          </div>
          {trusted ? null : (
            <>
              <p className="t-caption">{t('ide.project.untrusted.desc')}</p>
              <button
                className="btn btn-sm btn-primary"
                type="button"
                onClick={() => onTrust(true)}
              >
                {t('ide.project.trust')}
              </button>
            </>
          )}
        </>
      ) : null}

      {error ? <div className="inline-feedback error">{error}</div> : null}
    </div>
  );
}
