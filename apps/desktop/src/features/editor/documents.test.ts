import { describe, expect, it } from 'vitest';
import type { FileSnapshot } from '@fergents/protocol';

import {
  MAX_EDITABLE_BYTES,
  activeDocument,
  documentFor,
  editorReducer,
  EMPTY_EDITOR_STATE,
  isDirty,
  isFileConflict,
  isReadOnlySnapshot,
  type EditorState,
} from './documents';

function snapshot(
  path: string,
  content = 'hello\n',
  overrides: Partial<FileSnapshot> = {},
): FileSnapshot {
  return {
    path,
    content,
    etag: `etag-${content.length}`,
    size: content.length,
    language_id: 'typescript',
    truncated: false,
    binary: false,
    ...overrides,
  };
}

function open(state: EditorState, path: string, content = 'hello\n', preview = true): EditorState {
  return editorReducer(state, {
    type: 'open',
    snapshot: snapshot(path, content),
    preview,
    trusted: true,
  });
}

describe('document guards', () => {
  it('opens binary, truncated, and oversized files read-only', () => {
    expect(isReadOnlySnapshot(snapshot('a.ts'))).toBe(false);
    expect(isReadOnlySnapshot(snapshot('a.png', '', { binary: true }))).toBe(true);
    expect(isReadOnlySnapshot(snapshot('a.log', '', { truncated: true }))).toBe(true);
    expect(isReadOnlySnapshot(snapshot('a.log', '', { size: MAX_EDITABLE_BYTES + 1 }))).toBe(true);
  });

  it('keeps read-only documents unchanged on edit', () => {
    const state = editorReducer(EMPTY_EDITOR_STATE, {
      type: 'open',
      snapshot: snapshot('logo.png', '', { binary: true }),
      preview: false,
      trusted: true,
    });
    const changed = editorReducer(state, { type: 'change', path: 'logo.png', content: 'x' });
    expect(documentFor(changed, 'logo.png')?.content).toBe('');
  });

  it('marks documents read-only in untrusted projects', () => {
    const state = editorReducer(EMPTY_EDITOR_STATE, {
      type: 'open',
      snapshot: snapshot('a.ts'),
      preview: false,
      trusted: false,
    });
    expect(documentFor(state, 'a.ts')?.readOnly).toBe(true);
  });
});

describe('editorReducer', () => {
  it('tracks dirty state and pins previews on edit', () => {
    let state = open(EMPTY_EDITOR_STATE, 'a.ts');
    const document = documentFor(state, 'a.ts');
    expect(document?.preview).toBe(true);
    expect(document === null ? null : isDirty(document)).toBe(false);

    state = editorReducer(state, { type: 'change', path: 'a.ts', content: 'changed' });
    const dirty = documentFor(state, 'a.ts');
    expect(dirty === null ? null : isDirty(dirty)).toBe(true);
    expect(dirty?.preview).toBe(false);

    state = editorReducer(state, { type: 'change', path: 'a.ts', content: 'hello\n' });
    const reverted = documentFor(state, 'a.ts');
    expect(reverted === null ? null : isDirty(reverted)).toBe(false);
  });

  it('replaces the previous preview tab', () => {
    let state = open(EMPTY_EDITOR_STATE, 'a.ts');
    state = open(state, 'b.ts');
    expect(state.documents.map((document) => document.path)).toEqual(['b.ts']);
    expect(state.activePath).toBe('b.ts');

    state = open(state, 'c.ts', 'hello\n', false);
    expect(state.documents.map((document) => document.path)).toEqual(['b.ts', 'c.ts']);
  });

  it('activates and pins an existing tab instead of duplicating it', () => {
    let state = open(EMPTY_EDITOR_STATE, 'a.ts');
    state = open(state, 'b.ts', 'hello\n', false);
    state = open(state, 'a.ts', 'hello\n', false);
    expect(state.documents).toHaveLength(2);
    expect(documentFor(state, 'a.ts')?.preview).toBe(false);
    expect(state.activePath).toBe('a.ts');
    expect(activeDocument(state)?.path).toBe('a.ts');
  });

  it('selects the neighbouring tab when the active one closes', () => {
    let state = open(EMPTY_EDITOR_STATE, 'a.ts', 'hello\n', false);
    state = open(state, 'b.ts', 'hello\n', false);
    state = open(state, 'c.ts', 'hello\n', false);
    state = editorReducer(state, { type: 'select', path: 'b.ts' });
    state = editorReducer(state, { type: 'close', path: 'b.ts' });
    expect(state.activePath).toBe('c.ts');
    expect(state.documents.map((document) => document.path)).toEqual(['a.ts', 'c.ts']);
  });

  it('clears the active path when the last tab closes', () => {
    let state = open(EMPTY_EDITOR_STATE, 'a.ts');
    state = editorReducer(state, { type: 'close', path: 'a.ts' });
    expect(state).toEqual(EMPTY_EDITOR_STATE);
  });

  it('keeps the saved etag and content after a save', () => {
    let state = open(EMPTY_EDITOR_STATE, 'a.ts');
    state = editorReducer(state, { type: 'change', path: 'a.ts', content: 'next\n' });
    state = editorReducer(state, { type: 'saved', snapshot: snapshot('a.ts', 'next\n') });
    const saved = documentFor(state, 'a.ts');
    expect(saved?.etag).toBe('etag-5');
    expect(saved === null ? null : isDirty(saved)).toBe(false);
  });

  it('reloads an external change but keeps the read-only flag', () => {
    let state = editorReducer(EMPTY_EDITOR_STATE, {
      type: 'open',
      snapshot: snapshot('a.ts', 'hello\n', { truncated: true }),
      preview: false,
      trusted: true,
    });
    state = editorReducer(state, { type: 'reload', snapshot: snapshot('a.ts', 'new\n') });
    const reloaded = documentFor(state, 'a.ts');
    expect(reloaded?.content).toBe('new\n');
    expect(reloaded?.readOnly).toBe(true);
  });

  it('resets every tab', () => {
    const state = open(EMPTY_EDITOR_STATE, 'a.ts');
    expect(editorReducer(state, { type: 'reset' })).toEqual(EMPTY_EDITOR_STATE);
  });
});

describe('isFileConflict', () => {
  it('recognises the kernel conflict refusal', () => {
    expect(isFileConflict(new Error('rpc error -32008: file conflict: a.ts'))).toBe(true);
    expect(isFileConflict('file conflict: a.ts')).toBe(true);
    expect(isFileConflict(new Error('rpc error -32602: invalid params'))).toBe(false);
  });
});
