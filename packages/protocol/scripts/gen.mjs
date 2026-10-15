// Protocol codegen: JSON Schema -> TypeScript types.
//
// The JSON Schema under `protocol/schema/` at the repository root is the source
// of truth. This script regenerates `src/generated.ts` from it. A freshness
// test (`src/codegen.test.ts`) fails when the committed output drifts.
//
// Run with: `pnpm --filter @fergents/protocol gen`

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { compile } from 'json-schema-to-typescript';

const scriptDir = dirname(fileURLToPath(import.meta.url));

/** Repository root, resolved relative to this script (`packages/protocol/scripts`). */
const repoRoot = resolve(scriptDir, '../../..');

const HEADER = [
  '// @generated — do not edit',
  '// Generated from protocol/schema/protocol.schema.json by `pnpm --filter @fergents/protocol gen`.',
  '',
].join('\n');

/** Absolute path of the source-of-truth JSON Schema. */
export function schemaFilePath() {
  return resolve(repoRoot, 'protocol/schema/protocol.schema.json');
}

/** Absolute path of the committed generated TypeScript file. */
export function generatedFilePath() {
  return resolve(scriptDir, '../src/generated.ts');
}

/**
 * Rewrite every `#/$defs/...` reference to `#/definitions/...` in place.
 * @param {unknown} node
 */
function rewriteRefs(node) {
  if (Array.isArray(node)) {
    for (const item of node) rewriteRefs(item);
    return;
  }
  if (node !== null && typeof node === 'object') {
    for (const [key, value] of Object.entries(node)) {
      if (key === '$ref' && typeof value === 'string') {
        node[key] = value.replace('#/$defs/', '#/definitions/');
      } else {
        rewriteRefs(value);
      }
    }
  }
}

/**
 * The schema is a bag of `$defs` with no root type. json-schema-to-typescript
 * only emits named declarations reachable from the schema root, so each
 * definition is compiled on its own, with the whole `$defs` bag attached as
 * draft-07 `definitions` (so internal `$ref`s resolve). Cross-references are
 * inlined (`declareExternallyReferenced: false`) to avoid duplicate
 * declarations when the per-definition outputs are concatenated.
 * @param {object} schema
 * @returns {Promise<string>} the full file contents, including the header.
 */
export async function generateProtocolTypes() {
  const schema = JSON.parse(readFileSync(schemaFilePath(), 'utf8'));
  const defs = schema.$defs ?? {};

  const parts = [];
  for (const [name, definition] of Object.entries(defs)) {
    const root = { ...definition, definitions: defs };
    rewriteRefs(root);
    const ts = await compile(root, name, {
      bannerComment: '',
      additionalProperties: false,
      declareExternallyReferenced: false,
      style: {
        semi: true,
        singleQuote: true,
        printWidth: 100,
      },
    });
    parts.push(ts.trim());
  }

  return `${HEADER}\n${parts.join('\n\n')}\n`;
}

async function main() {
  const contents = await generateProtocolTypes();
  const target = generatedFilePath();
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, contents, 'utf8');
  console.log(`wrote ${target}`);
}

const invokedDirectly =
  typeof process.argv[1] === 'string' &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
