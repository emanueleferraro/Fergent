/**
 * The IDE sidebar (UI.md §5.3, §8.2): project switcher, filtered project
 * tree with git badges, and the brain tree in its own section.
 */
import { useMemo, useState, type ReactElement } from 'react';
import type { GitChangeKind } from '@fergents/protocol';

import { BrainExplorer } from '../explorer/BrainExplorer';
import { FileTree, type TreeBadge } from '../explorer/FileTree';
import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { changeBadge, filterPaths } from './model';
import { ProjectBar } from './ProjectBar';

/** Props for `ProjectExplorer`. */
export interface ProjectExplorerProps {
  kernel: KernelClient;
  projects: readonly {
    id: string;
    name: string;
    path: string;
    trust: 'trusted' | 'untrusted';
  }[];
  active: { id: string; name: string; path: string; trust: 'trusted' | 'untrusted' } | null;
  files: readonly string[];
  statuses: Record<string, GitChangeKind>;
  activePath?: string | undefined;
  error?: string | null | undefined;
  onSelectProject: (id: string) => void;
  onOpenProject: (path: string) => void;
  onTrust: (trusted: boolean) => void;
  onOpenFile: (path: string) => void;
  onOpenNote: (documentId: string) => void;
}

const BADGE_TITLES: Record<string, string> = {
  added: 'git.change.added',
  modified: 'git.change.modified',
  deleted: 'git.change.deleted',
  renamed: 'git.change.renamed',
  conflict: 'git.change.conflicted',
  untracked: 'git.change.untracked',
};

/** The project + brain explorer shown in the IDE sidebar. */
export function ProjectExplorer({
  kernel,
  projects,
  active,
  files,
  statuses,
  activePath,
  error,
  onSelectProject,
  onOpenProject,
  onTrust,
  onOpenFile,
  onOpenNote,
}: ProjectExplorerProps): ReactElement {
  const [filter, setFilter] = useState('');

  const visibleFiles = useMemo(() => filterPaths(files, filter), [files, filter]);

  const badges = useMemo<Record<string, TreeBadge>>(() => {
    const map: Record<string, TreeBadge> = {};
    for (const [path, kind] of Object.entries(statuses)) {
      const badge = changeBadge(kind);
      if (badge) {
        map[path] = { letter: badge.letter, tone: badge.tone, title: badgeTitle(badge.tone) };
      }
    }
    return map;
  }, [statuses]);

  return (
    <div className="project-explorer">
      <ProjectBar
        projects={projects}
        active={active}
        onSelect={onSelectProject}
        onOpen={onOpenProject}
        onTrust={onTrust}
        error={error}
      />

      <section className="project-explorer__section">
        <div className="panel-title">{t('ide.files.label')}</div>
        {active ? (
          <>
            <input
              className="input"
              type="search"
              aria-label={t('ide.project.filter')}
              placeholder={t('ide.project.filter')}
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
            />
            {visibleFiles.length === 0 ? (
              <div className="empty">
                <div className="empty-title">{t('ide.files.empty.title')}</div>
                <div className="empty-desc">{t('ide.files.empty.desc')}</div>
              </div>
            ) : (
              <FileTree
                documents={visibleFiles.map((path) => ({ path }))}
                activePath={activePath}
                onSelect={onOpenFile}
                status={badges}
                label={t('ide.files.label')}
              />
            )}
          </>
        ) : (
          <div className="empty">
            <div className="empty-title">{t('ide.project.empty.title')}</div>
            <div className="empty-desc">{t('ide.project.empty.desc')}</div>
          </div>
        )}
      </section>

      <section className="project-explorer__section">
        <div className="panel-title">{t('ide.brain.section')}</div>
        <BrainExplorer kernel={kernel} onOpenDocument={onOpenNote} />
      </section>
    </div>
  );
}

function badgeTitle(tone: string): string {
  const key = BADGE_TITLES[tone];
  return key === undefined ? tone : t(key as Parameters<typeof t>[0]);
}
