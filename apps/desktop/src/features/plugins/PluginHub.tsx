/**
 * The plugin section in the sidebar navigation: marketplace and dev panel.
 *
 * Settings → Plugins remains the place where installed plugins are managed;
 * this hub is the discovery and authoring surface.
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { MarketplaceView } from './MarketplaceView';
import { PluginDevPanel } from './PluginDevPanel';

/** The two hub tabs. */
export type PluginTab = 'marketplace' | 'dev';

/** Props for `PluginHub`. */
export interface PluginHubProps {
  kernel: KernelClient;
  /** Controlled active tab; omit to let the hub own the state. */
  tab?: PluginTab;
  onTabChange?(tab: PluginTab): void;
}

/** The marketplace browser plus the plugin dev panel. */
export function PluginHub({ kernel, tab, onTabChange }: PluginHubProps): ReactElement {
  const [localTab, setLocalTab] = useState<PluginTab>('marketplace');
  const active = tab ?? localTab;

  const select = (next: PluginTab): void => {
    setLocalTab(next);
    onTabChange?.(next);
  };

  return (
    <div className="plugin-view">
      <div className="tabs" role="tablist" aria-label={t('rail.plugins')}>
        <button
          type="button"
          role="tab"
          className={active === 'marketplace' ? 'tab active' : 'tab'}
          aria-selected={active === 'marketplace'}
          onClick={() => select('marketplace')}
        >
          {t('marketplace.title')}
        </button>
        <button
          type="button"
          role="tab"
          className={active === 'dev' ? 'tab active' : 'tab'}
          aria-selected={active === 'dev'}
          onClick={() => select('dev')}
        >
          {t('factory.title')}
        </button>
      </div>
      {active === 'marketplace' ? <MarketplaceView kernel={kernel} /> : null}
      {active === 'dev' ? <PluginDevPanel kernel={kernel} /> : null}
    </div>
  );
}
