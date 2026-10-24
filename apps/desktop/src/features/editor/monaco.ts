/**
 * Lazy Monaco loader (spec §2): Monaco and its workers live in their own
 * chunk so chat/brain startup never pays for them.
 *
 * Only the standalone editor API plus the Monarch grammars the kernel maps to
 * are imported — not the full `monaco-editor` entry point, which also pulls in
 * every language service. Language intelligence comes from the kernel LSP
 * client (task 3.8), not from Monaco's own services.
 *
 * `MonacoEnvironment` must be installed before the editor module is evaluated,
 * which is why the workers are imported here rather than at module scope.
 */
import type { MonacoLike } from './monacoAdapter';

let pending: Promise<MonacoLike> | null = null;

/** Install the worker factory used by Monaco (JSON validation only). */
async function installWorkers(): Promise<void> {
  const [{ default: EditorWorker }, { default: JsonWorker }] = await Promise.all([
    import('monaco-editor/editor/editor.worker.js?worker'),
    import('monaco-editor/language/json/json.worker.js?worker'),
  ]);

  (globalThis as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
    getWorker(_workerId: string, label: string): Worker {
      switch (label) {
        case 'json':
          return new JsonWorker();
        default:
          return new EditorWorker();
      }
    },
  };
}

/** Register the Monarch grammars for the languages the kernel reports. */
async function registerLanguages(): Promise<void> {
  await Promise.all([
    import('monaco-editor/languages/definitions/rust/register.js'),
    import('monaco-editor/languages/definitions/typescript/register.js'),
    import('monaco-editor/languages/definitions/javascript/register.js'),
    import('monaco-editor/languages/definitions/markdown/register.js'),
    import('monaco-editor/languages/definitions/css/register.js'),
    import('monaco-editor/languages/definitions/scss/register.js'),
    import('monaco-editor/languages/definitions/less/register.js'),
    import('monaco-editor/languages/definitions/html/register.js'),
    import('monaco-editor/languages/definitions/python/register.js'),
    import('monaco-editor/languages/definitions/yaml/register.js'),
    import('monaco-editor/languages/definitions/shell/register.js'),
    import('monaco-editor/languages/definitions/sql/register.js'),
    import('monaco-editor/languages/definitions/go/register.js'),
    import('monaco-editor/languages/definitions/java/register.js'),
    import('monaco-editor/languages/definitions/csharp/register.js'),
    import('monaco-editor/languages/definitions/cpp/register.js'),
    import('monaco-editor/languages/definitions/php/register.js'),
    import('monaco-editor/languages/definitions/ruby/register.js'),
    import('monaco-editor/languages/definitions/xml/register.js'),
    import('monaco-editor/languages/definitions/powershell/register.js'),
    import('monaco-editor/languages/definitions/lua/register.js'),
    import('monaco-editor/languages/definitions/kotlin/register.js'),
    import('monaco-editor/languages/definitions/swift/register.js'),
    import('monaco-editor/languages/definitions/r/register.js'),
    import('monaco-editor/languages/definitions/dockerfile/register.js'),
    import('monaco-editor/languages/definitions/ini/register.js'),
    // JSON is a language service in Monaco, not a Monarch definition.
    import('monaco-editor/languages/features/json/register.js'),
  ]);
}

/** Load (once) and return the Monaco namespace. */
export function loadMonaco(): Promise<MonacoLike> {
  pending ??= (async () => {
    await installWorkers();
    const monaco = await import('monaco-editor/editor/editor.api.js');
    await registerLanguages();
    return monaco as unknown as MonacoLike;
  })();
  return pending;
}
