/**
 * Container for the brain sidebar: loads data through the kernel client and
 * owns the explorer's local selection state (active note, tag, heading).
 */
import { useCallback, useState, type ReactElement } from 'react';
import type { OutlineEntry } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import { BrainSidebar } from './BrainSidebar';
import { useBrainExplorer } from './useBrainExplorer';

/** Props for `BrainExplorer`. */
export interface BrainExplorerProps {
  /** The connected kernel client. */
  kernel: KernelClient;
}

/** The live brain explorer. */
export function BrainExplorer({ kernel }: BrainExplorerProps): ReactElement {
  const [activeId, setActiveId] = useState<string | undefined>(undefined);
  const [activeTag, setActiveTag] = useState<string | undefined>(undefined);
  const [activeHeading, setActiveHeading] = useState<string | undefined>(undefined);
  const data = useBrainExplorer(kernel, activeId);

  const activePath = data.documents.find((document) => document.id === activeId)?.path;

  const onSelectPath = useCallback(
    (path: string) => {
      setActiveId(data.documents.find((document) => document.path === path)?.id);
    },
    [data.documents],
  );

  const onSelectHeading = useCallback((entry: OutlineEntry) => {
    setActiveHeading(entry.text);
  }, []);

  return (
    <BrainSidebar
      documents={data.documents}
      tags={data.tags}
      outline={data.outline}
      backlinks={data.backlinks}
      mentions={data.mentions}
      activePath={activePath}
      activeTag={activeTag}
      activeHeading={activeHeading}
      onSelectPath={onSelectPath}
      onSelectTag={setActiveTag}
      onSelectHeading={onSelectHeading}
      onOpenDocument={setActiveId}
    />
  );
}
