/**
 * The Settings section shell (Task 2.12): tabs for the per-brain settings and
 * the kernel feature toggles.
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { BrainSettings } from './BrainSettings';
import { FeaturesSettings } from './FeaturesSettings';

type SettingsTab = 'brain' | 'features';

/** Props for `SettingsView`. */
export interface SettingsViewProps {
  kernel: KernelClient;
}

/** The Settings section. */
export function SettingsView({ kernel }: SettingsViewProps): ReactElement {
  const [tab, setTab] = useState<SettingsTab>('brain');

  return (
    <div className="settings-view">
      <div className="tabs" role="tablist" aria-label={t('settings.tabs.label')}>
        <button
          type="button"
          role="tab"
          className={tab === 'brain' ? 'tab active' : 'tab'}
          aria-selected={tab === 'brain'}
          onClick={() => setTab('brain')}
        >
          {t('settings.tabs.brain')}
        </button>
        <button
          type="button"
          role="tab"
          className={tab === 'features' ? 'tab active' : 'tab'}
          aria-selected={tab === 'features'}
          onClick={() => setTab('features')}
        >
          {t('settings.tabs.features')}
        </button>
      </div>
      {tab === 'brain' ? <BrainSettings kernel={kernel} /> : <FeaturesSettings kernel={kernel} />}
    </div>
  );
}
