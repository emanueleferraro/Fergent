/**
 * The agent-run panel model (task 3.14): the spec/plan/tasks artifacts of a
 * workflow run and the labels the panel shows.
 */
import type { MessageKey } from '../../i18n/en';

/** The artifact gates, in the order the workflow writes them. */
export const ARTIFACT_GATES = ['spec', 'plan', 'task'] as const;

/** A gate that produces a Markdown artifact. */
export type ArtifactGate = (typeof ARTIFACT_GATES)[number];

/** One artifact tab. */
export interface ArtifactTab {
  gate: ArtifactGate;
  titleKey: MessageKey;
  body: string;
}

const GATE_TITLES: Record<ArtifactGate, MessageKey> = {
  spec: 'run.gate.spec',
  plan: 'run.gate.plan',
  task: 'run.gate.task',
};

/** The artifact tabs of a run, in workflow order (empty artifacts dropped). */
export function artifactTabs(artifacts: Readonly<Record<string, string>>): ArtifactTab[] {
  const tabs: ArtifactTab[] = [];
  for (const gate of ARTIFACT_GATES) {
    const body = artifacts[gate];
    if (body !== undefined && body.trim().length > 0) {
      tabs.push({ gate, titleKey: GATE_TITLES[gate], body });
    }
  }
  return tabs;
}

/** The i18n key of a run state, or `null` when the label is unknown. */
export function runStateKey(state: string): MessageKey | null {
  switch (state) {
    case 'running':
      return 'run.state.running';
    case 'awaiting_spec_approval':
      return 'run.state.spec';
    case 'awaiting_plan_approval':
      return 'run.state.plan';
    case 'awaiting_task_approval':
      return 'run.state.task';
    case 'awaiting_diff_approval':
      return 'run.state.diff';
    case 'completed':
      return 'run.state.completed';
    case 'rejected':
      return 'run.state.rejected';
    default:
      return null;
  }
}
