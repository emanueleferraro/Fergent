import { describe, expect, it } from 'vitest';

import { snippetSegments } from './highlight';

describe('snippetSegments', () => {
  it('parses kernel <mark> tags into marked segments', () => {
    const segments = snippetSegments('matched <mark>text</mark> here', 'text');

    expect(segments).toEqual([
      { text: 'matched ', marked: false },
      { text: 'text', marked: true },
      { text: ' here', marked: false },
    ]);
  });

  it('highlights query terms when the snippet has no marks', () => {
    const segments = snippetSegments('the MCP security note', 'mcp');

    expect(segments.filter((segment) => segment.marked).map((segment) => segment.text)).toEqual([
      'MCP',
    ]);
  });

  it('decodes HTML entities', () => {
    const segments = snippetSegments('a &amp; b &lt;c&gt;', '');

    expect(segments).toEqual([{ text: 'a & b <c>', marked: false }]);
  });
});
