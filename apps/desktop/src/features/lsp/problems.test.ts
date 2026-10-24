import { describe, expect, it } from 'vitest';
import type { LspDiagnostic } from '@fergents/protocol';

import {
  countProblems,
  flattenDiagnostics,
  groupProblems,
  indexDiagnostics,
  mergeDiagnostics,
  MONACO_SEVERITY,
  severityOf,
  toMonacoMarker,
} from './problems';

function diagnostic(path: string, line: number, severity: number, message = 'm'): LspDiagnostic {
  return {
    path,
    line,
    column: 1,
    end_line: line,
    end_column: 4,
    severity,
    message,
    source: 'rust-analyzer',
    code: 'E0001',
  };
}

describe('severityOf', () => {
  it('maps LSP severities and defaults to info', () => {
    expect(severityOf(1)).toBe('error');
    expect(severityOf(2)).toBe('warning');
    expect(severityOf(3)).toBe('info');
    expect(severityOf(4)).toBe('hint');
    expect(severityOf(0)).toBe('info');
  });
});

describe('groupProblems', () => {
  it('groups by file and sorts by severity then position', () => {
    const groups = groupProblems([
      diagnostic('src/b.ts', 5, 2),
      diagnostic('src/a.ts', 9, 1),
      diagnostic('src/a.ts', 3, 2),
      diagnostic('src/a.ts', 1, 1),
    ]);

    expect(groups.map((group) => group.path)).toEqual(['src/a.ts', 'src/b.ts']);
    expect(groups[0]?.problems.map((problem) => [problem.severity, problem.line])).toEqual([
      ['error', 1],
      ['error', 9],
      ['warning', 3],
    ]);
    expect(groups[0]?.counts).toEqual({ error: 2, warning: 1, info: 0, hint: 0 });
    expect(groups[1]?.counts).toEqual({ error: 0, warning: 1, info: 0, hint: 0 });
  });

  it('counts problems across files', () => {
    const problems = groupProblems([
      diagnostic('a.ts', 1, 1),
      diagnostic('b.ts', 1, 2),
      diagnostic('b.ts', 2, 2),
    ]).flatMap((group) => group.problems);
    expect(countProblems(problems)).toEqual({ error: 1, warning: 2, info: 0, hint: 0 });
  });
});

describe('diagnostic index', () => {
  it('indexes by path and merges live updates', () => {
    const index = indexDiagnostics([diagnostic('a.ts', 1, 1), diagnostic('b.ts', 2, 2)]);
    expect(Object.keys(index)).toEqual(['a.ts', 'b.ts']);

    const updated = mergeDiagnostics(index, 'a.ts', [diagnostic('a.ts', 7, 2)]);
    expect(updated['a.ts']?.[0]?.line).toBe(7);
    expect(updated['b.ts']).toHaveLength(1);

    const cleared = mergeDiagnostics(updated, 'a.ts', []);
    expect(cleared['a.ts']).toBeUndefined();
    expect(flattenDiagnostics(cleared)).toHaveLength(1);
  });
});

describe('toMonacoMarker', () => {
  it('produces a one-based marker with Monaco severity', () => {
    const [problem] = groupProblems([diagnostic('a.ts', 3, 1, 'boom')])[0]?.problems ?? [];
    expect(problem).toBeDefined();
    const marker = problem === undefined ? {} : toMonacoMarker(problem);
    expect(marker).toMatchObject({
      startLineNumber: 3,
      startColumn: 1,
      endLineNumber: 3,
      endColumn: 4,
      severity: MONACO_SEVERITY.error,
      message: 'boom (rust-analyzer)',
      source: 'rust-analyzer',
    });
  });
});
