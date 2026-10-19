/**
 * Snippet highlighting for the search panel (SEARCH.md §11.3).
 *
 * The kernel marks matched terms in the snippet with `<mark>`; when a snippet
 * has no marks (e.g. vector-only matches, or mock data), the query terms are
 * highlighted client-side. Output is plain segments so the UI never injects
 * HTML.
 */

/** One run of snippet text, optionally a highlighted match. */
export interface SnippetSegment {
  text: string;
  marked: boolean;
}

const ENTITIES: Record<string, string> = {
  '&amp;': '&',
  '&lt;': '<',
  '&gt;': '>',
  '&quot;': '"',
  '&#39;': "'",
};

function decodeEntities(value: string): string {
  return value.replace(/&(?:amp|lt|gt|quot|#39);/g, (match) => ENTITIES[match] ?? match);
}

/** Split a kernel snippet on its `<mark>` tags. */
function parseMarked(snippet: string): SnippetSegment[] {
  const segments: SnippetSegment[] = [];
  const pattern = /<mark>(.*?)<\/mark>/g;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(snippet)) !== null) {
    if (match.index > last) {
      segments.push({ text: decodeEntities(snippet.slice(last, match.index)), marked: false });
    }
    segments.push({ text: decodeEntities(match[1] ?? ''), marked: true });
    last = match.index + match[0].length;
  }
  if (last < snippet.length) {
    segments.push({ text: decodeEntities(snippet.slice(last)), marked: false });
  }
  return segments;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Highlight whole query terms inside plain text, case-insensitively. */
function parseTerms(text: string, query: string): SnippetSegment[] {
  const terms = query
    .split(/\s+/)
    .map((term) => term.trim())
    .filter((term) => term.length > 1)
    .map(escapeRegExp);
  if (terms.length === 0) {
    return [{ text, marked: false }];
  }
  const pattern = new RegExp(`(${terms.join('|')})`, 'gi');
  const segments: SnippetSegment[] = [];
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text)) !== null) {
    if (match.index > last) {
      segments.push({ text: text.slice(last, match.index), marked: false });
    }
    segments.push({ text: match[0], marked: true });
    last = match.index + match[0].length;
  }
  if (last < text.length) {
    segments.push({ text: text.slice(last), marked: false });
  }
  return segments;
}

/** Turn a result snippet into highlightable segments. */
export function snippetSegments(snippet: string, query: string): SnippetSegment[] {
  if (/<mark>/.test(snippet)) {
    return parseMarked(snippet);
  }
  return parseTerms(decodeEntities(snippet), query);
}
