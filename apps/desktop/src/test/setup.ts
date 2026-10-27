/**
 * Vitest setup: jsdom does not implement a few browser APIs that React Flow
 * (the graph view) relies on. Polyfill the minimum needed so the components
 * mount under test; nothing here changes application behavior.
 */

/** A no-op ResizeObserver: React Flow only needs the callbacks to exist. */
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

const globals = globalThis as {
  IS_REACT_ACT_ENVIRONMENT?: boolean;
  ResizeObserver?: unknown;
  matchMedia?: unknown;
};

// Plugin surfaces mount into their own React root (the shadow root), outside
// Testing Library's tree; this flag lets `act` wrap those renders too.
globals.IS_REACT_ACT_ENVIRONMENT = true;

if (!globals.ResizeObserver) {
  globals.ResizeObserver = ResizeObserverStub;
}

if (!globals.matchMedia) {
  globals.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    dispatchEvent: () => false,
  });
}
