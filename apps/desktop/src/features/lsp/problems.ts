/**
 * The Problems model (spec §3, task 3.8): kernel diagnostics are normalised
 * into severity-sorted, file-grouped problems, and into Monaco markers for the
 * inline squiggles. Pure data, so the panel and the markers share one truth.
 */
import type { LspDiagnostic, LspServerStatus } from '@fergents/protocol';

/** The four problem severities, in display order. */
export type ProblemSeverity = 'error' | 'warning' | 'info' | 'hint';

/** The badge tone for a language-server status (the design system's badges). */
export function serverStatusTone(status: LspServerStatus): string {
  switch (status) {
    case 'ready':
      return 'badge-green';
    case 'starting':
      return 'badge-yellow';
    case 'failed':
      return 'badge-red';
    case 'stopped':
      return 'badge-blue';
    default:
      return 'badge';
  }
}

/** The severities, most important first. */
export const PROBLEM_SEVERITIES: readonly ProblemSeverity[] = ['error', 'warning', 'info', 'hint'];

/** Map an LSP `DiagnosticSeverity` to a problem severity (unknown -> info). */
export function severityOf(lspSeverity: number): ProblemSeverity {
  switch (lspSeverity) {
    case 1:
      return 'error';
    case 2:
      return 'warning';
    case 4:
      return 'hint';
    default:
      return 'info';
  }
}

/** Monaco's `MarkerSeverity` values, used for the inline squiggles. */
export const MONACO_SEVERITY: Record<ProblemSeverity, number> = {
  error: 8,
  warning: 4,
  info: 2,
  hint: 1,
};

/** One normalised diagnostic. */
export interface Problem {
  path: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  severity: ProblemSeverity;
  message: string;
  source: string | null;
  code: string | null;
}

/** Problems for one file. */
export interface ProblemGroup {
  path: string;
  problems: Problem[];
  counts: Record<ProblemSeverity, number>;
}

const SEVERITY_RANK: Record<ProblemSeverity, number> = {
  error: 0,
  warning: 1,
  info: 2,
  hint: 3,
};

/** Counts per severity, all zero by default. */
export function emptyCounts(): Record<ProblemSeverity, number> {
  return { error: 0, warning: 0, info: 0, hint: 0 };
}

/** Normalise one kernel diagnostic. */
export function toProblem(diagnostic: LspDiagnostic): Problem {
  return {
    path: diagnostic.path,
    line: diagnostic.line,
    column: diagnostic.column,
    endLine: diagnostic.end_line,
    endColumn: diagnostic.end_column,
    severity: severityOf(diagnostic.severity),
    message: diagnostic.message,
    source: diagnostic.source ?? null,
    code: diagnostic.code ?? null,
  };
}

/** Total counts for a set of problems. */
export function countProblems(
  problems: readonly { severity: ProblemSeverity }[],
): Record<ProblemSeverity, number> {
  const counts = emptyCounts();
  for (const problem of problems) {
    counts[problem.severity] += 1;
  }
  return counts;
}

/** Group diagnostics by file, sorted by severity then position. */
export function groupProblems(diagnostics: readonly LspDiagnostic[]): ProblemGroup[] {
  const byPath = new Map<string, Problem[]>();
  for (const diagnostic of diagnostics) {
    const problem = toProblem(diagnostic);
    const list = byPath.get(diagnostic.path);
    if (list === undefined) {
      byPath.set(diagnostic.path, [problem]);
    } else {
      list.push(problem);
    }
  }
  return [...byPath.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([path, problems]) => ({
      path,
      problems: problems.sort(
        (left, right) =>
          SEVERITY_RANK[left.severity] - SEVERITY_RANK[right.severity] ||
          left.line - right.line ||
          left.column - right.column,
      ),
      counts: countProblems(problems),
    }));
}

/** Index every diagnostic by path (the shape the live event updates). */
export function indexDiagnostics(
  diagnostics: readonly LspDiagnostic[],
): Record<string, LspDiagnostic[]> {
  const index: Record<string, LspDiagnostic[]> = {};
  for (const diagnostic of diagnostics) {
    (index[diagnostic.path] ??= []).push(diagnostic);
  }
  return index;
}

/** Replace one file's diagnostics; an empty list removes the entry. */
export function mergeDiagnostics(
  index: Record<string, readonly LspDiagnostic[]>,
  path: string,
  diagnostics: readonly LspDiagnostic[],
): Record<string, LspDiagnostic[]> {
  const next: Record<string, LspDiagnostic[]> = {};
  for (const [key, list] of Object.entries(index)) {
    if (key !== path) {
      next[key] = [...list];
    }
  }
  if (diagnostics.length > 0) {
    next[path] = [...diagnostics];
  }
  return next;
}

/** Flatten an index back into one list (for the grouped view). */
export function flattenDiagnostics(
  index: Record<string, readonly LspDiagnostic[]>,
): LspDiagnostic[] {
  return Object.values(index).flat();
}

/** The Monaco marker for a problem (one-based lines and columns). */
export function toMonacoMarker(problem: Problem): Record<string, unknown> {
  return {
    startLineNumber: problem.line,
    startColumn: problem.column,
    endLineNumber: problem.endLine,
    endColumn: problem.endColumn,
    severity: MONACO_SEVERITY[problem.severity],
    message: problem.source === null ? problem.message : `${problem.message} (${problem.source})`,
    source: problem.source,
  };
}
