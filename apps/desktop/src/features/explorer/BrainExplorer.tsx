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
  /** Controlled active document id (owned by the shell). */
  activeDocumentId?: string | undefined;
  /** Called when a note should be opened; if omitted, selection stays local. */
  onOpenDocument?: ((documentId: string) => void) | undefined;
}

/** The live brain explorer. */
export function BrainExplorer({
  kernel,
  activeDocumentId,
  onOpenDocument,
}: BrainExplorerProps): ReactElement {
  const [localId, setLocalId] = useState<string | undefined>(undefined);
  const [activeTag, setActiveTag] = useState<string | undefined>(undefined);
  const [activeHeading, setActiveHeading] = useState<string | undefined>(undefined);
  const activeId = activeDocumentId ?? localId;
  const openDocument = onOpenDocument ?? setLocalId;
  const data = useBrainExplorer(kernel, activeId);

  const activePath = data.documents.find((document) => document.id === activeId)?.path;

  const onSelectPath = useCallback(
    (path: string) => {
      const id = data.documents.find((document) => document.path === path)?.id;
      if (id) {
        openDocument(id);
      }
    },
    [data.documents, openDocument],
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
      onOpenDocument={openDocument}
    />
  );
}
