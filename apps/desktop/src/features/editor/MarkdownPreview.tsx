/**
 * Live markdown preview pane.
 *
 * Renders the flavor HTML (see `markdown.ts`) and hydrates the parts that need
 * a real DOM: Mermaid diagrams (dynamic import, browser only) and wiki-link
 * clicks. KaTeX math is rendered to static HTML by the renderer.
 */
import { useEffect, useMemo, useRef, type MouseEvent, type ReactElement } from 'react';
import 'katex/dist/katex.min.css';

import { t } from '../../i18n/i18n';

import { renderMarkdown } from './markdown';

/** Props for `MarkdownPreview`. */
export interface MarkdownPreviewProps {
  /** Markdown source of the active note. */
  source: string;
  /** Called with the target path when a wiki link is clicked. */
  onOpenLink?: ((path: string) => void) | undefined;
}

function calloutTitles(): Record<string, string> {
  return {
    note: t('editor.callout.note'),
    tip: t('editor.callout.tip'),
    warning: t('editor.callout.warning'),
    danger: t('editor.callout.danger'),
    info: t('editor.callout.info'),
    success: t('editor.callout.success'),
  };
}

async function hydrateMermaid(root: HTMLElement): Promise<() => void> {
  const nodes = Array.from(root.querySelectorAll<HTMLElement>('pre.mermaid'));
  if (nodes.length === 0) {
    return () => undefined;
  }
  let cancelled = false;
  const mermaid = (await import('mermaid')).default;
  mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'dark' });
  for (const node of nodes) {
    if (cancelled) {
      break;
    }
    const code = node.textContent ?? '';
    try {
      const { svg } = await mermaid.render(`mermaid-${Math.random().toString(36).slice(2)}`, code);
      if (cancelled) {
        break;
      }
      const wrapper = document.createElement('div');
      wrapper.className = 'mermaid-block';
      wrapper.innerHTML = svg;
      node.replaceWith(wrapper);
    } catch {
      node.className = 'mermaid-error';
    }
  }
  return () => {
    cancelled = true;
  };
}

/** The rendered note preview. */
export function MarkdownPreview({ source, onOpenLink }: MarkdownPreviewProps): ReactElement {
  const host = useRef<HTMLDivElement>(null);
  const html = useMemo(() => renderMarkdown(source, { calloutTitles: calloutTitles() }), [source]);

  useEffect(() => {
    const root = host.current;
    if (!root) {
      return;
    }
    let cancel: (() => void) | undefined;
    void hydrateMermaid(root).then((dispose) => {
      cancel = dispose;
    });
    return () => {
      cancel?.();
    };
  }, [html]);

  const onClick = (event: MouseEvent<HTMLDivElement>): void => {
    const anchor = (event.target as HTMLElement).closest('a.wikilink');
    if (!anchor) {
      return;
    }
    event.preventDefault();
    const path = anchor.getAttribute('data-wikilink');
    if (path) {
      onOpenLink?.(path);
    }
  };

  return (
    <div ref={host} className="md" onClick={onClick} dangerouslySetInnerHTML={{ __html: html }} />
  );
}
