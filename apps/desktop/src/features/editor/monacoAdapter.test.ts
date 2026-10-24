import { describe, expect, it } from 'vitest';

import {
  disposeAllModels,
  disposeModelsOutside,
  editorOptions,
  ensureModel,
  modelUri,
  monacoLanguage,
  syncModelValue,
  themeName,
  type MonacoLike,
  type MonacoModel,
} from './monacoAdapter';

class FakeModel implements MonacoModel {
  value: string;
  disposed = false;
  writes = 0;
  constructor(value: string) {
    this.value = value;
  }
  getValue(): string {
    return this.value;
  }
  setValue(value: string): void {
    this.value = value;
    this.writes += 1;
  }
  dispose(): void {
    this.disposed = true;
  }
}

function fakeMonaco(): {
  monaco: MonacoLike;
  models: Map<string, FakeModel>;
  languages: string[];
} {
  const models = new Map<string, FakeModel>();
  const languages: string[] = [];
  const monaco: MonacoLike = {
    Uri: { parse: (value: string) => value },
    KeyMod: { CtrlCmd: 2048 },
    KeyCode: { KeyS: 49, F2: 60, F12: 70, Period: 80 },
    editor: {
      create: () => {
        throw new Error('not used by the adapter');
      },
      createModel: (value, language, uri) => {
        languages.push(language ?? '');
        const model = new FakeModel(value);
        models.set(String(uri), model);
        return model;
      },
      getModel: (uri) => models.get(String(uri)) ?? null,
    },
    languages: {
      registerHoverProvider: () => ({ dispose: () => undefined }),
    },
  };
  return { monaco, models, languages };
}

describe('monacoLanguage', () => {
  it('passes known kernel ids through and falls back to plain text', () => {
    expect(monacoLanguage('rust')).toBe('rust');
    expect(monacoLanguage('typescript')).toBe('typescript');
    expect(monacoLanguage('markdown')).toBe('markdown');
    expect(monacoLanguage('toml')).toBe('plaintext');
  });
});

describe('modelUri', () => {
  it('is stable and unique per path', () => {
    const { monaco } = fakeMonaco();
    expect(modelUri(monaco, 'src/a b.ts')).toBe(modelUri(monaco, 'src/a b.ts'));
    expect(modelUri(monaco, 'src/a.ts')).not.toBe(modelUri(monaco, 'src/b.ts'));
    expect(String(modelUri(monaco, 'src/a b.ts'))).toContain('src%2Fa%20b.ts');
  });
});

describe('ensureModel', () => {
  it('creates a model once and reuses it without touching its buffer', () => {
    const { monaco } = fakeMonaco();
    const models = new Map<string, MonacoModel>();

    const first = ensureModel(monaco, models, 'src/main.ts', 'const a = 1;', 'typescript');
    first.setValue('edited');
    const second = ensureModel(monaco, models, 'src/main.ts', 'stale prop', 'typescript');

    expect(second).toBe(first);
    expect(second.getValue()).toBe('edited');
    expect(models.size).toBe(1);
  });
});

describe('syncModelValue', () => {
  it('writes only when the value diverged', () => {
    const model = new FakeModel('a');
    syncModelValue(model, 'a');
    expect(model.writes).toBe(0);
    syncModelValue(model, 'b');
    expect(model.writes).toBe(1);
    expect(model.getValue()).toBe('b');
  });
});

describe('model disposal', () => {
  it('disposes only the models of closed tabs', () => {
    const models = new Map<string, MonacoModel>([
      ['a.ts', new FakeModel('a')],
      ['b.ts', new FakeModel('b')],
    ]);
    disposeModelsOutside(models, new Set(['b.ts']));
    expect(models.has('a.ts')).toBe(false);
    expect(models.has('b.ts')).toBe(true);
  });

  it('disposes everything on unmount', () => {
    const first = new FakeModel('a');
    const models = new Map<string, MonacoModel>([['a.ts', first]]);
    disposeAllModels(models);
    expect(first.disposed).toBe(true);
    expect(models.size).toBe(0);
  });
});

describe('editor options', () => {
  it('reflects read-only documents and the theme', () => {
    expect(editorOptions(true).readOnly).toBe(true);
    expect(editorOptions(false).readOnly).toBe(false);
    expect(themeName('dark')).toBe('vs-dark');
    expect(themeName('light')).toBe('vs');
  });
});
