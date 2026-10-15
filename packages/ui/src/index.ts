/**
 * @fergents/ui — Phase 0 scaffold.
 *
 * This package currently exposes only a version constant and a placeholder
 * component so the workspace toolchain (`build`, `typecheck`, `lint`, `test`)
 * has a React-typed public surface to operate on. Real UI components land in
 * later phases.
 */

import type { ReactElement } from 'react';

/** Package version placeholder for the Phase 0 scaffold. */
export const version = '0.1.0';

/** Placeholder component; renders nothing until real UI is added. */
export function Placeholder(): ReactElement | null {
  return null;
}
