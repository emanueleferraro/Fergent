import { describe, expect, it } from 'vitest';

import { renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
  it('renders wiki links with an alias', () => {
    const html = renderMarkdown('See [[MCP Security]] and [[notes/a|the note]].');

    expect(html).toContain('class="wikilink"');
    expect(html).toContain('data-wikilink="MCP Security"');
    expect(html).toContain('>the note</a>');
  });

  it('renders inline and block math through KaTeX', () => {
    const html = renderMarkdown('Euler: $e^{i\\pi}+1=0$.\n\n$$\nx^2 + y^2\n$$');

    expect(html).toContain('class="math-inline"');
    expect(html).toContain('class="math-block"');
    expect(html).toContain('katex');
  });

  it('renders callouts with the title from the marker', () => {
    const html = renderMarkdown('> [!warning] Careful\n> watch out');

    expect(html).toContain('class="callout warning"');
    expect(html).toContain('Careful');
    expect(html).toContain('watch out');
  });

  it('uses i18n callout titles when provided', () => {
    const html = renderMarkdown('> [!note]\n> body', { calloutTitles: { note: 'Nota' } });

    expect(html).toContain('class="callout note"');
    expect(html).toContain('Nota');
    expect(html).toContain('body');
  });

  it('leaves Mermaid fences as a placeholder for hydration', () => {
    const html = renderMarkdown('```mermaid\ngraph TD; A-->B;\n```');

    expect(html).toContain('<pre class="mermaid">');
    expect(html).toContain('graph TD');
  });

  it('renders GFM tables', () => {
    const html = renderMarkdown('| a | b |\n| - | - |\n| 1 | 2 |');

    expect(html).toContain('<table>');
  });

  it('does not pass raw HTML through', () => {
    const html = renderMarkdown('<script>alert(1)</script>');

    expect(html).not.toContain('<script>');
  });
});
