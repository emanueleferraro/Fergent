/**
 * Keeps the kernel's language servers in sync with the open editor buffers
 * (spec §3): `didOpen` on first sight, debounced `didChange` while typing, and
 * `didClose` when a tab closes. Only trusted projects talk to a server (D3).
 */
import { useEffect, useRef } from 'react';
import type { LspAck } from '@fergents/protocol';

import type { EditorDocument } from '../editor/documents';
import type { KernelClient } from '../../lib/kernel';

/** Typing settles for this long before a change is sent to the server. */
export const CHANGE_DEBOUNCE_MS = 350;

/** Mirror the open documents into the language servers of a project. */
export function useLspDocuments(
  kernel: KernelClient,
  projectId: string | null,
  trusted: boolean,
  documents: readonly EditorDocument[],
): void {
  // path -> the text the kernel last received.
  const sent = useRef<Map<string, string>>(new Map());
  // path -> the pending debounce timer.
  const timers = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());
  const latest = useRef<readonly EditorDocument[]>(documents);
  latest.current = documents;

  // Closing the project (or unmounting) closes what we opened on its servers.
  useEffect(() => {
    if (projectId === null) {
      return;
    }
    const project = projectId;
    const opened = sent.current;
    return () => {
      for (const path of opened.keys()) {
        void kernel
          .call<LspAck>('lsp.document.close', { project_id: project, path })
          .catch(() => undefined);
      }
      opened.clear();
    };
  }, [kernel, projectId]);

  useEffect(() => {
    if (projectId === null || !trusted) {
      return;
    }
    const open = new Set(documents.map((document) => document.path));

    for (const path of [...sent.current.keys()]) {
      if (open.has(path)) {
        continue;
      }
      sent.current.delete(path);
      const timer = timers.current.get(path);
      if (timer !== undefined) {
        clearTimeout(timer);
        timers.current.delete(path);
      }
      void kernel
        .call<LspAck>('lsp.document.close', { project_id: projectId, path })
        .catch(() => undefined);
    }

    for (const document of documents) {
      if (document.readOnly || document.binary || document.truncated) {
        continue;
      }
      const sentText = sent.current.get(document.path);
      if (sentText === document.content) {
        continue;
      }
      if (sentText === undefined) {
        sent.current.set(document.path, document.content);
        void kernel
          .call<LspAck>('lsp.document.open', {
            project_id: projectId,
            path: document.path,
            text: document.content,
            language_id: document.languageId,
          })
          .catch(() => undefined);
        continue;
      }
      const pending = timers.current.get(document.path);
      if (pending !== undefined) {
        clearTimeout(pending);
      }
      timers.current.set(
        document.path,
        setTimeout(() => {
          timers.current.delete(document.path);
          const current = latest.current.find((item) => item.path === document.path);
          if (current === undefined) {
            return;
          }
          sent.current.set(current.path, current.content);
          void kernel
            .call<LspAck>('lsp.document.change', {
              project_id: projectId,
              path: current.path,
              text: current.content,
            })
            .catch(() => undefined);
        }, CHANGE_DEBOUNCE_MS),
      );
    }
  }, [kernel, projectId, trusted, documents]);
}
