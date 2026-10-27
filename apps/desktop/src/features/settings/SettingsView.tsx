/**
 * The Settings section shell (Tasks 2.12, 2.17, 4.6): per-brain settings,
 * feature toggles, plugins and skills, and the provider / secret / MCP
 * managers.
 */
import { useState, type ReactElement } from 'react';

import type { MessageKey } from '../../i18n/en';
import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { PluginsView } from '../plugins/PluginsView';
import { SkillsView } from '../skills/SkillsView';

import { BrainSettings } from './BrainSettings';
import { FeaturesSettings } from './FeaturesSettings';
import { IdeSettings } from './IdeSettings';
import { McpSettings } from './McpSettings';
import { ProvidersSettings } from './ProvidersSettings';
import { SecretsSettings } from './SecretsSettings';

/** The Settings tabs, including the Phase 4 plugin and skill managers. */
export type SettingsTab =
  'brain' | 'features' | 'plugins' | 'skills' | 'providers' | 'secrets' | 'mcp' | 'ide';

const TABS: { id: SettingsTab; label: MessageKey }[] = [
  { id: 'brain', label: 'settings.tabs.brain' },
  { id: 'features', label: 'settings.tabs.features' },
  { id: 'plugins', label: 'settings.tabs.plugins' },
  { id: 'skills', label: 'settings.tabs.skills' },
  { id: 'providers', label: 'settings.tabs.providers' },
  { id: 'secrets', label: 'settings.tabs.secrets' },
  { id: 'mcp', label: 'settings.tabs.mcp' },
  { id: 'ide', label: 'settings.tabs.ide' },
];

/** Props for `SettingsView`. */
export interface SettingsViewProps {
  kernel: KernelClient;
  /** Controlled active tab; omit to let the view own the state. */
  tab?: SettingsTab;
  onTabChange?(tab: SettingsTab): void;
}

/** The Settings section. */
export function SettingsView({ kernel, tab, onTabChange }: SettingsViewProps): ReactElement {
  const [localTab, setLocalTab] = useState<SettingsTab>('brain');
  const active = tab ?? localTab;

  const select = (next: SettingsTab): void => {
    setLocalTab(next);
    onTabChange?.(next);
  };

  return (
    <div className="settings-view">
      <div className="tabs" role="tablist" aria-label={t('settings.tabs.label')}>
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            className={active === item.id ? 'tab active' : 'tab'}
            aria-selected={active === item.id}
            onClick={() => select(item.id)}
          >
            {t(item.label)}
          </button>
        ))}
      </div>
      {active === 'brain' ? <BrainSettings kernel={kernel} /> : null}
      {active === 'features' ? <FeaturesSettings kernel={kernel} /> : null}
      {active === 'plugins' ? <PluginsView kernel={kernel} /> : null}
      {active === 'skills' ? <SkillsView kernel={kernel} /> : null}
      {active === 'providers' ? <ProvidersSettings kernel={kernel} /> : null}
      {active === 'secrets' ? <SecretsSettings kernel={kernel} /> : null}
      {active === 'mcp' ? <McpSettings kernel={kernel} /> : null}
      {active === 'ide' ? <IdeSettings kernel={kernel} /> : null}
    </div>
  );
}
