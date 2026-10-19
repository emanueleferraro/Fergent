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
  ResizeObserver?: unknown;
  matchMedia?: unknown;
};

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
