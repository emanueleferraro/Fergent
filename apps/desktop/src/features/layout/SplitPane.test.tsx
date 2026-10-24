import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { MAX_RATIO, MIN_RATIO } from './layout';
import { SplitPane } from './SplitPane';

afterEach(() => {
  cleanup();
});

function setup(ratio = 0.5, onReset = vi.fn()) {
  const onRatioChange = vi.fn();
  render(
    <SplitPane
      direction="row"
      ratio={ratio}
      label="Resize panes"
      first={<div>chat</div>}
      second={<div>ide</div>}
      onRatioChange={onRatioChange}
      onReset={onReset}
    />,
  );
  return {
    onRatioChange,
    onReset,
    divider: screen.getByRole('separator', { name: 'Resize panes' }),
  };
}

describe('SplitPane', () => {
  it('renders both panes and exposes the divider state', () => {
    const { divider } = setup();
    expect(screen.getByText('chat')).toBeDefined();
    expect(screen.getByText('ide')).toBeDefined();
    expect(divider.getAttribute('aria-orientation')).toBe('vertical');
    expect(divider.getAttribute('aria-valuenow')).toBe('50');
    expect(divider.getAttribute('aria-valuemin')).toBe(String(Math.round(MIN_RATIO * 100)));
    expect(divider.getAttribute('aria-valuemax')).toBe(String(Math.round(MAX_RATIO * 100)));
  });

  it('moves the divider with the arrow keys', () => {
    const { divider, onRatioChange } = setup();
    fireEvent.keyDown(divider, { key: 'ArrowRight' });
    expect(onRatioChange).toHaveBeenLastCalledWith(0.55);
    fireEvent.keyDown(divider, { key: 'ArrowLeft' });
    expect(onRatioChange).toHaveBeenLastCalledWith(0.45);
  });

  it('jumps to the bounds with Home and End', () => {
    const { divider, onRatioChange } = setup();
    fireEvent.keyDown(divider, { key: 'Home' });
    expect(onRatioChange).toHaveBeenLastCalledWith(MIN_RATIO);
    fireEvent.keyDown(divider, { key: 'End' });
    expect(onRatioChange).toHaveBeenLastCalledWith(MAX_RATIO);
  });

  it('ignores keys that do not resize', () => {
    const { divider, onRatioChange } = setup();
    fireEvent.keyDown(divider, { key: 'Enter' });
    expect(onRatioChange).not.toHaveBeenCalled();
  });

  it('resets on double click', () => {
    const { divider, onReset } = setup();
    fireEvent.doubleClick(divider);
    expect(onReset).toHaveBeenCalledTimes(1);
  });
});
