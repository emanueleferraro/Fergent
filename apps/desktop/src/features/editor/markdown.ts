/**
 * Renders the Fergents markdown flavor to HTML for the live preview
 * (`BRAIN.md` §4.2). CommonMark and GFM tables come from `markdown-it`;
 * wiki links, callouts, KaTeX math, and Mermaid fences are added on top.
 *
 * The renderer is pure (no DOM, no side effects) so it is unit-testable.
 * Mermaid needs a DOM, so it is emitted as a placeholder (`.mermaid`) that
 * `MarkdownPreview` hydrates in the browser.
 *
 * The rule helpers use small structural types instead of `markdown-it`'s
 * type namespace (which is not importable under `export =` + ESM); each is a
 * supertype of the corresponding markdown-it state, so registration type-checks.
 */
import katex from 'katex';
import MarkdownIt from 'markdown-it';

type Nesting = -1 | 0 | 1;

/** The token fields the renderer rules touch. */
interface MdToken {
  type: string;
  content: string;
  info: string;
  map: [number, number] | null;
  meta: unknown;
  children: MdToken[] | null;
}

interface MdInlineState {
  src: string;
  pos: number;
  posMax: number;
  push(type: string, tag: string, nesting: Nesting): MdToken;
}

interface MdBlockState {
  src: string;
  bMarks: number[];
  tShift: number[];
  eMarks: number[];
  line: number;
  push(type: string, tag: string, nesting: Nesting): MdToken;
}

interface MdCoreState {
  tokens: MdToken[];
}

const CALLOUT_TYPES = new Set(['note', 'tip', 'warning', 'danger', 'info', 'success']);

/** Options for `renderMarkdown`. */
export interface RenderOptions {
  /** Human titles per callout type (i18n); falls back to the capitalized type. */
  calloutTitles?: Readonly<Record<string, string>> | undefined;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function renderMath(tex: string, displayMode: boolean): string {
  try {
    return katex.renderToString(tex, { displayMode, throwOnError: false, output: 'html' });
  } catch {
    return `<code>${escapeHtml(tex)}</code>`;
  }
}

/** Inline `[[target|text]]` / `[[target#heading]]` wiki links. */
function wikilinkRule(state: MdInlineState, silent: boolean): boolean {
  const { src, pos } = state;
  if (src[pos] !== '[' || src[pos + 1] !== '[') {
    return false;
  }
  const end = src.indexOf(']]', pos + 2);
  if (end < 0) {
    return false;
  }
  const inner = src.slice(pos + 2, end);
  if (inner.length === 0 || inner.includes('[')) {
    return false;
  }
  if (!silent) {
    state.push('wikilink', 'a', 0).content = inner;
  }
  state.pos = end + 2;
  return true;
}

function splitWikilink(inner: string): {
  path: string;
  heading: string | undefined;
  label: string;
} {
  const [targetRaw, aliasRaw] = inner.split('|');
  const target = (targetRaw ?? '').trim();
  const hash = target.indexOf('#');
  const path = hash >= 0 ? target.slice(0, hash) : target;
  const heading = hash >= 0 ? target.slice(hash + 1) : undefined;
  const alias = aliasRaw?.trim();
  const label =
    alias && alias.length > 0 ? alias : heading ? `${path || 'note'} › ${heading}` : target;
  return { path, heading, label };
}

function renderWikilink(inner: string): string {
  const { path, heading, label } = splitWikilink(inner);
  const href = `fergents://note/${encodeURIComponent(path)}${
    heading ? `#${encodeURIComponent(heading)}` : ''
  }`;
  return `<a class="wikilink" href="${escapeHtml(href)}" data-wikilink="${escapeHtml(
    path,
  )}">${escapeHtml(label)}</a>`;
}

/** Inline math: `$tex$`. */
function mathInlineRule(state: MdInlineState, silent: boolean): boolean {
  const { src, pos, posMax } = state;
  if (src[pos] !== '$' || src[pos + 1] === '$') {
    return false;
  }
  let end = pos + 1;
  while (end < posMax && src[end] !== '$') {
    end++;
  }
  if (end >= posMax || end === pos + 1) {
    return false;
  }
  if (!silent) {
    state.push('math_inline', 'math', 0).content = src.slice(pos + 1, end);
  }
  state.pos = end + 1;
  return true;
}

/** Block math: `$$tex$$`, single- or multi-line. */
function mathBlockRule(
  state: MdBlockState,
  startLine: number,
  endLine: number,
  silent: boolean,
): boolean {
  const start = state.bMarks[startLine]! + state.tShift[startLine]!;
  const max = state.eMarks[startLine]!;
  if (state.src.slice(start, start + 2) !== '$$') {
    return false;
  }
  const firstLine = state.src.slice(start + 2, max);
  const inlineClose = firstLine.indexOf('$$');
  if (inlineClose >= 0) {
    if (!silent) {
      const token = state.push('math_block', 'math', 0);
      token.content = firstLine.slice(0, inlineClose);
      token.map = [startLine, startLine + 1];
    }
    state.line = startLine + 1;
    return true;
  }
  let content = firstLine;
  let line = startLine + 1;
  let closed = false;
  for (; line < endLine; line++) {
    const lineStart = state.bMarks[line]! + state.tShift[line]!;
    const text = state.src.slice(lineStart, state.eMarks[line]!);
    const closeAt = text.indexOf('$$');
    if (closeAt >= 0) {
      content += `\n${text.slice(0, closeAt)}`;
      closed = true;
      break;
    }
    content += `\n${text}`;
  }
  if (!closed) {
    return false;
  }
  if (!silent) {
    const token = state.push('math_block', 'math', 0);
    token.content = content.trim();
    token.map = [startLine, line + 1];
  }
  state.line = line + 1;
  return true;
}

