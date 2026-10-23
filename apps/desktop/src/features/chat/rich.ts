/**
 * Rich outputs (Task 2.15): typed fenced blocks in assistant messages, the
 * chart spec schema, and the sandboxed frame documents they render into.
 *
 * Everything here is pure and testable; `RichBlock.tsx` does the rendering.
 */

/** The rich block kinds the chat understands. */
export type RichKind = 'chart' | 'mermaid' | 'html';

/** A parsed rich block. */
export interface RichBlockData {
  kind: RichKind;
  body: string;
}

/** One piece of a message body: markdown or a rich block. */
export type BodySegment =
  { kind: 'markdown'; text: string } | { kind: 'rich'; block: RichBlockData };

const RICH_FENCE = /^```fergents-(chart|mermaid|html)[ \t]*\r?\n([\s\S]*?)^```[ \t]*$/gm;

/** Split a message body into markdown and rich-block segments. */
export function splitRichBlocks(source: string): BodySegment[] {
  const segments: BodySegment[] = [];
  let cursor = 0;
  RICH_FENCE.lastIndex = 0;
  let match = RICH_FENCE.exec(source);
  while (match) {
    if (match.index > cursor) {
      segments.push({ kind: 'markdown', text: source.slice(cursor, match.index) });
    }
    segments.push({
      kind: 'rich',
      block: { kind: match[1] as RichKind, body: match[2] ?? '' },
    });
    cursor = match.index + match[0].length;
    match = RICH_FENCE.exec(source);
  }
  if (cursor === 0) {
    return [{ kind: 'markdown', text: source }];
  }
  if (cursor < source.length) {
    segments.push({ kind: 'markdown', text: source.slice(cursor) });
  }
  return segments;
}

/** A validated chart spec. */
export interface ChartPoint {
  label: string;
  value: number;
}

/** A validated chart spec. */
export interface ChartSpec {
  type: 'bar' | 'line' | 'pie';
  title: string | null;
  data: ChartPoint[];
}

/** The maximum number of points a chart may render. */
export const MAX_CHART_POINTS = 200;

