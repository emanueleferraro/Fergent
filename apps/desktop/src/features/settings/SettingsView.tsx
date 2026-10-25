/**
 * The Settings section shell (Tasks 2.12, 2.17): per-brain settings, feature
 * toggles, and the provider / secret / MCP managers.
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { BrainSettings } from './BrainSettings';
import { FeaturesSettings } from './FeaturesSettings';
import { IdeSettings } from './IdeSettings';
import { McpSettings } from './McpSettings';
import { ProvidersSettings } from './ProvidersSettings';
import { SecretsSettings } from './SecretsSettings';

type SettingsTab = 'brain' | 'features' | 'providers' | 'secrets' | 'mcp' | 'ide';

const TABS: { id: SettingsTab; label: Parameters<typeof t>[0] }[] = [
  { id: 'brain', label: 'settings.tabs.brain' },
  { id: 'features', label: 'settings.tabs.features' },
  { id: 'providers', label: 'settings.tabs.providers' },
  { id: 'secrets', label: 'settings.tabs.secrets' },
  { id: 'mcp', label: 'settings.tabs.mcp' },
  { id: 'ide', label: 'settings.tabs.ide' },
];

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
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            className={tab === item.id ? 'tab active' : 'tab'}
            aria-selected={tab === item.id}
            onClick={() => setTab(item.id)}
          >
            {t(item.label)}
          </button>
        ))}
      </div>
      {tab === 'brain' ? <BrainSettings kernel={kernel} /> : null}
      {tab === 'features' ? <FeaturesSettings kernel={kernel} /> : null}
      {tab === 'providers' ? <ProvidersSettings kernel={kernel} /> : null}
      {tab === 'secrets' ? <SecretsSettings kernel={kernel} /> : null}
      {tab === 'mcp' ? <McpSettings kernel={kernel} /> : null}
      {tab === 'ide' ? <IdeSettings kernel={kernel} /> : null}
    </div>
  );
}
