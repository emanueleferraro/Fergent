import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { t } from '../../i18n/i18n';

import { RichBlock } from './RichBlock';
import { sanitizeHtml, splitRichBlocks } from './rich';

afterEach(() => {
  cleanup();
});

const CHART = JSON.stringify({
  type: 'bar',
  title: 'Tokens',
  data: [
    { label: 'in', value: 10 },
    { label: 'out', value: 20 },
  ],
});

describe('Rich outputs', () => {
  it('renders a valid chart spec in a sandboxed frame', () => {
    render(<RichBlock block={{ kind: 'chart', body: CHART }} enabled />);

    const frame = screen.getByTitle(t('rich.chart.title')) as HTMLIFrameElement;
    expect(frame.getAttribute('sandbox')).toBe('allow-scripts');
    expect(frame.getAttribute('sandbox')).not.toContain('allow-same-origin');
    expect(frame.getAttribute('srcdoc')).toContain('<svg');
    expect(frame.getAttribute('srcdoc')).toContain("default-src 'none'");
    expect(screen.getByText('Tokens')).toBeDefined();
  });

  it('reports an invalid chart spec and falls back to the code block', () => {
    render(<RichBlock block={{ kind: 'chart', body: '[1, 2, 3]' }} enabled />);

    expect(screen.getByText(t('rich.invalid', { kind: t('rich.kind.chart') }))).toBeDefined();
    expect(screen.getByText('[1, 2, 3]')).toBeDefined();
    expect(screen.queryByTitle(t('rich.chart.title'))).toBeNull();
  });

  it('shows a plain code block when the capability is disabled', () => {
    render(<RichBlock block={{ kind: 'chart', body: CHART }} enabled={false} />);

    expect(screen.getByText(t('rich.disabled', { kind: t('rich.kind.chart') }))).toBeDefined();
    expect(screen.getByText(CHART)).toBeDefined();
    expect(screen.queryByTitle(t('rich.chart.title'))).toBeNull();
  });

  it('sanitizes HTML before it reaches the frame', () => {
    const dirty =
      '<p onclick="alert(1)" class="ok">hi</p><script>alert(2)</script>' +
      '<a href="javascript:alert(3)">link</a><img src="https://evil.test/x.png">';
    const clean = sanitizeHtml(dirty);

    expect(clean).not.toContain('onclick');
    expect(clean).not.toContain('<script');
    expect(clean).not.toContain('javascript:');
    expect(clean).not.toContain('evil.test');
    expect(clean).toContain('<p class="ok">hi</p>');
  });

  it('splits markdown and rich blocks in order', () => {
    const body = ['before', '```fergents-chart', CHART, '```', 'after'].join('\n');
    const segments = splitRichBlocks(body);

    expect(segments).toHaveLength(3);
    expect(segments[0]?.kind).toBe('markdown');
    expect(segments[1]).toEqual({ kind: 'rich', block: { kind: 'chart', body: `${CHART}\n` } });
    expect(segments[2]?.kind).toBe('markdown');
  });
});
