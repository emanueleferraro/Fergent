import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { generateProtocolTypes, generatedFilePath } from '../scripts/gen.mjs';

describe('protocol codegen', () => {
  it('committed src/generated.ts is fresh', async () => {
    const expected = (await generateProtocolTypes()).trim();
    const committed = readFileSync(generatedFilePath(), 'utf8').trim();

    expect(committed).toBe(expected);
  });
});
