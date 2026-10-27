/**
 * The marketplace browser (`marketplace.*`, spec §7).
 *
 * Marketplace installs never complete on their own: the kernel returns the
 * package's install review, and the user approves it from the UI.
 */
import { useCallback, useEffect, useState } from 'react';

import type { KernelClient } from '../../lib/kernel';
import type { SkillReview } from '../skills/model';

import type { InstallReview } from './model';

/** The download descriptor of one entry. */
export interface MarketDownload {
  url: string;
  sha256: string;
  size: number | null;
}

/** One published entry. */
export interface MarketEntry {
  id: string;
  kind: string;
  name: string;
  version: string;
  api_version: number;
  description: string | null;
  tags: string[];
  author: string | null;
  homepage: string | null;
  min_kernel: string | null;
  download: MarketDownload;
  signature: string | null;
  published_at: string | null;
  yanked: boolean;
}

/** A cached entry as returned by `marketplace.search`. */
export interface CachedEntry extends MarketEntry {
  source_id: string;
  fetched_at: number;
  stale: boolean;
}

/** A newer version available for an installed package. */
export interface UpdateInfo {
  kind: string;
  id: string;
  installed: string;
  available: string;
}

/** One source's refresh outcome. */
export interface RefreshReport {
  source_id: string;
  entries: number;
  signature: string;
  error: string | null;
}

/** The result of `marketplace.install`: a review, never an install. */
export interface MarketplaceInstallResult {
  kind: 'plugin' | 'skill';
  requires_approval: boolean;
  entry: CachedEntry;
  review: InstallReview | SkillReview;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The state returned by {@link useMarketplace}. */
export interface MarketplaceController {
  loading: boolean;
  error: string | null;
  entries: CachedEntry[];
  refreshing: boolean;
  pending: string | null;
  search(query?: string, kind?: string): Promise<void>;
  refresh(): Promise<void>;
  checkUpdates(): Promise<UpdateInfo[]>;
  install(entry: CachedEntry): Promise<MarketplaceInstallResult | null>;
}

/** The marketplace index. */
export function useMarketplace(kernel: KernelClient): MarketplaceController {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [entries, setEntries] = useState<CachedEntry[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [pending, setPending] = useState<string | null>(null);

  const search = useCallback(
    async (query?: string, kind?: string): Promise<void> => {
      setPending('search');
      try {
        const params: Record<string, unknown> = {};
        if (query !== undefined && query.trim() !== '') {
          params.query = query.trim();
        }
        if (kind !== undefined && kind !== '') {
          params.kind = kind;
        }
        const result = await kernel.call<{ entries: CachedEntry[] }>('marketplace.search', params);
        setEntries(result.entries);
        setError(null);
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setLoading(false);
        setPending(null);
      }
    },
    [kernel],
  );

  useEffect(() => {
    void search();
  }, [search]);

  const refresh = useCallback(async (): Promise<void> => {
    setRefreshing(true);
    setError(null);
    try {
      await kernel.call<{ refreshed: RefreshReport[] }>('marketplace.refresh');
      await search();
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setRefreshing(false);
    }
  }, [kernel, search]);

  const checkUpdates = useCallback(async (): Promise<UpdateInfo[]> => {
    try {
      const result = await kernel.call<{ updates: UpdateInfo[] }>('marketplace.check_updates');
      return result.updates;
    } catch (cause) {
      setError(messageOf(cause));
      return [];
    }
  }, [kernel]);

  const install = useCallback(
    async (entry: CachedEntry): Promise<MarketplaceInstallResult | null> => {
      setPending(entry.id);
      setError(null);
      try {
        return await kernel.call<MarketplaceInstallResult>('marketplace.install', {
          source_id: entry.source_id,
          kind: entry.kind,
          id: entry.id,
        });
      } catch (cause) {
        setError(messageOf(cause));
        return null;
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  return { loading, error, entries, refreshing, pending, search, refresh, checkUpdates, install };
}
