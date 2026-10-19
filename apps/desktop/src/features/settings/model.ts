/**
 * Per-brain settings model (Task 21, BRAIN.md §6.1).
 *
 * Only the brain identity (name/color/icon) is mutable through `brain.update`
 * in Phase 1; the remaining `brain.toml` sections are read-only until the
 * kernel exposes a write path for them. These helpers are pure so the load /
 * save / validation logic is unit-testable.
 */
import type { BrainConfig, BrainUpdateParams } from '@fergents/protocol';

/** The editable identity fields of a brain. */
export interface IdentityDraft {
  name: string;
  color: string;
  icon: string;
}

/** Validation errors for {@link IdentityDraft} (`null` means valid). */
export interface DraftErrors {
  name: 'required' | null;
  color: 'invalid' | null;
}

const HEX_COLOR = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;

/** Build the editable draft from a loaded configuration. */
export function draftFrom(config: BrainConfig | null): IdentityDraft {
  return {
    name: config?.brain?.name?.trim() ?? '',
    color: config?.brain?.color?.trim() ?? '',
    icon: config?.brain?.icon?.trim() ?? '',
  };
}

/** Validate a draft: a name is required, the color must be a hex value. */
export function validateDraft(draft: IdentityDraft): DraftErrors {
  const color = draft.color.trim();
  return {
    name: draft.name.trim().length === 0 ? 'required' : null,
    color: color.length > 0 && !HEX_COLOR.test(color) ? 'invalid' : null,
  };
}

/** Whether a draft has any validation error. */
export function hasErrors(errors: DraftErrors): boolean {
  return errors.name !== null || errors.color !== null;
}

/**
 * Build `brain.update` params from a draft, or `null` when nothing changed.
 *
 * Only the fields that differ from the loaded values are sent (unchanged value
 * handling); clearing the color or icon sends `null` to unset them.
 */
export function toUpdateParams(
  brainId: string,
  draft: IdentityDraft,
  original: IdentityDraft,
): BrainUpdateParams | null {
  const params: BrainUpdateParams = { brain_id: brainId };
  let changed = false;

  const name = draft.name.trim();
  if (name !== original.name.trim()) {
    params.name = name;
    changed = true;
  }

  const color = draft.color.trim();
  if (color !== original.color.trim()) {
    params.color = color.length > 0 ? color : null;
    changed = true;
  }

  const icon = draft.icon.trim();
  if (icon !== original.icon.trim()) {
    params.icon = icon.length > 0 ? icon : null;
    changed = true;
  }

  return changed ? params : null;
}

/** Labels used when formatting a read-only configuration value. */
export interface ValueLabels {
  on: string;
  off: string;
  unset: string;
}

/** Format a `brain.toml` value for display in a read-only settings row. */
export function formatValue(value: unknown, labels: ValueLabels): string {
  if (value === null || value === undefined || value === '') {
    return labels.unset;
  }
  if (typeof value === 'boolean') {
    return value ? labels.on : labels.off;
  }
  if (typeof value === 'number') {
    return String(value);
  }
  if (Array.isArray(value)) {
    return value.length > 0 ? value.map(String).join(', ') : labels.unset;
  }
  return String(value);
}
