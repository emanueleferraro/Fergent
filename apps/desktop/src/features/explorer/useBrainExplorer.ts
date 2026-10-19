/**
 * Loads the brain explorer data from the kernel.
 *
 * `doc.list` and `tag.list` are fetched once (and on `reloadToken` change);
 * the active note's outline, backlinks, and unlinked mentions are fetched
 * whenever the selection changes. Results are defensive: a missing or
 * unexpected payload degrades to an empty list rather than throwing.
 */
import { useEffect, useState } from 'react';
import type { Document, LinkRef, OutlineEntry, Tag, UnlinkedMention } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

/** The data the brain sidebar renders. */
export interface BrainExplorerData {
  documents: Document[];
  tags: Tag[];
  outline: OutlineEntry[];
  backlinks: LinkRef[];
  mentions: UnlinkedMention[];
  loading: boolean;
  error: string | null;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function items<T>(payload: Record<string, unknown> | null | undefined, key: string): T[] {
  const value = payload?.[key];
  return Array.isArray(value) ? (value as T[]) : [];
}

/** Load the brain explorer data for `documentId` (the active note). */
export function useBrainExplorer(
  kernel: KernelClient,
  documentId?: string,
  reloadToken = 0,
): BrainExplorerData {
  const [documents, setDocuments] = useState<Document[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [outline, setOutline] = useState<OutlineEntry[]>([]);
  const [backlinks, setBacklinks] = useState<LinkRef[]>([]);
  const [mentions, setMentions] = useState<UnlinkedMention[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const [docResult, tagResult] = await Promise.all([
          kernel.call<Record<string, unknown>>('doc.list'),
          kernel.call<Record<string, unknown>>('tag.list'),
        ]);
        if (!active) {
          return;
        }
        setDocuments(items<Document>(docResult, 'documents'));
        setTags(items<Tag>(tagResult, 'tags'));
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
  }, [kernel, reloadToken]);

  useEffect(() => {
    if (!documentId) {
      setOutline([]);
      setBacklinks([]);
      setMentions([]);
      return;
    }
    let active = true;
    void (async () => {
      try {
        const [outlineResult, backlinksResult, mentionsResult] = await Promise.all([
          kernel.call<Record<string, unknown>>('doc.outline', { document_id: documentId }),
          kernel.call<Record<string, unknown>>('doc.backlinks', { document_id: documentId }),
          kernel.call<Record<string, unknown>>('doc.unlinked_mentions', {
            document_id: documentId,
          }),
        ]);
        if (!active) {
          return;
        }
        setOutline(items<OutlineEntry>(outlineResult, 'outline'));
        setBacklinks(items<LinkRef>(backlinksResult, 'backlinks'));
        setMentions(items<UnlinkedMention>(mentionsResult, 'mentions'));
      } catch (cause) {
        if (active) {
          setError(messageOf(cause));
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel, documentId, reloadToken]);

  return { documents, tags, outline, backlinks, mentions, loading, error };
}
