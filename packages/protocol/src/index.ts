/**
 * Fergents kernel protocol (TypeScript).
 *
 * The JSON Schema in `protocol/schema/` is the source of truth. The types in
 * `./generated.ts` are generated from it; `./envelope.ts` holds the generic,
 * hand-written JSON-RPC 2.0 envelope types.
 */

export * from './envelope.js';
export * from './generated.js';
