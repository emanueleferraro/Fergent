/**
 * The editor outline (spec §2): the kernel returns the language server's raw
 * `textDocument/documentSymbol` result, which may be `DocumentSymbol[]`
 * (hierarchical) or `SymbolInformation[]` (flat). Both are normalised to one
 * tree here.
 */

/** One outline entry. */
export interface OutlineItem {
  name: string;
  detail: string | null;
  /** LSP `SymbolKind`; kept opaque for the UI. */
  kind: number;
  /** One-based line of the symbol's identifier. */
  line: number;
  /** One-based column of the symbol's identifier. */
  column: number;
  children: OutlineItem[];
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null;
}

/** One-based position of an LSP `Position`, or `null`. */
function toPosition(value: unknown): { line: number; column: number } | null {
  const candidate = asRecord(value);
  if (candidate === null) {
    return null;
  }
  const { line, character } = candidate;
  if (typeof line !== 'number' || typeof character !== 'number') {
    return null;
  }
  return { line: line + 1, column: character + 1 };
}

/**
 * The identifier position of a symbol: `selectionRange` for `DocumentSymbol`,
 * the whole `range` for `SymbolInformation` (whose range lives in `location`).
 */
function identifierPosition(
  symbol: Record<string, unknown>,
): { line: number; column: number } | null {
  const selection = asRecord(symbol.selectionRange);
  const fromSelection = toPosition(selection?.start);
  if (fromSelection !== null) {
    return fromSelection;
  }
  const range = asRecord(symbol.range);
  const fromRange = toPosition(range?.start);
  if (fromRange !== null) {
    return fromRange;
  }
  const location = asRecord(symbol.location);
  return toPosition(asRecord(location?.range)?.start);
}

function toItem(value: unknown): OutlineItem | null {
  const symbol = asRecord(value);
  if (symbol === null || typeof symbol.name !== 'string') {
    return null;
  }
  const position = identifierPosition(symbol);
  if (position === null) {
    return null;
  }
  const children = Array.isArray(symbol.children)
    ? symbol.children.map(toItem).filter((item): item is OutlineItem => item !== null)
    : [];
  return {
    name: symbol.name,
    detail: typeof symbol.detail === 'string' && symbol.detail.length > 0 ? symbol.detail : null,
    kind: typeof symbol.kind === 'number' ? symbol.kind : 0,
    line: position.line,
    column: position.column,
    children,
  };
}

/** Normalise a raw `documentSymbol` result; invalid input yields no entries. */
export function parseOutline(result: unknown): OutlineItem[] {
  if (!Array.isArray(result)) {
    return [];
  }
  return result.map(toItem).filter((item): item is OutlineItem => item !== null);
}