/** Detect `> [!type]` blockquotes and tag the open/close tokens. */
function calloutCoreRule(state: MdCoreState): void {
  const tokens = state.tokens;
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i]?.type !== 'blockquote_open') {
      continue;
    }
    const inline = tokens[i + 2];
    if (inline?.type !== 'inline') {
      continue;
    }
    const lines = inline.content.split('\n');
    const match = /^\[!([a-zA-Z]+)\][+-]?\s*(.*)$/.exec((lines[0] ?? '').trim());
    if (!match) {
      continue;
    }
    const type = (match[1] ?? '').toLowerCase();
    if (!CALLOUT_TYPES.has(type)) {
      continue;
    }
    const title = (match[2] ?? '').trim();
    inline.content = lines.slice(1).join('\n');
    const first = inline.children?.[0];
    if (first?.type === 'text') {
      first.content = '';
    }
    let depth = 0;
    let closeIdx = -1;
    for (let j = i; j < tokens.length; j++) {
      if (tokens[j]?.type === 'blockquote_open') {
        depth++;
      } else if (tokens[j]?.type === 'blockquote_close') {
        depth--;
        if (depth === 0) {
          closeIdx = j;
          break;
        }
      }
    }
    const meta = { callout: type, title };
    const open = tokens[i];
    if (open) {
      open.meta = meta;
    }
    const close = closeIdx >= 0 ? tokens[closeIdx] : undefined;
    if (close) {
      close.meta = meta;
    }
  }
}

function createMarkdownIt() {
  const md = new MarkdownIt({ html: false, linkify: true, typographer: false });

  md.inline.ruler.before('link', 'wikilink', wikilinkRule);
  md.inline.ruler.before('link', 'math_inline', mathInlineRule);
  md.block.ruler.before('fence', 'math_block', mathBlockRule);
  md.core.ruler.after('inline', 'callouts', calloutCoreRule);

  md.renderer.rules.wikilink = (tokens, idx) => renderWikilink(tokens[idx]?.content ?? '');
  md.renderer.rules.math_inline = (tokens, idx) =>
    `<span class="math-inline">${renderMath(tokens[idx]?.content ?? '', false)}</span>`;
  md.renderer.rules.math_block = (tokens, idx) =>
    `<div class="math-block">${renderMath(tokens[idx]?.content ?? '', true)}</div>`;

  const defaultFence = md.renderer.rules.fence;
  md.renderer.rules.fence = (tokens, idx, options, env, self) => {
    const token = tokens[idx];
    const info = (token?.info ?? '').trim().split(/\s+/)[0] ?? '';
    if (info === 'mermaid') {
      return `<div class="mermaid-block"><pre class="mermaid">${escapeHtml(
        token?.content ?? '',
      )}</pre></div>`;
    }
    return defaultFence
      ? defaultFence(tokens, idx, options, env, self)
      : self.renderToken(tokens, idx, options);
  };

  const defaultQuoteOpen = md.renderer.rules.blockquote_open;
  const defaultQuoteClose = md.renderer.rules.blockquote_close;
  md.renderer.rules.blockquote_open = (tokens, idx, options, env, self) => {
    const meta = tokens[idx]?.meta as { callout?: string; title?: string } | null | undefined;
    if (meta?.callout) {
      const titles = (env as { calloutTitles?: Readonly<Record<string, string>> } | undefined)
        ?.calloutTitles;
      const title =
        meta.title && meta.title.length > 0
          ? meta.title
          : (titles?.[meta.callout] ??
            meta.callout.charAt(0).toUpperCase() + meta.callout.slice(1));
      return `<div class="callout ${meta.callout}"><div class="callout-body"><div class="callout-title">${escapeHtml(
        title,
      )}</div>\n`;
    }
    return defaultQuoteOpen
      ? defaultQuoteOpen(tokens, idx, options, env, self)
      : self.renderToken(tokens, idx, options);
  };
  md.renderer.rules.blockquote_close = (tokens, idx, options, env, self) => {
    const meta = tokens[idx]?.meta as { callout?: string } | null | undefined;
    if (meta?.callout) {
      return '</div></div>\n';
    }
    return defaultQuoteClose
      ? defaultQuoteClose(tokens, idx, options, env, self)
      : self.renderToken(tokens, idx, options);
  };

  return md;
}

const renderer = createMarkdownIt();

/** Render Fergents markdown to HTML for the preview. */
export function renderMarkdown(source: string, options: RenderOptions = {}): string {
  return renderer.render(source, { calloutTitles: options.calloutTitles });
}