/** Validate a `fergents-chart` JSON body. */
export function parseChartSpec(body: string): { spec: ChartSpec } | { error: string } {
  let raw: unknown;
  try {
    raw = JSON.parse(body);
  } catch {
    return { error: 'invalid_json' };
  }
  if (typeof raw !== 'object' || raw === null) {
    return { error: 'invalid_spec' };
  }
  const record = raw as Record<string, unknown>;
  const type = record.type;
  if (type !== 'bar' && type !== 'line' && type !== 'pie') {
    return { error: 'invalid_type' };
  }
  const rows = record.data;
  if (!Array.isArray(rows) || rows.length === 0) {
    return { error: 'invalid_data' };
  }
  if (rows.length > MAX_CHART_POINTS) {
    return { error: 'too_many_points' };
  }
  const data: ChartPoint[] = [];
  for (const row of rows) {
    if (typeof row !== 'object' || row === null) {
      return { error: 'invalid_data' };
    }
    const point = row as Record<string, unknown>;
    if (typeof point.label !== 'string' || typeof point.value !== 'number') {
      return { error: 'invalid_data' };
    }
    if (!Number.isFinite(point.value)) {
      return { error: 'invalid_data' };
    }
    data.push({ label: point.label.slice(0, 80), value: point.value });
  }
  const title = typeof record.title === 'string' ? record.title.slice(0, 120) : null;
  return { spec: { type, title, data } };
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

const CHART_WIDTH = 640;
const CHART_HEIGHT = 320;
const PAD = 32;
const PALETTE = ['#D97757', '#6B94D9', '#7FB894', '#E5B84D', '#B07FD9', '#5FB3B3'];

/** Render a validated spec to a standalone SVG string. */
export function chartSvg(spec: ChartSpec): string {
  const innerW = CHART_WIDTH - PAD * 2;
  const innerH = CHART_HEIGHT - PAD * 2;
  const values = spec.data.map((point) => point.value);
  const min = Math.min(0, ...values);
  const max = Math.max(...values, min + 1);
  const span = max - min;
  const scale = (value: number): number => innerH - ((value - min) / span) * innerH;

  let shapes = '';
  if (spec.type === 'pie') {
    const total = values.reduce((sum, value) => sum + Math.max(0, value), 0) || 1;
    let angle = -Math.PI / 2;
    const cx = CHART_WIDTH / 2;
    const cy = CHART_HEIGHT / 2;
    const radius = Math.min(innerW, innerH) / 2 - 8;
    shapes = spec.data
      .map((point, index) => {
        const slice = (Math.max(0, point.value) / total) * Math.PI * 2;
        const startX = cx + radius * Math.cos(angle);
        const startY = cy + radius * Math.sin(angle);
        angle += slice;
        const endX = cx + radius * Math.cos(angle);
        const endY = cy + radius * Math.sin(angle);
        const large = slice > Math.PI ? 1 : 0;
        return `<path d="M${cx},${cy} L${startX},${startY} A${radius},${radius} 0 ${large} 1 ${endX},${endY} Z" fill="${PALETTE[index % PALETTE.length]}" opacity="0.85"><title>${escapeXml(point.label)}: ${point.value}</title></path>`;
      })
      .join('');
  } else {
    const step = spec.data.length > 1 ? innerW / (spec.data.length - 1) : innerW;
    const points = spec.data.map((point, index) => ({
      x: PAD + index * step,
      y: PAD + scale(point.value),
    }));
    if (spec.type === 'line') {
      shapes = `<polyline fill="none" stroke="#D97757" stroke-width="2" points="${points
        .map((point) => `${point.x},${point.y}`)
        .join(' ')}"/>${points
        .map(
          (point, index) =>
            `<circle cx="${point.x}" cy="${point.y}" r="3" fill="#D97757"><title>${escapeXml(
              spec.data[index]?.label ?? '',
            )}: ${spec.data[index]?.value ?? 0}</title></circle>`,
        )
        .join('')}`;
    } else {
      const barW = Math.max(2, innerW / spec.data.length - 6);
      shapes = spec.data
        .map((point, index) => {
          const x = PAD + index * (innerW / spec.data.length) + 3;
          const y = PAD + scale(point.value);
          const height = PAD + innerH - y;
          return `<rect x="${x}" y="${y}" width="${barW}" height="${Math.max(0, height)}" rx="3" fill="${PALETTE[index % PALETTE.length]}" opacity="0.85"><title>${escapeXml(point.label)}: ${point.value}</title></rect>`;
        })
        .join('');
    }
  }

  const title = spec.title
    ? `<text x="${PAD}" y="20" fill="#98989F" font-size="12">${escapeXml(spec.title)}</text>`
    : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${CHART_WIDTH}" height="${CHART_HEIGHT}" viewBox="0 0 ${CHART_WIDTH} ${CHART_HEIGHT}" role="img">${title}${shapes}</svg>`;
}

const ALLOWED_TAGS = new Set([
  'a',
  'article',
  'b',
  'blockquote',
  'br',
  'code',
  'div',
  'em',
  'figcaption',
  'figure',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'hr',
  'i',
  'img',
  'li',
  'ol',
  'p',
  'pre',
  'section',
  'small',
  'span',
  'strong',
  'sub',
  'sup',
  'table',
  'tbody',
  'td',
  'th',
  'thead',
  'tr',
  'u',
  'ul',
]);

const DROPPED_TAGS = new Set([
  'base',
  'button',
  'embed',
  'form',
  'iframe',
  'input',
  'link',
  'meta',
  'object',
  'script',
  'style',
  'template',
]);

const ALLOWED_ATTRS = new Set(['alt', 'class', 'colspan', 'href', 'rowspan', 'src', 'title']);

/** Sanitize an HTML fragment with a tag/attribute allowlist. */
export function sanitizeHtml(source: string): string {
  if (typeof DOMParser === 'undefined') {
    return '';
  }
  const doc = new DOMParser().parseFromString(source, 'text/html');
  const walk = (parent: Element, depth: number): void => {
    if (depth > 32) {
      parent.textContent = '';
      return;
    }
    for (const node of Array.from(parent.children)) {
      const tag = node.tagName.toLowerCase();
      if (DROPPED_TAGS.has(tag)) {
        node.remove();
        continue;
      }
      if (!ALLOWED_TAGS.has(tag)) {
        node.replaceWith(...Array.from(node.childNodes));
        continue;
      }
      for (const attr of Array.from(node.attributes)) {
        const name = attr.name.toLowerCase();
        const value = attr.value.trim();
        const unsafeHref = name === 'href' && /^\s*javascript:/i.test(value);
        const unsafeSrc = name === 'src' && !/^data:image\//i.test(value);
        if (!ALLOWED_ATTRS.has(name) || name.startsWith('on') || unsafeHref || unsafeSrc) {
          node.removeAttribute(attr.name);
        }
      }
      walk(node, depth + 1);
    }
  };
  walk(doc.body, 0);
  return doc.body.innerHTML;
}

/** Wrap a fragment in a CSP-locked document for an `<iframe sandbox>`. */
export function frameDocument(body: string, background = 'transparent'): string {
  const style = [
    'html,body{margin:0;padding:0;background:' + background + ';color:#EDEDEF;',
    'font-family:Inter,-apple-system,"Segoe UI",Roboto,sans-serif;font-size:13px;',
    'overflow:auto;max-height:420px}',
    'svg{max-width:100%;height:auto}',
    'table{border-collapse:collapse}',
    'td,th{border:1px solid rgba(255,255,255,0.13);padding:4px 8px}',
  ].join('');
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'"><style>${style}</style></head><body>${body}</body></html>`;
}
