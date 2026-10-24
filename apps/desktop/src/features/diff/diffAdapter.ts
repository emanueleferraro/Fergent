/**
 * The Monaco diff adapter (spec D15): a narrow surface over
 * `monaco.editor.createDiffEditor` so the preview can be tested with a fake
 * Monaco in jsdom. Only `DiffEditorHost` talks to the real implementation.
 */
import type { MonacoLike, MonacoModel } from '../editor/monacoAdapter';

/** The subset of a Monaco diff editor the preview uses. */
export interface MonacoDiffEditor {
  setModel(model: { original: MonacoModel; modified: MonacoModel }): void;
  updateOptions(options: Record<string, unknown>): void;
  dispose(): void;
}

/** The slice of the `monaco` namespace used by the diff preview. */
export interface MonacoDiffLike extends MonacoLike {
  editor: MonacoLike['editor'] & {
    createDiffEditor(element: HTMLElement, options: Record<string, unknown>): MonacoDiffEditor;
  };
}

/** Options shared by inline and side-by-side diff modes. */
export function diffOptions(sideBySide: boolean): Record<string, unknown> {
  return {
    readOnly: true,
    originalEditable: false,
    renderSideBySide: sideBySide,
    automaticLayout: true,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    renderOverviewRuler: false,
    ignoreTrimWhitespace: false,
    renderIndicators: true,
  };
}

/** A one-to-one model URI per side of a diff. */
export function diffModelUri(
  monaco: MonacoLike,
  path: string,
  side: 'original' | 'proposed',
): unknown {
  return monaco.Uri.parse(`inmemory://fergents-diff/${side}/${encodeURIComponent(path)}`);
}
