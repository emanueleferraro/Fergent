import { describe, expect, it } from 'vitest';

import { Placeholder, version } from './index.js';

describe('@fergents/ui scaffold', () => {
  it('exposes a version string', () => {
    expect(version).toBeTypeOf('string');
  });

  it('placeholder renders nothing', () => {
    expect(Placeholder()).toBeNull();
  });
});
