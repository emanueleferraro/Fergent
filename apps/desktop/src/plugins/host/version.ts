/**
 * Slot API versioning (`UI.md` §22: "slots are versioned").
 *
 * A plugin declares the API version it targets in its manifest. The shell
 * renders contributions for its own version and the previous one (N-1), the
 * same rule the kernel applies to packages (`fergents_packages::api_compatible`).
 */

/** The slot API version implemented by this shell. */
export const HOST_API_VERSION = 1;

/** Whether a contribution targeting `required` can render on `current`. */
export function isApiCompatible(required: number, current: number = HOST_API_VERSION): boolean {
  return required === current || (current > 0 && required + 1 === current);
}
