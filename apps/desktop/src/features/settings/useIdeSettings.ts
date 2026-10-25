/**
 * The IDE settings controller (task 3.16): the shell, the editor defaults and
 * the language-server registry.
 *
 * The shell and editor preferences live in the kernel's generic settings store
 * (`settings.get` / `settings.set`), so they survive a restart and are shared
 * with the terminal. The servers come from the LSP registry; adding or removing
 * one is a registry write, not a preference.
 */
import { useCallback, useEffect, useState } from 'react';
import type { LspServer, LspServerListResult, SettingsValueResult } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import { useCapabilityEnabled } from './useCapabilityEnabled';

/** The settings key holding `{ shell }`. */
export const IDE_SHELL_KEY = 'ide.shell';
/** The settings key holding the editor defaults. */
export const IDE_EDITOR_KEY = 'ide.editor';

/** The editor preferences stored under `ide.editor`. */
export interface EditorPreferences {
  format_on_save: boolean;
  max_file_size_kb: number;
}

/** The defaults when nothing is stored yet. */
export const DEFAULT_EDITOR: EditorPreferences = {
  format_on_save: false,
  max_file_size_kb: 2048,
};

/** The add-server form values. */
export interface ServerDraft {
  language_id: string;
  command: string;
  /** Space-separated arguments. */
  args: string;
}

/** Everything the IDE settings page needs. */
export interface IdeSettingsController {
  loading: boolean;
  saving: boolean;
  /** True while a registry write is in flight. */
  busy: boolean;
  error: string | null;
  /** True while the remote-capability toggle is in flight. */
  remoteBusy: boolean;
  /** Whether git fetch, pull, and push are allowed. */
  remote: boolean;
  /** The persisted shell id (`auto`, `bash`, …). */
  shell: string;
  /** The shell selected in the form. */
  shellDraft: string;
  /** The persisted editor defaults. */
  editor: EditorPreferences;
  /** The editor defaults being edited. */
  draft: EditorPreferences;
  servers: LspServer[];
  dirty: boolean;
  saved: boolean;
  setShellDraft: (value: string) => void;
  setDraft: (value: EditorDraftPatch) => void;
  save: () => Promise<void>;
  /** Enable or disable the git remote capability. */
  setRemote: (enabled: boolean) => Promise<void>;
  addServer: (server: ServerAddPayload) => Promise<void>;
  removeServer: (id: string) => Promise<void>;
  dismissError: () => void;
}

/** A partial editor-draft update. */
export type EditorDraftPatch = Partial<EditorPreferences>;

