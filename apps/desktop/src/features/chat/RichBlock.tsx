/**
 * Renders one rich block (Task 2.15): a chart, a Mermaid diagram, or sanitized
 * HTML — always inside a sandboxed, CSP-locked iframe.
 *
 * When the corresponding capability is disabled, or the spec is malformed, the
 * block degrades to a notice plus the raw code.
 */
import { useEffect, useMemo, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';

import { chartSvg, frameDocument, parseChartSpec, sanitizeHtml, type RichBlockData } from './rich';

/** Props for `RichBlock`. */
export interface RichBlockProps {
  block: RichBlockData;
  /** Whether the matching capability (`charts` / `mermaid` / `html`) is on. */
  enabled: boolean;
}

function kindLabel(kind: RichBlockData['kind']): string {
  switch (kind) {
    case 'chart':
      return t('rich.kind.chart');
    case 'mermaid':
      return t('rich.kind.mermaid');
    default:
      return t('rich.kind.html');
  }
}

const SANDBOX = 'allow-scripts';

/** Render a rich block, or a notice and the code when it cannot be rendered. */
export function RichBlock({ block, enabled }: RichBlockProps): ReactElement {
  const [mermaidSvg, setMermaidSvg] = useState<string | null>(null);
  const [mermaidError, setMermaidError] = useState(false);

  useEffect(() => {
    if (!enabled || block.kind !== 'mermaid') {
      return;
    }
    let active = true;
    setMermaidError(false);
    void (async () => {
      try {
        const mermaid = (await import('mermaid')).default;
        mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'dark' });
        const id = `rich-mermaid-${Math.random().toString(36).slice(2)}`;
        const { svg } = await mermaid.render(id, block.body);
        if (active) {
          setMermaidSvg(svg);
        }
      } catch {
        if (active) {
          setMermaidError(true);
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [enabled, block.kind, block.body]);

  const chart = useMemo(
    () => (block.kind === 'chart' ? parseChartSpec(block.body) : null),
    [block.kind, block.body],
  );

  const fallback = (reason: string): ReactElement => (
    <div className="rich-block rich-block--fallback">
      <div className="rich-block__note">
        {t(reason === 'disabled' ? 'rich.disabled' : 'rich.invalid', {
          kind: kindLabel(block.kind),
        })}
      </div>
      <pre className="rich-block__code">
        <code>{block.body}</code>
      </pre>
    </div>
  );

  if (!enabled) {
    return fallback('disabled');
  }

  if (block.kind === 'chart') {
    if (!chart || 'error' in chart) {
      return fallback('invalid');
    }
    return (
      <figure className="rich-block">
        <iframe
          className="rich-block__frame"
          title={t('rich.chart.title')}
          sandbox={SANDBOX}
          referrerPolicy="no-referrer"
          srcDoc={frameDocument(chartSvg(chart.spec))}
        />
        {chart.spec.title ? (
          <figcaption className="rich-block__caption">{chart.spec.title}</figcaption>
        ) : null}
      </figure>
    );
  }

  if (block.kind === 'html') {
    return (
      <figure className="rich-block">
        <iframe
          className="rich-block__frame"
          title={t('rich.html.title')}
          sandbox={SANDBOX}
          referrerPolicy="no-referrer"
          srcDoc={frameDocument(sanitizeHtml(block.body))}
        />
      </figure>
    );
  }

  if (mermaidError) {
    return fallback('invalid');
  }

  if (!mermaidSvg) {
    return (
      <div className="rich-block rich-block--note" aria-busy="true">
        {t('rich.loading')}
      </div>
    );
  }

  return (
    <figure className="rich-block">
      <iframe
        className="rich-block__frame"
        title={t('rich.mermaid.title')}
        sandbox={SANDBOX}
        referrerPolicy="no-referrer"
        srcDoc={frameDocument(mermaidSvg)}
      />
    </figure>
  );
}
