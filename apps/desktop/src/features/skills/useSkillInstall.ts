/**
 * Skill install: inspect a `SKILL.md` folder/archive, then approve.
 *
 * Unlike plugins, `skill.install` takes the source again plus a boolean
 * approval; the review is informational.
 */
import { useCallback, useState } from 'react';

import type { KernelClient } from '../../lib/kernel';

import type { SkillReview, SkillView } from './model';

/** Where a skill install comes from (`SkillSource::from_json`). */
export interface SkillSourceInput {
  kind: 'file' | 'dir';
  path: string;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The state returned by {@link useSkillInstall}. */
export interface SkillInstallController {
  review: SkillReview | null;
  source: SkillSourceInput | null;
  inspecting: boolean;
  installing: boolean;
  error: string | null;
  inspect(source: SkillSourceInput): Promise<void>;
  approve(): Promise<SkillView | null>;
  reset(): void;
}

/** Inspect and install a skill. */
export function useSkillInstall(kernel: KernelClient): SkillInstallController {
  const [review, setReview] = useState<SkillReview | null>(null);
  const [source, setSource] = useState<SkillSourceInput | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const inspect = useCallback(
    async (next: SkillSourceInput): Promise<void> => {
      setInspecting(true);
      setError(null);
      setReview(null);
      try {
        const result = await kernel.call<SkillReview>('skill.inspect', { source: next });
        setReview(result);
        setSource(next);
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setInspecting(false);
      }
    },
    [kernel],
  );

  const approve = useCallback(async (): Promise<SkillView | null> => {
    if (source === null) {
      setError('No skill review is open.');
      return null;
    }
    setInstalling(true);
    setError(null);
    try {
      const view = await kernel.call<SkillView>('skill.install', {
        source,
        approve: true,
      });
      setReview(null);
      setSource(null);
      return view;
    } catch (cause) {
      setError(messageOf(cause));
      return null;
    } finally {
      setInstalling(false);
    }
  }, [kernel, source]);

  const reset = useCallback((): void => {
    setReview(null);
    setSource(null);
    setError(null);
  }, []);

  return { review, source, inspecting, installing, error, inspect, approve, reset };
}
