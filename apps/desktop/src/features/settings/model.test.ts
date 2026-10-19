import { describe, expect, it } from 'vitest';
import type { BrainConfig } from '@fergents/protocol';

import {
  draftFrom,
  formatValue,
  hasErrors,
  toUpdateParams,
  validateDraft,
  type IdentityDraft,
} from './model';

const LABELS = { on: 'On', off: 'Off', unset: '—' };

const DRAFT: IdentityDraft = { name: 'Personal', color: '#D97757', icon: '🧠' };

describe('draftFrom', () => {
  it('reads the identity from a configuration', () => {
    const config: BrainConfig = {
      brain: { id: 'b1', name: 'Personal', color: '#D97757', icon: '🧠' },
    };
    expect(draftFrom(config)).toEqual(DRAFT);
  });

  it('degrades to empty values without a configuration', () => {
    expect(draftFrom(null)).toEqual({ name: '', color: '', icon: '' });
    expect(draftFrom({})).toEqual({ name: '', color: '', icon: '' });
  });
});

describe('validateDraft', () => {
  it('requires a name', () => {
    expect(validateDraft({ ...DRAFT, name: '   ' })).toEqual({ name: 'required', color: null });
    expect(hasErrors(validateDraft({ ...DRAFT, name: '' }))).toBe(true);
  });

  it('accepts a hex color and an empty color', () => {
    expect(validateDraft(DRAFT)).toEqual({ name: null, color: null });
    expect(validateDraft({ ...DRAFT, color: '' })).toEqual({ name: null, color: null });
    expect(validateDraft({ ...DRAFT, color: '#abc' })).toEqual({ name: null, color: null });
  });

  it('rejects a non-hex color', () => {
    expect(validateDraft({ ...DRAFT, color: 'orange' }).color).toBe('invalid');
  });
});

describe('toUpdateParams', () => {
  it('returns null when nothing changed, ignoring surrounding whitespace', () => {
    expect(toUpdateParams('b1', DRAFT, DRAFT)).toBeNull();
    expect(toUpdateParams('b1', { ...DRAFT, name: ' Personal ' }, DRAFT)).toBeNull();
  });

  it('sends only the changed fields', () => {
    expect(toUpdateParams('b1', { ...DRAFT, name: 'Work' }, DRAFT)).toEqual({
      brain_id: 'b1',
      name: 'Work',
    });
    expect(toUpdateParams('b1', { ...DRAFT, icon: '📓' }, DRAFT)).toEqual({
      brain_id: 'b1',
      icon: '📓',
    });
  });

  it('unsets the color when it is cleared', () => {
    expect(toUpdateParams('b1', { ...DRAFT, color: '' }, DRAFT)).toEqual({
      brain_id: 'b1',
      color: null,
    });
  });
});

describe('formatValue', () => {
  it('formats booleans, scalars, arrays, and unset values', () => {
    expect(formatValue(true, LABELS)).toBe('On');
    expect(formatValue(false, LABELS)).toBe('Off');
    expect(formatValue(512, LABELS)).toBe('512');
    expect(formatValue('hybrid', LABELS)).toBe('hybrid');
    expect(formatValue(['a', 'b'], LABELS)).toBe('a, b');
    expect(formatValue(null, LABELS)).toBe('—');
    expect(formatValue(undefined, LABELS)).toBe('—');
    expect(formatValue('', LABELS)).toBe('—');
    expect(formatValue([], LABELS)).toBe('—');
  });
});
