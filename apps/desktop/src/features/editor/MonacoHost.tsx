/**
 * The Monaco host: one editor instance mounted in the IDE view, one model per
 * open file. Everything else (documents, dirty state, saving) lives in
 * `useProjectEditor`, so this component only translates editor events into
 * controller calls.
 */
import { useEffect, useRef, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import { loadMonaco } from './monaco';
import {
  disposeAllModels,
  disposeModelsOutside,
  editorOptions,
  ensureModel,
  syncModelValue,
  themeName,
  type MonacoEditor,
  type MonacoLike,
  type MonacoModel,
} from './monacoAdapter';

/** A request to scroll a one-based line into view (outline, go-to-definition). */
export interface RevealRequest {
  line: number;
  /** Changes even when the line is the same, so the effect re-runs. */
  token: number;
}

/** Props for `MonacoHost`. */
export interface MonacoHostProps {
  path: string;
  /** The buffer content; changes only on load, reload, or a save. */
  content: string;
  languageId: string;
  readOnly: boolean;
  theme: 'dark' | 'light';
  /** Every open tab, so models of closed tabs can be disposed. */
  openPaths: readonly string[];
  reveal?: RevealRequest | null | undefined;
  onChange: (content: string) => void;
  onSave: () => void;
}

/** The Monaco editor instance for the active file. */
export function MonacoHost({
  path,
  content,
  languageId,
  readOnly,
  theme,
  openPaths,
  reveal,
  onChange,
  onSave,
}: MonacoHostProps): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const monacoRef = useRef<MonacoLike | null>(null);
  const editorRef = useRef<MonacoEditor | null>(null);
  const modelsRef = useRef<Map<string, MonacoModel>>(new Map());
  const modelPathRef = useRef<string | null>(null);
  const readOnlyRef = useRef<boolean | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;
  const themeRef = useRef(theme);
  themeRef.current = theme;
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);

  // Mount once: load the chunk, create the editor, wire the shortcuts.
  useEffect(() => {
    let cancelled = false;
    let subscription: { dispose(): void } | null = null;
    void (async () => {
      try {
        const monaco = await loadMonaco();
        if (cancelled || containerRef.current === null) {
          return;
        }
        monacoRef.current = monaco;
        monaco.editor.setTheme?.(themeName(themeRef.current));
        const editor = monaco.editor.create(containerRef.current, {
          ...editorOptions(readOnlyRef.current ?? false),
          model: null,
        });
        // Ctrl/Cmd+S saves through the kernel; Ctrl+F/Ctrl+H are Monaco defaults.
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => onSaveRef.current());
        subscription = editor.onDidChangeModelContent(() => onChangeRef.current(editor.getValue()));
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
      subscription?.dispose();
      editorRef.current?.dispose();
      editorRef.current = null;
      disposeAllModels(modelsRef.current);
      modelPathRef.current = null;
      readOnlyRef.current = null;
    };
  }, []);

  // Swap models and push external changes (reload) into the buffer.
  useEffect(() => {
    const monaco = monacoRef.current;
    const editor = editorRef.current;
    if (!ready || monaco === null || editor === null) {
      return;
    }
    const model = ensureModel(monaco, modelsRef.current, path, content, languageId);
    if (modelPathRef.current !== path) {
      modelPathRef.current = path;
      editor.setModel(model);
      editor.focus();
    }
    syncModelValue(model, content);
    if (readOnlyRef.current !== readOnly) {
      readOnlyRef.current = readOnly;
      editor.updateOptions(editorOptions(readOnly));
    }
  }, [ready, path, content, languageId, readOnly]);

  // Models follow tabs: closing a tab disposes its model (spec §2).
  useEffect(() => {
    if (ready) {
      disposeModelsOutside(modelsRef.current, new Set(openPaths));
    }
  }, [ready, openPaths]);

  useEffect(() => {
    if (ready) {
      monacoRef.current?.editor.setTheme?.(themeName(theme));
    }
  }, [ready, theme]);

  useEffect(() => {
    const editor = editorRef.current;
    if (!ready || editor === null || reveal === undefined || reveal === null) {
      return;
    }
    editor.revealLineInCenter(reveal.line);
    editor.setPosition({ lineNumber: reveal.line, column: 1 });
    editor.focus();
  }, [ready, reveal]);

  return (
    <>
      {failed ? <div className="inline-feedback error">{t('ide.editor.loadError')}</div> : null}
      <div className="ide-editor__monaco" data-testid="ide-monaco" ref={containerRef} />
    </>
  );
}
