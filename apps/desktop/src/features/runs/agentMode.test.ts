import { describe, expect, it } from 'vitest';

import { artifactTabs, runStateKey } from './agentMode';

describe('artifactTabs', () => {
  it('returns the present artifacts in workflow order', () => {
    const tabs = artifactTabs({
      task: '# Tasks',
      spec: '# Spec',
      plan: '# Plan',
    });

    expect(tabs.map((tab) => tab.gate)).toEqual(['spec', 'plan', 'task']);
    expect(tabs[0]?.titleKey).toBe('run.gate.spec');
    expect(tabs[1]?.body).toBe('# Plan');
  });

  it('drops missing and blank artifacts', () => {
    const tabs = artifactTabs({ spec: '# Spec', plan: '   ', diff: '# Diff' });

    expect(tabs.map((tab) => tab.gate)).toEqual(['spec']);
  });

  it('returns nothing for an empty run', () => {
    expect(artifactTabs({})).toEqual([]);
  });
});

describe('runStateKey', () => {
  it('maps every workflow state to a label key', () => {
    expect(runStateKey('running')).toBe('run.state.running');
    expect(runStateKey('awaiting_spec_approval')).toBe('run.state.spec');
    expect(runStateKey('awaiting_plan_approval')).toBe('run.state.plan');
    expect(runStateKey('awaiting_task_approval')).toBe('run.state.task');
    expect(runStateKey('awaiting_diff_approval')).toBe('run.state.diff');
    expect(runStateKey('completed')).toBe('run.state.completed');
    expect(runStateKey('rejected')).toBe('run.state.rejected');
  });

  it('returns null for a state without a label', () => {
    expect(runStateKey('queued')).toBeNull();
  });
});
