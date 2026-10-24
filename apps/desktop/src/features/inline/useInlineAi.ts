/**
 * The inline AI controller (spec §6, task 3.12): one engine per project,
 * exposed to the Monaco provider and the status bar.
 */
import { useEffect, useMemo, useState } from 'react';
import type { ComposerRun } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import {
  InlineEngine,
  type InlineActionKind,
  type InlineRequest,
  type InlineStatus,
} from './inlineEngine';

/** The inline AI state and actions the IDE uses. */
export interface InlineAiController {
  /** `capability.inline-ai` is enabled. */
  enabled: boolean;
  status: InlineStatus;
  suggest(request: InlineRequest): Promise<string | null>;
  cancel(): void;
  action(kind: InlineActionKind, path: string, selection: string): Promise<ComposerRun | null>;
}

/** Debounced ghost-text completions and quick actions for one project. */
export function useInlineAi(
  kernel: KernelClient,
  projectId: string | null,
  trusted: boolean,
  enabled: boolean,
): InlineAiController {
  const engine = useMemo(
    () => new InlineEngine({ kernel, projectId, trusted, enabled }),
    [kernel, projectId, trusted, enabled],
  );
  const [status, setStatus] = useState<InlineStatus>(() => engine.getStatus());

  useEffect(() => {
    const unsubscribe = engine.subscribe(setStatus);
    return () => {
      unsubscribe();
      engine.cancel();
    };
  }, [engine]);

  return useMemo(
    () => ({
      enabled,
      status,
      suggest: (request: InlineRequest) => engine.suggest(request),
      cancel: () => engine.cancel(),
      action: (kind: InlineActionKind, path: string, selection: string) =>
        engine.action(kind, path, selection),
    }),
    [enabled, engine, status],
  );
}
