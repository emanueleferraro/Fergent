/**
 * The persisted chat + IDE split (spec decision D14).
 *
 * The layout tree is loaded once per project through `editor.layout.get`,
 * falling back to the default split when nothing (or something invalid) was
 * stored. Divider moves are persisted after a short debounce so dragging
 * does not flood the kernel.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { EditorLayoutGetResult } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import { DEFAULT_LAYOUT, parseLayout, rootRatio, setRatio, type LayoutNode } from './layout';

/** How long to wait after the last divider move before persisting. */
export const PERSIST_DELAY_MS = 400;

/** The layout state exposed to the IDE workspace. */
export interface IdeLayoutController {
  /** The active layout tree. */
  layout: LayoutNode;
  /** The share of the space taken by the first pane (chat). */
  ratio: number;
  /** Move the root divider. */
  resize: (ratio: number) => void;
  /** Return to the default split. */
  reset: () => void;
}

/**
 * Load, resize, and persist the workspace layout for `projectId`.
 *
 * Without a project the layout is in-memory only: it still renders (chat +
 * IDE) but nothing is stored.
 */
export function useIdeLayout(kernel: KernelClient, projectId: string | null): IdeLayoutController {
  const [layout, setLayout] = useState<LayoutNode>(DEFAULT_LAYOUT);
  const layoutRef = useRef(layout);
  layoutRef.current = layout;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    if (!projectId) {
      setLayout(DEFAULT_LAYOUT);
      return;
    }
    void (async () => {
      try {
        const result = await kernel.call<EditorLayoutGetResult>('editor.layout.get', {
          project_id: projectId,
        });
        if (!cancelled) {
          setLayout(parseLayout(result?.layout));
        }
      } catch {
        if (!cancelled) {
          setLayout(DEFAULT_LAYOUT);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel, projectId]);

  useEffect(
    () => () => {
      if (timer.current) {
        clearTimeout(timer.current);
      }
    },
    [],
  );

  const persist = useCallback(
    (next: LayoutNode) => {
      if (!projectId) {
        return;
      }
      if (timer.current) {
        clearTimeout(timer.current);
      }
      timer.current = setTimeout(() => {
        timer.current = null;
        void kernel.call('editor.layout.set', { project_id: projectId, layout: next }).catch(() => {
          // The layout is a preference: a failed save is not fatal.
        });
      }, PERSIST_DELAY_MS);
    },
    [kernel, projectId],
  );

  const resize = useCallback(
    (ratio: number) => {
      const next = setRatio(layoutRef.current, DEFAULT_LAYOUT.id, ratio);
      layoutRef.current = next;
      setLayout(next);
      persist(next);
    },
    [persist],
  );

  const reset = useCallback(() => {
    layoutRef.current = DEFAULT_LAYOUT;
    setLayout(DEFAULT_LAYOUT);
    persist(DEFAULT_LAYOUT);
  }, [persist]);

  return { layout, ratio: rootRatio(layout), resize, reset };
}