/** A validated server to register. */
export interface ServerAddPayload {
  language_id: string;
  command: string;
  args: string[];
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Read the stored editor defaults, falling back per field. */
export function readEditorPreferences(value: Record<string, unknown>): EditorPreferences {
  const format = value['format_on_save'];
  const size = value['max_file_size_kb'];
  return {
    format_on_save: typeof format === 'boolean' ? format : DEFAULT_EDITOR.format_on_save,
    max_file_size_kb:
      typeof size === 'number' && Number.isFinite(size) && size > 0
        ? size
        : DEFAULT_EDITOR.max_file_size_kb,
  };
}

/** Split the argument input into the argv the registry stores. */
export function parseArgs(input: string): string[] {
  return input
    .split(/\s+/)
    .map((arg) => arg.trim())
    .filter((arg) => arg.length > 0);
}

function sameEditor(left: EditorPreferences, right: EditorPreferences): boolean {
  return (
    left.format_on_save === right.format_on_save && left.max_file_size_kb === right.max_file_size_kb
  );
}

/** Load and persist the IDE settings of the kernel. */
export function useIdeSettings(kernel: KernelClient, lspEnabled: boolean): IdeSettingsController {
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [busy, setBusy] = useState(false);
  const [remoteBusy, setRemoteBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const remoteCapability = useCapabilityEnabled(kernel, 'capability.git-remote');
  const [remote, setRemoteState] = useState(false);
  const [shell, setShell] = useState('auto');
  const [shellDraft, setShellDraft] = useState('auto');
  const [editor, setEditor] = useState<EditorPreferences>(DEFAULT_EDITOR);
  const [draft, setDraftState] = useState<EditorPreferences>(DEFAULT_EDITOR);
  const [servers, setServers] = useState<LspServer[]>([]);
  const [saved, setSaved] = useState(false);

  const loadServers = useCallback(async (): Promise<void> => {
    if (!lspEnabled) {
      setServers([]);
      return;
    }
    const listed = await kernel.call<LspServerListResult>('lsp.server.list');
    setServers(listed.servers);
  }, [kernel, lspEnabled]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void (async () => {
      try {
        const [storedShell, storedEditor] = await Promise.all([
          kernel.call<SettingsValueResult>('settings.get', { key: IDE_SHELL_KEY }),
          kernel.call<SettingsValueResult>('settings.get', { key: IDE_EDITOR_KEY }),
        ]);
        if (!active) {
          return;
        }
        const shellValue = storedShell.value['shell'];
        if (typeof shellValue === 'string' && shellValue.length > 0) {
          setShell(shellValue);
          setShellDraft(shellValue);
        }
        const preferences = readEditorPreferences(storedEditor.value);
        setEditor(preferences);
        setDraftState(preferences);
        setError(null);
        await loadServers();
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
  }, [kernel, loadServers]);

  const setDraft = useCallback((patch: EditorDraftPatch) => {
    setDraftState((current) => ({ ...current, ...patch }));
    setSaved(false);
  }, []);

  useEffect(() => {
    setRemoteState(remoteCapability);
  }, [remoteCapability]);

  const setRemote = useCallback(
    async (enabled: boolean): Promise<void> => {
      const previous = remote;
      setRemoteState(enabled);
      setRemoteBusy(true);
      setError(null);
      try {
        await kernel.call('capability.set', { id: 'capability.git-remote', enabled });
      } catch (cause) {
        setRemoteState(previous);
        setError(messageOf(cause));
      } finally {
        setRemoteBusy(false);
      }
    },
    [kernel, remote],
  );

  const updateShellDraft = useCallback((value: string) => {
    setShellDraft(value);
    setSaved(false);
  }, []);

  const dirty = shellDraft !== shell || !sameEditor(draft, editor);

  const save = useCallback(async (): Promise<void> => {
    setSaving(true);
    setError(null);
    try {
      await kernel.call('settings.set', {
        key: IDE_SHELL_KEY,
        value: { shell: shellDraft },
      });
      await kernel.call('settings.set', {
        key: IDE_EDITOR_KEY,
        value: { ...draft },
      });
      setShell(shellDraft);
      setEditor(draft);
      setSaved(true);
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setSaving(false);
    }
  }, [draft, kernel, shellDraft]);

  const addServer = useCallback(
    async (server: ServerAddPayload): Promise<void> => {
      setBusy(true);
      setError(null);
      try {
        await kernel.call('lsp.server.add', { ...server, enabled: true });
        await loadServers();
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setBusy(false);
      }
    },
    [kernel, loadServers],
  );

  const removeServer = useCallback(
    async (id: string): Promise<void> => {
      setBusy(true);
      setError(null);
      try {
        await kernel.call('lsp.server.remove', { id });
        await loadServers();
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setBusy(false);
      }
    },
    [kernel, loadServers],
  );

  const dismissError = useCallback(() => setError(null), []);

  return {
    loading,
    saving,
    busy,
    error,
    remoteBusy,
    remote,
    shell,
    shellDraft,
    editor,
    draft,
    servers,
    dirty,
    saved,
    setShellDraft: updateShellDraft,
    setDraft,
    save,
    setRemote,
    addServer,
    removeServer,
    dismissError,
  };
}
