/**
 * The Monaco host: one editor instance mounted in the IDE view, one model per
 * open file. Everything else (documents, dirty state, saving) lives in
 * `useProjectEditor`, so this component only translates editor events into
 * controller calls.
 */
import { useEffect, useRef, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import { toMonacoMarker, type Problem } from '../lsp/problems';
import { loadMonaco } from './monaco';
import {
  disposeAllModels,
  disposeModelsOutside,
  editorOptions,
  ensureModel,
  modelPath,
  monacoLanguage,
  syncModelValue,
  themeName,
  type MonacoEditor,
  type MonacoHoverProvider,
  type MonacoInlineProvider,
  type MonacoLike,
  type MonacoModel,
} from './monacoAdapter';

/** A request to scroll a one-based position into view (outline, problems). */
export interface RevealRequest {
  line: number;
  column?: number | undefined;
  /** Changes even when the line is the same, so the effect re-runs. */
  token: number;
}

/** The LSP operations the host wires into Monaco (positions are one-based). */
export interface MonacoHostLsp {
  hover: (path: string, line: number, column: number) => Promise<string | null>;
  onDefinition: (path: string, line: number, column: number) => void;
  onRename: (path: string, line: number, column: number) => void;
  onCodeActions: (path: string, line: number, column: number) => void;
}

/** The inline AI surface the host wires into Monaco (task 3.12). */
export interface MonacoHostInline {
  /** Resolve a ghost-text suggestion for the cursor, or `null` for none. */
  complete: (
    path: string,
    text: string,
    offset: number,
    languageId: string,
    readOnly: boolean,
  ) => Promise<string | null>;
  /** `Ctrl+.` on the current selection (quick actions, never in-place edits). */
  onQuickActions: (path: string, line: number, column: number, selection: string) => void;
}

/** A non-empty editor selection, as the chat context builder needs it. */
export interface MonacoSelection {
  path: string;
  startLine: number;
  endLine: number;
  text: string;
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
  /** Diagnostics of the active file, rendered as inline squiggles. */
  markers?: readonly Problem[] | undefined;
  /** LSP interactions, or `null` when the project is untrusted. */
  lsp?: MonacoHostLsp | null | undefined;
  /** Inline AI, or `null` when the project is untrusted. */
  inline?: MonacoHostInline | null | undefined;
  /** Reports the current non-empty selection (task 3.14, @selection). */
  onSelection?: ((selection: MonacoSelection | null) => void) | undefined;
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
  markers,
  lsp,
  inline,
  onSelection,
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
  const lspRef = useRef(lsp);
  lspRef.current = lsp;
  const inlineRef = useRef(inline);
  inlineRef.current = inline;
  const onSelectionRef = useRef(onSelection);
  onSelectionRef.current = onSelection;
  const hoverProviders = useRef<{ dispose(): void }[]>([]);
  const hoverLanguages = useRef<Set<string>>(new Set());
  const inlineProviders = useRef<{ dispose(): void }[]>([]);
  const inlineLanguages = useRef<Set<string>>(new Set());
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);

  // Mount once: load the chunk, create the editor, wire the shortcuts.
  useEffect(() => {
    let cancelled = false;
    let subscription: { dispose(): void } | null = null;
    let selectionSubscription: { dispose(): void } | null = null;
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
        // The cursor position of the active model, or `null` before a model.
        const cursor = (): { path: string; line: number; column: number } | null => {
          const position = editor.getPosition();
          const current = modelPathRef.current;
          return position === null || current === null
            ? null
            : { path: current, line: position.lineNumber, column: position.column };
        };
        // The selected text, or an empty string when the selection is empty.
        const selectionText = (): string => {
          const selection = editor.getSelection?.() ?? null;
          const model = editor.getModel();
          if (selection === null || model === null || typeof model.getValueInRange !== 'function') {
            return '';
          }
          if (
            selection.startLineNumber === selection.endLineNumber &&
            selection.startColumn === selection.endColumn
          ) {
            return '';
          }
          return model.getValueInRange(selection);
        };
        // The selection as the chat attachment expects it (or null).
        const selectionSnapshot = (): MonacoSelection | null => {
          const current = modelPathRef.current;
          const selection = editor.getSelection?.() ?? null;
          if (current === null || selection === null) {
            return null;
          }
          const text = selectionText();
          if (text.trim().length === 0) {
            return null;
          }
          return {
            path: current,
            startLine: selection.startLineNumber,
            endLine: selection.endLineNumber,
            text,
          };
        };
        // Ctrl/Cmd+S saves through the kernel; Ctrl+F/Ctrl+H are Monaco defaults.
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => onSaveRef.current());
        // LSP shortcuts: F12 definition, F2 rename, Ctrl+. quick fixes.
        editor.addCommand(monaco.KeyCode.F12, () => {
          const at = cursor();
          if (at !== null) {
            lspRef.current?.onDefinition(at.path, at.line, at.column);
          }
        });
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.F12, () => {
          const at = cursor();
          if (at !== null) {
            lspRef.current?.onDefinition(at.path, at.line, at.column);
          }
        });
        editor.addCommand(monaco.KeyCode.F2, () => {
          const at = cursor();
          if (at !== null) {
            lspRef.current?.onRename(at.path, at.line, at.column);
          }
        });
        // Ctrl+. opens quick actions: inline AI first, then the LSP fixes.
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Period, () => {
          const at = cursor();
          if (at === null) {
            return;
          }
          const inlineActions = inlineRef.current;
          if (inlineActions !== null && inlineActions !== undefined) {
            inlineActions.onQuickActions(at.path, at.line, at.column, selectionText());
            return;
          }
          lspRef.current?.onCodeActions(at.path, at.line, at.column);
        });
        subscription = editor.onDidChangeModelContent(() => onChangeRef.current(editor.getValue()));
        selectionSubscription =
          editor.onDidChangeCursorSelection?.(() => {
            onSelectionRef.current?.(selectionSnapshot());
          }) ?? null;
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
      selectionSubscription?.dispose();
      for (const provider of hoverProviders.current) {
        provider.dispose();
      }
      hoverProviders.current = [];
      hoverLanguages.current.clear();
      for (const provider of inlineProviders.current) {
        provider.dispose();
      }
      inlineProviders.current = [];
      inlineLanguages.current.clear();
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
      onSelectionRef.current?.(null);
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

  // One hover provider per language, reading the editor's current path.
  useEffect(() => {
    const monaco = monacoRef.current;
    if (!ready || monaco === null) {
      return;
    }
    const language = monacoLanguage(languageId);
    if (hoverLanguages.current.has(language)) {
      return;
    }
    hoverLanguages.current.add(language);
    const provider: MonacoHoverProvider = {
      async provideHover(model, position): Promise<{ contents: unknown[] } | null> {
        const actions = lspRef.current;
        const target = modelPath(model);
        if (!actions || target === null) {
          return null;
        }
        const markdown = await actions.hover(target, position.lineNumber, position.column);
        return markdown === null ? null : { contents: [{ value: markdown }] };
      },
    };
    hoverProviders.current.push(monaco.languages.registerHoverProvider(language, provider));
  }, [ready, languageId]);

  // One inline completion provider per language (task 3.12). Monaco renders the
  // result as ghost text: Tab accepts it, Esc dismisses it.
  useEffect(() => {
    const monaco = monacoRef.current;
    if (!ready || monaco === null) {
      return;
    }
    const language = monacoLanguage(languageId);
    if (inlineLanguages.current.has(language)) {
      return;
    }
    inlineLanguages.current.add(language);
    const provider: MonacoInlineProvider = {
      async provideInlineCompletions(model, position) {
        const actions = inlineRef.current;
        const target = modelPath(model);
        if (actions === null || actions === undefined || target === null) {
          return null;
        }
        if (typeof model.getOffsetAt !== 'function') {
          return null;
        }
        const suggestion = await actions.complete(
          target,
          model.getValue(),
          model.getOffsetAt(position),
          language,
          readOnlyRef.current === true,
        );
        return suggestion === null ? null : { items: [{ insertText: suggestion }] };
      },
    };
    inlineProviders.current.push(
      monaco.languages.registerInlineCompletionsProvider(language, provider),
    );
  }, [ready, languageId]);

  // Inline squiggles for the active file (the Problems panel owns the truth).
  useEffect(() => {
    const monaco = monacoRef.current;
    const editor = editorRef.current;
    if (!ready || monaco === null || editor === null) {
      return;
    }
    const model = editor.getModel();
    if (model === null) {
      return;
    }
    monaco.editor.setModelMarkers?.(model, 'fergents-lsp', (markers ?? []).map(toMonacoMarker));
  }, [ready, path, markers]);

  useEffect(() => {
    const editor = editorRef.current;
    if (!ready || editor === null || reveal === undefined || reveal === null) {
      return;
    }
    editor.revealLineInCenter(reveal.line);
    editor.setPosition({ lineNumber: reveal.line, column: reveal.column ?? 1 });
    editor.focus();
  }, [ready, reveal]);

  return (
    <>
      {failed ? <div className="inline-feedback error">{t('ide.editor.loadError')}</div> : null}
      <div className="ide-editor__monaco" data-testid="ide-monaco" ref={containerRef} />
    </>
  );
}
