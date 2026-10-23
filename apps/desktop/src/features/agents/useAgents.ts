/**
 * The Agent Builder controller (Task 2.16): list, create, duplicate, and
 * delete agent definitions through the kernel.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import type { AgentDefinition, AgentDeletedResult, AgentListResult } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import {
  draftFrom,
  draftToMarkdown,
  duplicateDraft,
  emptyDraft,
  validateDraft,
  type AgentDraft,
  type DraftErrors,
} from './model';

/** Everything the Agents view needs. */
export interface AgentsController {
  loading: boolean;
  error: string | null;
  agents: AgentDefinition[];
  selectedId: string | null;
  draft: AgentDraft;
  errors: DraftErrors;
  saving: boolean;
  notice: string | null;
  isNew: boolean;
  edit: (draft: AgentDraft) => void;
  select: (id: string) => void;
  startNew: () => void;
  duplicate: () => void;
  save: () => Promise<void>;
  remove: (id: string) => Promise<void>;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Load and edit agent definitions. */
export function useAgents(kernel: KernelClient): AgentsController {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<AgentDraft>(emptyDraft);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const result = await kernel.call<AgentListResult>('agent.list');
        if (!active) {
          return;
        }
        setAgents(result.agents);
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
  }, [kernel]);

  const reload = useCallback(async (): Promise<AgentDefinition[]> => {
    const result = await kernel.call<AgentListResult>('agent.list');
    setAgents(result.agents);
    return result.agents;
  }, [kernel]);

  const edit = useCallback((next: AgentDraft) => {
    setDraft(next);
    setNotice(null);
  }, []);

  const select = useCallback(
    (id: string) => {
      const definition = agents.find((agent) => agent.id === id);
      if (!definition) {
        return;
      }
      setSelectedId(id);
      setDraft(draftFrom(definition));
      setNotice(null);
    },
    [agents],
  );

  const startNew = useCallback(() => {
    setSelectedId(null);
    setDraft(emptyDraft());
    setNotice(null);
  }, []);

  const duplicate = useCallback(() => {
    const taken = agents.map((agent) => agent.id);
    setDraft(duplicateDraft(draft, taken));
    setSelectedId(null);
    setNotice(null);
  }, [agents, draft]);

  const save = useCallback(async (): Promise<void> => {
    if (Object.keys(validateDraft(draft)).length > 0) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const created = await kernel.call<AgentDefinition>('agent.create', {
        definition: draftToMarkdown(draft),
      });
      await reload();
      setSelectedId(created.id);
      setDraft(draftFrom(created));
      setNotice('saved');
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setSaving(false);
    }
  }, [kernel, draft, reload]);

  const remove = useCallback(
    async (id: string): Promise<void> => {
      setError(null);
      try {
        await kernel.call<AgentDeletedResult>('agent.delete', { id });
        const remaining = await reload();
        if (selectedId === id) {
          setSelectedId(null);
          setDraft(remaining[0] ? draftFrom(remaining[0]) : emptyDraft());
        }
        setNotice('deleted');
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel, reload, selectedId],
  );

  const errors = useMemo(() => validateDraft(draft), [draft]);

  return {
    loading,
    error,
    agents,
    selectedId,
    draft,
    errors,
    saving,
    notice,
    isNew: selectedId === null,
    edit,
    select,
    startNew,
    duplicate,
    save,
    remove,
  };
}
