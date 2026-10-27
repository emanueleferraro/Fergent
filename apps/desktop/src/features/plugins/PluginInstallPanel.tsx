/**
 * The plugin installer: pick a source, inspect it, review the permissions, and
 * approve. The kernel never installs without the review round-trip.
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import type { PluginView } from './model';
import { ReviewCard } from './ReviewCard';
import { usePluginInstall, type PluginSourceInput } from './usePluginInstall';

type SourceKind = PluginSourceInput['kind'];

const KINDS: { id: SourceKind; label: Parameters<typeof t>[0] }[] = [
  { id: 'file', label: 'plugins.install.kind.file' },
  { id: 'dir', label: 'plugins.install.kind.dir' },
  { id: 'git', label: 'plugins.install.kind.git' },
];

/** Props for `PluginInstallPanel`. */
export interface PluginInstallPanelProps {
  kernel: KernelClient;
  onInstalled(plugin: PluginView): void;
  onClose(): void;
}

/** Install a plugin from a file, a folder, or a git repository. */
export function PluginInstallPanel({
  kernel,
  onInstalled,
  onClose,
}: PluginInstallPanelProps): ReactElement {
  const installer = usePluginInstall(kernel);
  const [kind, setKind] = useState<SourceKind>('dir');
  const [value, setValue] = useState('');
  const [installed, setInstalled] = useState(false);

  const inspect = (): void => {
    const source: PluginSourceInput = { kind };
    if (kind === 'git') {
      source.url = value.trim();
    } else {
      source.path = value.trim();
    }
    void installer.inspect(source);
  };

  const approve = async (permissions: string[]): Promise<void> => {
    const plugin = await installer.approve(permissions);
    if (plugin !== null) {
      setInstalled(true);
      onInstalled(plugin);
    }
  };

  return (
    <section className="plugin-section" aria-label={t('plugins.install.title')}>
      <div className="plugin-section__title">{t('plugins.install.title')}</div>

      <div className="plugin-toolbar">
        <div className="plugin-toolbar__group">
          <label className="field">
            <span className="field-label">{t('plugins.install.kind')}</span>
            <select
              className="select"
              value={kind}
              onChange={(event) => {
                setKind(event.target.value as SourceKind);
                setValue('');
              }}
            >
              {KINDS.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {t(entry.label)}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span className="field-label">
              {kind === 'git' ? t('plugins.install.url') : t('plugins.install.path')}
            </span>
            <input
              className="input"
              value={value}
              placeholder={
                kind === 'git'
                  ? t('plugins.install.url.placeholder')
                  : t('plugins.install.path.placeholder')
              }
              onChange={(event) => setValue(event.target.value)}
            />
          </label>
        </div>
        <div className="plugin-toolbar__group">
          <button
            type="button"
            className="btn btn-sm"
            disabled={installer.inspecting || value.trim() === ''}
            onClick={inspect}
          >
            {installer.inspecting ? t('plugins.install.inspecting') : t('plugins.install.inspect')}
          </button>
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>
            {t('plugins.install.close')}
          </button>
        </div>
      </div>

      {installer.error ? <div className="inline-feedback error">{installer.error}</div> : null}
      {installed ? (
        <div className="inline-feedback success">{t('plugins.install.success')}</div>
      ) : null}

      {installer.review !== null ? (
        <ReviewCard
          review={installer.review}
          busy={installer.installing}
          onApprove={(permissions) => void approve(permissions)}
          onCancel={installer.reset}
        />
      ) : null}
    </section>
  );
}
