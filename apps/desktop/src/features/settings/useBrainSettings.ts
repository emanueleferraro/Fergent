/**
 * Loads a brain's configuration (`brain.get`) and saves the editable identity
 * through `brain.update`. Phase 1 only exposes name/color/icon as mutable.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import type { BrainConfig, BrainGetResult, BrainListResult, BrainState } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import {
  draftFrom,
  hasErrors,
  toUpdateParams,
  validateDraft,
  type DraftErrors,
  type IdentityDraft,
} from './model';

const EMPTY_DRAFT: IdentityDraft = { name: '', color: '', icon: '' };

/** The state returned by {@link useBrainSettings}. */
export interface BrainSettingsState {
  loading: boolean;
  error: string | null;
  brainId: string | null;
  state: BrainState | null;
  config: BrainConfig | null;
  draft: IdentityDraft;
  errors: DraftErrors;
  dirty: boolean;
  saving: boolean;
  saveError: string | null;
  saved: boolean;
  setDraft: (draft: IdentityDraft) => void;
  reset: () => void;
  save: () => Promise<void>;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Load and edit the settings of a brain (the first active one by default). */
export function useBrainSettings(
  kernel: KernelClient,
  brainId?: string | undefined,
): BrainSettingsState {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [id, setId] = useState<string | null>(brainId ?? null);
  const [state, setState] = useState<BrainState | null>(null);
  const [config, setConfig] = useState<BrainConfig | null>(null);
  const [original, setOriginal] = useState<IdentityDraft>(EMPTY_DRAFT);
  const [draft, setDraftState] = useState<IdentityDraft>(EMPTY_DRAFT);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void (async () => {
      try {
        let target = brainId ?? null;
        if (!target) {
          const list = await kernel.call<BrainListResult>('brain.list');
          target = list.brains[0]?.brain_id ?? null;
        }
        if (!active) {
          return;
        }
        if (!target) {
          setId(null);
          setState(null);
          setConfig(null);
          setOriginal(EMPTY_DRAFT);
          setDraftState(EMPTY_DRAFT);
          setError(null);
          return;
        }
        const result = await kernel.call<BrainGetResult>('brain.get', { brain_id: target });
        if (!active) {
          return;
        }
        const next = draftFrom(result.config);
        setId(target);
        setState(result.state);
        setConfig(result.config);
        setOriginal(next);
        setDraftState(next);
        setError(null);
      } catch (cause) {
        if (active) {
          setError(messageOf(cause));
        }
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel, brainId]);

  const errors = useMemo(() => validateDraft(draft), [draft]);
  const dirty = useMemo(
    () => id !== null && toUpdateParams(id, draft, original) !== null,
    [id, draft, original],
  );

  const setDraft = useCallback((next: IdentityDraft) => {
    setDraftState(next);
    setSaved(false);
    setSaveError(null);
  }, []);

  const reset = useCallback(() => {
    setDraftState(original);
    setSaved(false);
    setSaveError(null);
  }, [original]);

  const save = useCallback(async () => {
    if (id === null) {
      return;
    }
    const params = toUpdateParams(id, draft, original);
    if (!params || hasErrors(validateDraft(draft))) {
      return;
    }
    setSaving(true);
    setSaveError(null);
    try {
      await kernel.call('brain.update', params);
      setOriginal({
        name: draft.name.trim(),
        color: draft.color.trim(),
        icon: draft.icon.trim(),
      });
      setSaved(true);
    } catch (cause) {
      setSaveError(messageOf(cause));
    } finally {
      setSaving(false);
    }
  }, [kernel, id, draft, original]);

  return {
    loading,
    error,
    brainId: id,
    state,
    config,
    draft,
    errors,
    dirty,
    saving,
    saveError,
    saved,
    setDraft,
    reset,
    save,
  };
}
