/**
 * CodeMirror 6 markdown editor (decision D1). Presents a controlled `value`
 * but keeps the underlying `EditorView` alive across renders; external value
 * changes are dispatched as transactions so the cursor is preserved.
 */
import { markdown } from '@codemirror/lang-markdown';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { basicSetup } from 'codemirror';
import { useEffect, useRef, type ReactElement } from 'react';

/** Props for `CodeEditor`. */
export interface CodeEditorProps {
  /** The document source. */
  value: string;
  /** Called with the full source after every edit. */
  onChange?: ((value: string) => void) | undefined;
  /** Render read-only. */
  readOnly?: boolean | undefined;
  /** Accessible label for the editor region. */
  ariaLabel?: string | undefined;
}

/** The markdown source editor. */
export function CodeEditor({
  value,
  onChange,
  readOnly = false,
  ariaLabel,
}: CodeEditorProps): ReactElement {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const initialValue = useRef(value);

  useEffect(() => {
    const parent = host.current;
    if (!parent) {
      return;
    }
    const state = EditorState.create({
      doc: initialValue.current,
      extensions: [
        basicSetup,
        markdown(),
        EditorView.lineWrapping,
        EditorState.readOnly.of(readOnly),
        EditorView.contentAttributes.of(ariaLabel ? { 'aria-label': ariaLabel } : {}),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current?.(update.state.doc.toString());
          }
        }),
      ],
    });
    const instance = new EditorView({ state, parent });
    view.current = instance;
    return () => {
      instance.destroy();
      view.current = null;
    };
  }, [readOnly, ariaLabel]);

  useEffect(() => {
    const instance = view.current;
    if (!instance) {
      return;
    }
    const current = instance.state.doc.toString();
    if (current !== value) {
      instance.dispatch({ changes: { from: 0, to: current.length, insert: value } });
    }
  }, [value]);

  return <div className="editor-source" ref={host} />;
}
