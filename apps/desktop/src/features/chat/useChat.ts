/**
 * The chat controller (Task 2.13): sessions, transcripts, prompts, search,
 * forks, and the send/retry/fork actions used by the sidebar and panel.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type {
  AgentDefinition,
  AgentListResult,
  AttachmentRouting,
  CapabilityListResult,
  ChatMessage,
  ChatMessageListResult,
  ChatMessageSendResult,
  ChatSearchHit,
  ChatSession,
  ChatSessionListResult,
  ChatSessionSearchResult,
  PromptListResult,
  PromptTemplate,
  ProviderListResult,
} from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

/** The canned definition used when the workspace has no agents yet. */
export const ASSISTANT_DEFINITION = [
  '---',
  'id: assistant',
  'name: Assistant',
  'description: General-purpose assistant.',
  'model: mock-1',
  'tools: []',
  '---',
  '',
  'You are a helpful, concise assistant.',
  '',
].join('\n');

/** Everything the chat UI needs. */
export interface ChatController {
  loading: boolean;
  error: string | null;
  agents: AgentDefinition[];
  sessions: ChatSession[];
  activeId: string | null;
  active: ChatSession | null;
  messages: ChatMessage[];
  messagesLoading: boolean;
  sending: boolean;
  actionError: string | null;
  prompts: PromptTemplate[];
  hits: ChatSearchHit[];
  searching: boolean;
  notice: string | null;
  draft: string;
  /** Enabled capability ids (gates rich outputs). */
  capabilities: Set<string>;
  /** Models offered by the configured providers (empty when none are set up). */
  models: string[];
  setDraft: (value: string) => void;
  select: (id: string) => void;
  setModel: (model: string | null) => Promise<void>;
  newSession: () => Promise<void>;
  createAssistant: () => Promise<void>;
  send: (text?: string) => Promise<void>;
  retry: () => Promise<void>;
  fork: () => Promise<void>;
  rename: (id: string, title: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  attach: (name: string, mediaType: string, size: number) => Promise<void>;
  search: (query: string) => Promise<void>;
  clearSearch: () => void;
  usePrompt: (prompt: PromptTemplate) => void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Load and drive the chat surface. */
export function useChat(kernel: KernelClient): ChatController {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [prompts, setPrompts] = useState<PromptTemplate[]>([]);
  const [hits, setHits] = useState<ChatSearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [capabilities, setCapabilities] = useState<Set<string>>(new Set());
  const [models, setModels] = useState<string[]>([]);

  const activeIdRef = useRef(activeId);
  activeIdRef.current = activeId;

  const loadSessions = useCallback(async (): Promise<ChatSession[]> => {
    const result = await kernel.call<ChatSessionListResult>('chat.session.list', { limit: 50 });
    setSessions(result.sessions);
    return result.sessions;
  }, [kernel]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void (async () => {
      try {
        const [agentList, sessionList, promptList, capabilityList] = await Promise.all([
          kernel.call<AgentListResult>('agent.list'),
          kernel.call<ChatSessionListResult>('chat.session.list', { limit: 50 }),
          kernel.call<PromptListResult>('prompt.list'),
          kernel.call<CapabilityListResult | null>('capability.list'),
        ]);
        if (!active) {
          return;
        }
        setAgents(agentList.agents);
        setSessions(sessionList.sessions);
        setPrompts(promptList.prompts);
        setCapabilities(
          new Set(
            (capabilityList?.capabilities ?? [])
              .filter((capability) => capability.enabled)
              .map((capability) => capability.id),
          ),
        );
        setActiveId(sessionList.sessions[0]?.id ?? null);
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

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const result = await kernel.call<ProviderListResult>('provider.list');
        if (active) {
          setModels(result.providers.flatMap((provider) => provider.models));
        }
      } catch {
        // Providers are optional: without them the selector only offers the default.
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel]);

  useEffect(() => {
    if (!activeId) {
      setMessages([]);
      return;
    }
    let active = true;
    setMessagesLoading(true);
    void (async () => {
      try {
        const result = await kernel.call<ChatMessageListResult>('chat.message.list', {
          session_id: activeId,
        });
        if (active) {
          setMessages(result.messages);
        }
      } catch (cause) {
        if (active) {
          setActionError(messageOf(cause));
        }
      } finally {
        if (active) {
          setMessagesLoading(false);
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel, activeId]);

  const select = useCallback((id: string) => {
    setActiveId(id);
    setActionError(null);
    setHits([]);
  }, []);

  const createFor = useCallback(
    async (agentId: string): Promise<void> => {
      const session = await kernel.call<ChatSession>('chat.session.create', {
        agent_id: agentId,
      });
      await loadSessions();
      setMessages([]);
      setActiveId(session.id);
    },
    [kernel, loadSessions],
  );

  const newSession = useCallback(async (): Promise<void> => {
    setActionError(null);
    const agentId = agents[0]?.id;
    if (!agentId) {
      setActionError(null);
      setNotice(null);
      return;
    }
    try {
      await createFor(agentId);
    } catch (cause) {
      setActionError(messageOf(cause));
    }
  }, [agents, createFor]);

  const createAssistant = useCallback(async (): Promise<void> => {
    try {
      const created = await kernel.call<AgentDefinition>('agent.create', {
        definition: ASSISTANT_DEFINITION,
      });
      setAgents((prev) =>
        prev.some((agent) => agent.id === created.id) ? prev : [...prev, created],
      );
      await createFor(created.id);
    } catch (cause) {
      setActionError(messageOf(cause));
    }
  }, [kernel, createFor]);

  const send = useCallback(
    async (text?: string): Promise<void> => {
      const body = (text ?? draft).trim();
      if (!body || !activeId || sending) {
        return;
      }
      setSending(true);
      setActionError(null);
      setNotice(null);
      try {
        const result = await kernel.call<ChatMessageSendResult>('chat.message.send', {
          session_id: activeId,
          text: body,
        });
        setMessages(result.messages);
        setDraft('');
        await loadSessions();
      } catch (cause) {
        setActionError(messageOf(cause));
      } finally {
        setSending(false);
      }
    },
    [kernel, activeId, draft, sending, loadSessions],
  );

  const retry = useCallback(async (): Promise<void> => {
    if (!activeId || sending) {
      return;
    }
    setSending(true);
    setActionError(null);
    try {
      const result = await kernel.call<ChatMessageSendResult>('chat.message.retry', {
        session_id: activeId,
      });
      setMessages(result.messages);
      await loadSessions();
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setSending(false);
    }
  }, [kernel, activeId, sending, loadSessions]);

  const fork = useCallback(async (): Promise<void> => {
    if (!activeId) {
      return;
    }
    setActionError(null);
    try {
      const forked = await kernel.call<ChatSession>('chat.message.fork', {
        session_id: activeId,
      });
      await loadSessions();
      setActiveId(forked.id);
    } catch (cause) {
      setActionError(messageOf(cause));
    }
  }, [kernel, activeId, loadSessions]);

  const rename = useCallback(
    async (id: string, title: string): Promise<void> => {
      setActionError(null);
      try {
        await kernel.call<ChatSession>('chat.session.rename', { session_id: id, title });
        await loadSessions();
      } catch (cause) {
        setActionError(messageOf(cause));
      }
    },
    [kernel, loadSessions],
  );

  const setModel = useCallback(
    async (model: string | null): Promise<void> => {
      if (!activeId) {
        return;
      }
      setActionError(null);
      try {
        await kernel.call<ChatSession>('chat.session.model', {
          session_id: activeId,
          model,
        });
        await loadSessions();
      } catch (cause) {
        setActionError(messageOf(cause));
      }
    },
    [kernel, activeId, loadSessions],
  );

  const remove = useCallback(
    async (id: string): Promise<void> => {
      setActionError(null);
      try {
        await kernel.call('chat.session.delete', { session_id: id });
        const remaining = await loadSessions();
        if (activeIdRef.current === id) {
          setActiveId(remaining[0]?.id ?? null);
        }
      } catch (cause) {
        setActionError(messageOf(cause));
      }
    },
    [kernel, loadSessions],
  );

  const attach = useCallback(
    async (name: string, mediaType: string, size: number): Promise<void> => {
      if (!activeId) {
        return;
      }
      setActionError(null);
      try {
        const routing = await kernel.call<AttachmentRouting>('chat.attach', {
          session_id: activeId,
          name,
          media_type: mediaType,
          size,
        });
        setNotice(
          routing.note ??
            [routing.attachment.name, routing.attachment.kind, routing.routed].join(' · '),
        );
      } catch (cause) {
        setActionError(messageOf(cause));
      }
    },
    [kernel, activeId],
  );

  const search = useCallback(
    async (query: string): Promise<void> => {
      if (query.trim().length === 0) {
        setHits([]);
        return;
      }
      setSearching(true);
      try {
        const result = await kernel.call<ChatSessionSearchResult>('chat.session.search', {
          query,
        });
        setHits(result.hits);
      } catch (cause) {
        setActionError(messageOf(cause));
      } finally {
        setSearching(false);
      }
    },
    [kernel],
  );

  const clearSearch = useCallback(() => {
    setHits([]);
  }, []);

  const usePrompt = useCallback((prompt: PromptTemplate) => {
    setDraft(prompt.body);
  }, []);

  const active = useMemo(
    () => sessions.find((session) => session.id === activeId) ?? null,
    [sessions, activeId],
  );

  return {
    loading,
    error,
    agents,
    sessions,
    activeId,
    active,
    messages,
    messagesLoading,
    sending,
    actionError,
    prompts,
    hits,
    searching,
    notice,
    draft,
    capabilities,
    models,
    setDraft,
    select,
    setModel,
    newSession,
    createAssistant,
    send,
    retry,
    fork,
    rename,
    remove,
    attach,
    search,
    clearSearch,
    usePrompt,
  };
}
