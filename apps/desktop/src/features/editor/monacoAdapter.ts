/**
 * The Monaco adapter (spec D16): the editor is wrapped behind this small
 * interface so the controllers can be tested with a fake Monaco in jsdom.
 * `MonacoHost` is the only module that talks to the real thing.
 */

/** The subset of a Monaco text model the adapter needs. */
export interface MonacoModel {
  uri?: unknown;
  getValue(): string;
  setValue(value: string): void;
  dispose(): void;
  getOffsetAt?(position: { lineNumber: number; column: number }): number;
  getValueInRange?(range: unknown): string;
}

/** A Monaco selection range (one-based, as Monaco reports it). */
export interface MonacoRange {
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
  isEmpty?(): boolean;
}

/** The subset of a Monaco editor instance the adapter uses. */
export interface MonacoEditor {
  setModel(model: MonacoModel | null): void;
  getModel(): MonacoModel | null;
  updateOptions(options: Record<string, unknown>): void;
  addCommand(keybinding: number, handler: () => void): string | null;
  onDidChangeModelContent(listener: () => void): { dispose(): void };
  getValue(): string;
  getPosition(): { lineNumber: number; column: number } | null;
  getSelection?(): MonacoRange | null;
  focus(): void;
  revealLineInCenter(line: number): void;
  setPosition(position: { lineNumber: number; column: number }): void;
  dispose(): void;
}

/** A Monaco hover provider, as registered per language. */
export interface MonacoHoverProvider {
  provideHover(
    model: MonacoModel,
    position: { lineNumber: number; column: number },
  ): Promise<{ contents: unknown[] } | null>;
}

/** One ghost-text suggestion returned by an inline completion provider. */
export interface MonacoInlineItem {
  insertText: string;
}

/** The inline completion list; `dispose` is Monaco's optional cleanup hook. */
export interface MonacoInlineCompletions {
  items: MonacoInlineItem[];
  dispose?(): void;
}

/** A Monaco inline completion provider (ghost text; Tab accepts, Esc hides). */
export interface MonacoInlineProvider {
  provideInlineCompletions(
    model: MonacoModel,
    position: { lineNumber: number; column: number },
    context: unknown,
    token: unknown,
  ): Promise<MonacoInlineCompletions | null>;
}

/** The slice of the `monaco` namespace used by the IDE. */
export interface MonacoLike {
  Uri: { parse(value: string): unknown };
  KeyMod: { CtrlCmd: number };
  KeyCode: { KeyS: number; F2: number; F12: number; Period: number };
  editor: {
    create(element: HTMLElement, options: Record<string, unknown>): MonacoEditor;
    createModel(value: string, language: string | undefined, uri: unknown): MonacoModel;
    getModel(uri: unknown): MonacoModel | null;
    setTheme?(theme: string): void;
    setModelMarkers?(model: MonacoModel, owner: string, markers: readonly unknown[]): void;
  };
  languages: {
    registerHoverProvider(languageId: string, provider: MonacoHoverProvider): { dispose(): void };
    registerInlineCompletionsProvider(
      languageId: string,
      provider: MonacoInlineProvider,
    ): { dispose(): void };
  };
}

/** Language ids Monaco ships out of the box (the kernel reports the same ids). */
const MONACO_LANGUAGES = new Set([
  'bat',
  'c',
  'clojure',
  'coffee',
  'cpp',
  'csharp',
  'css',
  'dockerfile',
  'elixir',
  'fsharp',
  'go',
  'graphql',
  'html',
  'ini',
  'java',
  'javascript',
  'json',
  'julia',
  'kotlin',
  'less',
  'lua',
  'markdown',
  'mysql',
  'objective-c',
  'perl',
  'php',
  'powershell',
  'python',
  'r',
  'ruby',
  'rust',
  'scala',
  'scss',
  'shell',
  'sql',
  'swift',
  'typescript',
  'xml',
  'yaml',
]);

/** Map a kernel language id to one Monaco knows, falling back to plain text. */
export function monacoLanguage(languageId: string): string {
  return MONACO_LANGUAGES.has(languageId) ? languageId : 'plaintext';
}

/** Editor options for a document; read-only documents offer no cursor edits. */
export function editorOptions(readOnly: boolean): Record<string, unknown> {
  return {
    readOnly,
    automaticLayout: true,
    minimap: { enabled: true },
    scrollBeyondLastLine: false,
    renderWhitespace: 'selection',
    tabSize: 2,
    insertSpaces: true,
    fixedOverflowWidgets: true,
    wordWrap: 'off',
    // Ghost text for inline AI (task 3.12); Tab accepts, Esc dismisses.
    inlineSuggest: { enabled: true },
    // Find/replace (Ctrl+F / Ctrl+H) and multi-cursor are Monaco defaults.
  };
}

/** The theme name for a Fergents theme. */
export function themeName(theme: 'dark' | 'light'): string {
  return theme === 'dark' ? 'vs-dark' : 'vs';
}

/** A stable, one-to-one model URI for a project-relative path. */
export function modelUri(monaco: MonacoLike, path: string): unknown {
  return monaco.Uri.parse(`inmemory://fergents/${encodeURIComponent(path)}`);
}

/** Decode the path back out of a `modelUri` (`inmemory://fergents/<encoded>`). */
export function pathFromModelUri(uri: string): string | null {
  const prefix = 'inmemory://fergents/';
  if (!uri.startsWith(prefix)) {
    return null;
  }
  try {
    return decodeURIComponent(uri.slice(prefix.length));
  } catch {
    return null;
  }
}

/** The project-relative path of a model, or `null` when it is not ours. */
export function modelPath(model: MonacoModel): string | null {
  const uri = model.uri;
  const text = typeof uri === 'string' ? uri : uri === null || uri === undefined ? '' : String(uri);
  return text.length === 0 ? null : pathFromModelUri(text);
}

/**
 * Get the model for `path`, creating it from `content` on first open.
 *
 * Models are reused across tab switches (spec §2: one model per file URI);
 * the buffer value is never overwritten here, so unsaved edits survive.
 */
export function ensureModel(
  monaco: MonacoLike,
  models: Map<string, MonacoModel>,
  path: string,
  content: string,
  languageId: string,
): MonacoModel {
  const uri = modelUri(monaco, path);
  const existing = monaco.editor.getModel(uri) ?? models.get(path) ?? null;
  if (existing !== null) {
    models.set(path, existing);
    return existing;
  }
  const model = monaco.editor.createModel(content, monacoLanguage(languageId), uri);
  models.set(path, model);
  return model;
}

/**
 * Replace the model text when it diverged from the buffer (external reload).
 * When the values match — the usual case while typing — nothing happens.
 */
export function syncModelValue(model: MonacoModel, content: string): void {
  if (model.getValue() !== content) {
    model.setValue(content);
  }
}

/** Dispose the models of tabs that are no longer open. */
export function disposeModelsOutside(
  models: Map<string, MonacoModel>,
  keep: ReadonlySet<string>,
): void {
  for (const [path, model] of models) {
    if (!keep.has(path)) {
      model.dispose();
      models.delete(path);
    }
  }
}

/** Dispose every cached model (the IDE view unmounted). */
export function disposeAllModels(models: Map<string, MonacoModel>): void {
  for (const model of models.values()) {
    model.dispose();
  }
  models.clear();
}
