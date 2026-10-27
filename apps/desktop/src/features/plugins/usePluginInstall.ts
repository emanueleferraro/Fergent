/**
 * The two-phase plugin install (spec §6): inspect → review → install.
 *
 * The review is valid for ten minutes in the kernel; the UI never installs
 * without an explicit approval of the declared permission subset.
 */
import { useCallback, useState } from 'react';

import type { KernelClient } from '../../lib/kernel';

import type { InstallReview, PluginView } from './model';

/** Where an install comes from (`PluginSource::from_json`). */
export interface PluginSourceInput {
  kind: 'file' | 'dir' | 'git';
  path?: string;
  url?: string;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The state returned by {@link usePluginInstall}. */
export interface PluginInstallController {
  review: InstallReview | null;
  inspecting: boolean;
  installing: boolean;
  error: string | null;
  inspect(source: PluginSourceInput): Promise<void>;
  approve(permissions: string[]): Promise<PluginView | null>;
  reset(): void;
}

/** Inspect and install a plugin. */
export function usePluginInstall(kernel: KernelClient): PluginInstallController {
  const [review, setReview] = useState<InstallReview | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const inspect = useCallback(
    async (source: PluginSourceInput): Promise<void> => {
      setInspecting(true);
      setError(null);
      setReview(null);
      try {
        const result = await kernel.call<InstallReview>('plugin.inspect', { source });
        setReview(result);
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setInspecting(false);
      }
    },
    [kernel],
  );

  const approve = useCallback(
    async (permissions: string[]): Promise<PluginView | null> => {
      if (review === null) {
        setError('No install review is open.');
        return null;
      }
      setInstalling(true);
      setError(null);
      try {
        const view = await kernel.call<PluginView>('plugin.install', {
          review_id: review.review_id,
          permissions,
        });
        setReview(null);
        return view;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
        return null;
      } finally {
        setInstalling(false);
      }
    },
    [kernel, review],
  );

  const reset = useCallback((): void => {
    setReview(null);
    setError(null);
  }, []);

  return { review, inspecting, installing, error, inspect, approve, reset };
}
