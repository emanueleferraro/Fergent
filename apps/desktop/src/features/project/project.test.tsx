import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Project } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import { MockKernelClient } from '../../lib/kernel';

import { filterPaths, changeBadge, baseName, statusMap } from './model';
import { ProjectExplorer } from './ProjectExplorer';
import { ProjectBar } from './ProjectBar';

afterEach(() => {
  cleanup();
});

const PROJECT: Project = {
  id: 'p1',
  path: '/work/repo',
  name: 'repo',
  vcs: 'git',
  brain_id: null,
  trust: 'untrusted',
  settings: {},
  last_opened_at: 0,
  created_at: 0,
  updated_at: 0,
};

describe('project model', () => {
  it('filters paths case-insensitively and keeps order', () => {
    const files = ['src/main.rs', 'src/lib.rs', 'README.md'];
    expect(filterPaths(files, '')).toEqual(files);
    expect(filterPaths(files, 'MAIN')).toEqual(['src/main.rs']);
    expect(filterPaths(files, '  .rs ')).toEqual(['src/main.rs', 'src/lib.rs']);
    expect(filterPaths(files, 'nothing')).toEqual([]);
  });

  it('maps git kinds to badges', () => {
    expect(changeBadge('modified')).toEqual({ letter: 'M', tone: 'modified' });
    expect(changeBadge('untracked')).toEqual({ letter: 'U', tone: 'untracked' });
    expect(changeBadge('conflicted')).toEqual({ letter: '!', tone: 'conflict' });
    expect(changeBadge('mystery')).toBeNull();
  });

  it('indexes a repository status by path', () => {
    const status = {
      project_id: 'p1',
      branch: 'main',
      head: null,
      detached: false,
      ahead: 0,
      behind: 0,
      clean: false,
      entries: [
        { path: 'a.rs', kind: 'modified' as const, staged: false },
        { path: 'b.rs', kind: 'untracked' as const, staged: false },
      ],
    };
    expect(statusMap(status)).toEqual({ 'a.rs': 'modified', 'b.rs': 'untracked' });
    expect(statusMap(null)).toEqual({});
  });

  it('extracts the base name', () => {
    expect(baseName('src/main.rs')).toBe('main.rs');
    expect(baseName('README.md')).toBe('README.md');
  });
});

describe('ProjectBar', () => {
  it('switches projects through the select', () => {
    const onSelect = vi.fn();
    render(
      <ProjectBar
        projects={[PROJECT, { ...PROJECT, id: 'p2', name: 'other' }]}
        active={PROJECT}
        onSelect={onSelect}
        onOpen={vi.fn()}
        onTrust={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByRole('combobox', { name: t('ide.project.label') }), {
      target: { value: 'p2' },
    });

    expect(onSelect).toHaveBeenCalledWith('p2');
  });

  it('opens a folder by path', () => {
    const onOpen = vi.fn();
    render(
      <ProjectBar
        projects={[PROJECT]}
        active={PROJECT}
        onSelect={vi.fn()}
        onOpen={onOpen}
        onTrust={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText(t('ide.project.openPlaceholder')), {
      target: { value: 'C:/code/app' },
    });
    fireEvent.click(screen.getByRole('button', { name: t('ide.project.open') }));

    expect(onOpen).toHaveBeenCalledWith('C:/code/app');
  });

  it('offers trust for an untrusted project only', () => {
    const onTrust = vi.fn();
    const { rerender } = render(
      <ProjectBar
        projects={[PROJECT]}
        active={PROJECT}
        onSelect={vi.fn()}
        onOpen={vi.fn()}
        onTrust={onTrust}
      />,
    );

    expect(screen.getByText(t('ide.project.untrusted.desc'))).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: t('ide.project.trust') }));
    expect(onTrust).toHaveBeenCalledWith(true);

    rerender(
      <ProjectBar
        projects={[PROJECT]}
        active={{ ...PROJECT, trust: 'trusted' }}
        onSelect={vi.fn()}
        onOpen={vi.fn()}
        onTrust={onTrust}
      />,
    );
    expect(screen.queryByRole('button', { name: t('ide.project.trust') })).toBeNull();
  });
});

describe('ProjectExplorer', () => {
  function renderExplorer(files: readonly string[], statuses: Record<string, 'modified'>) {
    return render(
      <ProjectExplorer
        kernel={new MockKernelClient()}
        projects={[PROJECT]}
        active={PROJECT}
        files={files}
        statuses={statuses}
        onSelectProject={vi.fn()}
        onOpenProject={vi.fn()}
        onTrust={vi.fn()}
        onOpenFile={vi.fn()}
        onOpenNote={vi.fn()}
      />,
    );
  }

  it('filters the project tree', () => {
    renderExplorer(['src/main.rs', 'src/lib.rs', 'README.md'], {});

    expect(screen.getByText('README.md')).toBeDefined();
    expect(screen.getByText('src')).toBeDefined();

    fireEvent.change(screen.getByLabelText(t('ide.project.filter')), {
      target: { value: 'readme' },
    });

    expect(screen.getByText('README.md')).toBeDefined();
    expect(screen.queryByText('src')).toBeNull();

    fireEvent.change(screen.getByLabelText(t('ide.project.filter')), {
      target: { value: 'zzz' },
    });
    expect(screen.getByText(t('ide.files.empty.title'))).toBeDefined();
  });

  it('shows git badges on changed files', () => {
    renderExplorer(['src/main.rs'], { 'src/main.rs': 'modified' });

    fireEvent.click(screen.getByText('src'));

    const badge = screen.getByLabelText(t('git.change.modified'));
    expect(badge.textContent).toBe('M');
  });
});
