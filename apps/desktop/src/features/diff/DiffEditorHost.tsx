/**
 * The Monaco diff host (UI.md §8.7, spec D15): renders one proposed file as an
 * inline diff (side-by-side on request). Hunk approval lives in `DiffPreview`;
 * this component is only the read-only viewer.
 */
import { useEffect, useRef, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import { loadMonaco } from '../editor/monaco';
import { monacoLanguage, themeName, type MonacoModel } from '../editor/monacoAdapter';

import {
  diffModelUri,
  diffOptions,
  type MonacoDiffEditor,
  type MonacoDiffLike,
} from './diffAdapter';

/** Props for `DiffEditorHost`. */
export interface DiffEditorHostProps {
  path: string;
  original: string;
  proposed: string;
  languageId: string;
  theme: 'dark' | 'light';
  sideBySide: boolean;
}

/** The Monaco diff editor for one file. */
export function DiffEditorHost({
  path,
  original,
  proposed,
  languageId,
  theme,
  sideBySide,
}: DiffEditorHostProps): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const monacoRef = useRef<MonacoDiffLike | null>(null);
  const editorRef = useRef<MonacoDiffEditor | null>(null);
  const originalRef = useRef<MonacoModel | null>(null);
  const modifiedRef = useRef<MonacoModel | null>(null);
  const latest = useRef({ original, proposed, languageId, theme, sideBySide });
  latest.current = { original, proposed, languageId, theme, sideBySide };
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);

  // Mount once per file: create the two models and the diff editor.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const monaco = (await loadMonaco()) as MonacoDiffLike;
        if (cancelled || containerRef.current === null) {
          return;
        }
        const current = latest.current;
        monacoRef.current = monaco;
        monaco.editor.setTheme?.(themeName(current.theme));
        const originalModel = monaco.editor.createModel(
          current.original,
          monacoLanguage(current.languageId),
          diffModelUri(monaco, path, 'original'),
        );
        const modifiedModel = monaco.editor.createModel(
          current.proposed,
          monacoLanguage(current.languageId),
          diffModelUri(monaco, path, 'proposed'),
        );
        const editor = monaco.editor.createDiffEditor(
          containerRef.current,
          diffOptions(current.sideBySide),
        );
        editor.setModel({ original: originalModel, modified: modifiedModel });
        originalRef.current = originalModel;
        modifiedRef.current = modifiedModel;
        editorRef.current = editor;
        setReady(true);
      } catch {
        if (!cancelled) {
          setFailed(true);
        }
      }
    })();
    return () => {
      cancelled = true;
      editorRef.current?.dispose();
      editorRef.current = null;
      originalRef.current?.dispose();
      originalRef.current = null;
      modifiedRef.current?.dispose();
      modifiedRef.current = null;
      monacoRef.current = null;
    };
  }, [path]);

  // Push a new proposal into the existing models.
  useEffect(() => {
    if (!ready) {
      return;
    }
    if (originalRef.current !== null && originalRef.current.getValue() !== original) {
      originalRef.current.setValue(original);
    }
    if (modifiedRef.current !== null && modifiedRef.current.getValue() !== proposed) {
      modifiedRef.current.setValue(proposed);
    }
  }, [ready, original, proposed]);

  useEffect(() => {
    if (ready) {
      editorRef.current?.updateOptions({ renderSideBySide: sideBySide });
    }
  }, [ready, sideBySide]);

  useEffect(() => {
    if (ready) {
      monacoRef.current?.editor.setTheme?.(themeName(theme));
    }
  }, [ready, theme]);

  return (
    <>
      {failed ? <div className="inline-feedback error">{t('diff.loadError')}</div> : null}
      <div className="diff-editor" data-testid="diff-editor" ref={containerRef} />
    </>
  );
}
