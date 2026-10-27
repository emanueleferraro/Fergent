/**
 * The skill installer: inspect a `SKILL.md` folder or a `.fergskill` archive,
 * read the review, then approve. The skill source is sent again on approve.
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { ReviewCard } from '../plugins/ReviewCard';

import type { SkillView } from './model';
import { useSkillInstall, type SkillSourceInput } from './useSkillInstall';

type SourceKind = SkillSourceInput['kind'];

const KINDS: { id: SourceKind; label: Parameters<typeof t>[0] }[] = [
  { id: 'file', label: 'plugins.install.kind.file' },
  { id: 'dir', label: 'plugins.install.kind.dir' },
];

/** Props for `SkillInstallPanel`. */
export interface SkillInstallPanelProps {
  kernel: KernelClient;
  onInstalled(skill: SkillView): void;
  onClose(): void;
}

/** Install a skill from a folder or an archive. */
export function SkillInstallPanel({
  kernel,
  onInstalled,
  onClose,
}: SkillInstallPanelProps): ReactElement {
  const installer = useSkillInstall(kernel);
  const [kind, setKind] = useState<SourceKind>('dir');
  const [path, setPath] = useState('');
  const [installed, setInstalled] = useState(false);

  const approve = async (): Promise<void> => {
    const skill = await installer.approve();
    if (skill !== null) {
      setInstalled(true);
      onInstalled(skill);
    }
  };

  return (
    <section className="plugin-section" aria-label={t('skills.install.title')}>
      <div className="plugin-section__title">{t('skills.install.title')}</div>

      <div className="plugin-toolbar">
        <div className="plugin-toolbar__group">
          <label className="field">
            <span className="field-label">{t('plugins.install.kind')}</span>
            <select
              className="select"
              value={kind}
              onChange={(event) => setKind(event.target.value as SourceKind)}
            >
              {KINDS.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {t(entry.label)}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span className="field-label">{t('skills.install.path')}</span>
            <input
              className="input"
              value={path}
              placeholder={t('skills.install.path.placeholder')}
              onChange={(event) => setPath(event.target.value)}
            />
          </label>
        </div>
        <div className="plugin-toolbar__group">
          <button
            type="button"
            className="btn btn-sm"
            disabled={installer.inspecting || path.trim() === ''}
            onClick={() => void installer.inspect({ kind, path: path.trim() })}
          >
            {installer.inspecting ? t('skills.install.inspecting') : t('skills.install.inspect')}
          </button>
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>
            {t('plugins.install.close')}
          </button>
        </div>
      </div>

      {installer.error ? <div className="inline-feedback error">{installer.error}</div> : null}
      {installed ? (
        <div className="inline-feedback success">{t('skills.install.success')}</div>
      ) : null}

      {installer.review !== null ? (
        <ReviewCard
          review={installer.review}
          busy={installer.installing}
          onApprove={() => void approve()}
          onCancel={installer.reset}
        />
      ) : null}
    </section>
  );
}
