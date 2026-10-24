/**
 * A resizable, keyboard-accessible two-pane split (UI.md §6.3, §10).
 *
 * The divider is a `role="separator"` control: arrow keys move it (5% per
 * press), Home/End jump to the bounds, and a double click restores the
 * default ratio. Dragging with a pointer updates the ratio continuously.
 */
import {
  useRef,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactElement,
  type ReactNode,
} from 'react';

import { t } from '../../i18n/i18n';

import { clampRatio, MAX_RATIO, MIN_RATIO, resizeRatio } from './layout';

/** Props for `SplitPane`. */
export interface SplitPaneProps {
  /** `row` places the panes side by side; `column` stacks them. */
  direction: 'row' | 'column';
  /** Share of the space taken by `first`, between `MIN_RATIO` and `MAX_RATIO`. */
  ratio: number;
  /** Accessible name of the divider. */
  label: string;
  first: ReactNode;
  second: ReactNode;
  onRatioChange: (ratio: number) => void;
  /** Restore the default split (divider double click). */
  onReset?: (() => void) | undefined;
}

/** Two panes separated by a keyboard-accessible divider. */
export function SplitPane({
  direction,
  ratio,
  label,
  first,
  second,
  onRatioChange,
  onReset,
}: SplitPaneProps): ReactElement {
  const container = useRef<HTMLDivElement | null>(null);

  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>): void => {
    const next = resizeRatio(ratio, event.key);
    if (next === ratio) {
      return;
    }
    event.preventDefault();
    onRatioChange(next);
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>): void => {
    if (event.button !== 0) {
      return;
    }
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>): void => {
    if (event.currentTarget.hasPointerCapture?.(event.pointerId) !== true) {
      return;
    }
    const rect = container.current?.getBoundingClientRect();
    if (!rect || rect.width === 0 || rect.height === 0) {
      return;
    }
    const raw =
      direction === 'row'
        ? (event.clientX - rect.left) / rect.width
        : (event.clientY - rect.top) / rect.height;
    onRatioChange(clampRatio(raw));
  };

  const onPointerUp = (event: ReactPointerEvent<HTMLDivElement>): void => {
    event.currentTarget.releasePointerCapture?.(event.pointerId);
  };

  return (
    <div ref={container} className={`split-pane split-pane--${direction}`}>
      <div className="split-pane__pane" style={{ flexBasis: `${ratio * 100}%` }}>
        {first}
      </div>
      <div
        className="split-pane__divider"
        role="separator"
        tabIndex={0}
        aria-label={label}
        aria-orientation={direction === 'row' ? 'vertical' : 'horizontal'}
        aria-valuenow={Math.round(ratio * 100)}
        aria-valuemin={Math.round(MIN_RATIO * 100)}
        aria-valuemax={Math.round(MAX_RATIO * 100)}
        title={t('ide.split.reset')}
        onKeyDown={onKeyDown}
        onDoubleClick={onReset}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      />
      <div className="split-pane__pane split-pane__pane--flex">{second}</div>
    </div>
  );
}
