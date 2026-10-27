/**
 * The plugin factory (`factory.*`, spec §6 / D18).
 *
 * Workspaces live under `$FERGENTS_HOME/factory/<run-id>`. The kernel builds,
 * tests, and packages there; it never installs on its own. `factory.install`
 * always returns a review first.
 */
import { useCallback, useEffect, useState } from 'react';

import type { KernelClient } from '../../lib/kernel';

import type { SkillReview } from '../skills/model';

import type { InstallReview } from './model';

/** The templates the kernel ships (`crates/core/src/factory.rs`). */
export const FACTORY_TEMPLATES = [
  'kernel-tool',
  'ui-panel',
  'hybrid',
  'theme',
  'skill-prompt',
  'skill-code',
] as const;

/** A factory template name. */
export type FactoryTemplate = (typeof FACTORY_TEMPLATES)[number];

/** `factory.scaffold`. */
export interface FactoryScaffoldResult {
  run_id: string;
  template: string;
  kind: string;
  path: string;
  files: string[];
}

/** `factory.build`. */
export interface FactoryBuildResult {
  run_id: string;
  skipped: boolean;
  reason?: string;
  log?: string;
}

/** `factory.test`. */
export interface FactoryTestResult {
  kind: string;
  ok: boolean;
  id: string;
  version: string;
  problems: string[];
  warnings: string[];
  permissions?: { declared: string[]; granted: string[] };
  cache?: boolean;
  slots?: string[];
  commands?: string[];
  capabilities?: string[];
  settings?: string[];
}

/** `factory.package`. */
export interface FactoryPackageResult {
  run_id: string;
  kind: string;
  extension: string;
  path: string;
}

/** `factory.report`. */
export interface FactoryReport {
  run_id: string;
  kind: string;
  manifest: string;
  path: string;
  files: string[];
  build_log: string | null;
  test: FactoryTestResult | null;
  packaged: string | null;
}

/** `factory.install` without a review: the review to approve. */
export interface FactoryInstallPreview {
  kind: 'plugin' | 'skill';
  id: string;
  requires_approval: boolean;
  review: InstallReview | SkillReview;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The state returned by {@link useFactory}. */
export interface FactoryController {
  loading: boolean;
  error: string | null;
  runs: string[];
  pending: string | null;
  refresh(): Promise<void>;
  scaffold(
    template: FactoryTemplate,
    id: string,
    name?: string,
  ): Promise<FactoryScaffoldResult | null>;
  build(runId: string): Promise<void>;
  test(runId: string): Promise<FactoryTestResult | null>;
  package(runId: string): Promise<FactoryPackageResult | null>;
  report(runId: string): Promise<FactoryReport | null>;
  install(runId: string): Promise<FactoryInstallPreview | null>;
}

/** The plugin factory workspaces. */
export function useFactory(kernel: KernelClient): FactoryController {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [runs, setRuns] = useState<string[]>([]);
  const [pending, setPending] = useState<string | null>(null);

  const refresh = useCallback(async (): Promise<void> => {
    setPending('list');
    try {
      const result = await kernel.call<{ runs: string[] }>('factory.list');
      setRuns(result.runs);
      setError(null);
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setLoading(false);
      setPending(null);
    }
  }, [kernel]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const runAction = useCallback(
    async <T>(key: string, method: string, params: Record<string, unknown>): Promise<T | null> => {
      setPending(key);
      setError(null);
      try {
        return await kernel.call<T>(method, params);
      } catch (cause) {
        setError(messageOf(cause));
        return null;
      } finally {
        setPending(null);
      }
    },
    [kernel],
  );

  const scaffold = useCallback(
    async (
      template: FactoryTemplate,
      id: string,
      name?: string,
    ): Promise<FactoryScaffoldResult | null> => {
      const result = await runAction<FactoryScaffoldResult>('scaffold', 'factory.scaffold', {
        template,
        id,
        ...(name === undefined ? {} : { name }),
      });
      if (result !== null) {
        await refresh();
      }
      return result;
    },
    [refresh, runAction],
  );

  const build = useCallback(
    async (runId: string): Promise<void> => {
      await runAction<FactoryBuildResult>(runId, 'factory.build', { run_id: runId });
    },
    [runAction],
  );

  const test = useCallback(
    (runId: string): Promise<FactoryTestResult | null> =>
      runAction<FactoryTestResult>(runId, 'factory.test', { run_id: runId }),
    [runAction],
  );

  const packageRun = useCallback(
    (runId: string): Promise<FactoryPackageResult | null> =>
      runAction<FactoryPackageResult>(runId, 'factory.package', { run_id: runId }),
    [runAction],
  );

  const report = useCallback(
    (runId: string): Promise<FactoryReport | null> =>
      runAction<FactoryReport>(runId, 'factory.report', { run_id: runId }),
    [runAction],
  );

  const install = useCallback(
    (runId: string): Promise<FactoryInstallPreview | null> =>
      runAction<FactoryInstallPreview>(runId, 'factory.install', { run_id: runId }),
    [runAction],
  );

  return {
    loading,
    error,
    runs,
    pending,
    refresh,
    scaffold,
    build,
    test,
    package: packageRun,
    report,
    install,
  };
}
