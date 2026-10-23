/**
 * The Agent Builder model (Task 2.16): a draft, its validation, and the
 * `agents/<id>.md` markdown it serializes to.
 */
import type { AgentDefinition } from '@fergents/protocol';

/** An editable agent definition. */
export interface AgentDraft {
  id: string;
  name: string;
  description: string;
  model: string;
  temperature: string;
  icon: string;
  color: string;
  tools: string;
  skills: string;
  mcp: string;
  prompt: string;
}

/** Per-field validation failures. */
export type DraftErrors = Partial<Record<keyof AgentDraft, string>>;

/** A blank draft for a new agent. */
export function emptyDraft(): AgentDraft {
  return {
    id: '',
    name: '',
    description: '',
    model: 'mock-1',
    temperature: '',
    icon: '🤖',
    color: '#6B94D9',
    tools: '',
    skills: '',
    mcp: '',
    prompt: 'You are a helpful assistant. Be concise.',
  };
}

/** Project a registered definition onto a draft. */
export function draftFrom(definition: AgentDefinition): AgentDraft {
  return {
    id: definition.id,
    name: definition.name,
    description: definition.description ?? '',
    model: definition.model,
    temperature:
      definition.temperature === null || definition.temperature === undefined
        ? ''
        : String(definition.temperature),
    icon: definition.icon ?? '',
    color: definition.color ?? '',
    tools: (definition.tools ?? []).join(', '),
    skills: (definition.skills ?? []).join(', '),
    mcp: (definition.mcp ?? []).join(', '),
    prompt: '',
  };
}

const ID_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

/** Validate a draft; empty means valid. */
export function validateDraft(draft: AgentDraft): DraftErrors {
  const errors: DraftErrors = {};
  if (draft.id.trim().length === 0) {
    errors.id = 'id_required';
  } else if (!ID_PATTERN.test(draft.id.trim())) {
    errors.id = 'id_invalid';
  }
  if (draft.name.trim().length === 0) {
    errors.name = 'name_required';
  }
  if (draft.model.trim().length === 0) {
    errors.model = 'model_required';
  }
  const temperature = draft.temperature.trim();
  if (temperature.length > 0) {
    const value = Number(temperature);
    if (!Number.isFinite(value) || value < 0 || value > 2) {
      errors.temperature = 'temperature_range';
    }
  }
  return errors;
}

function csv(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

function yamlList(items: string[]): string {
  return `[${items.map((item) => JSON.stringify(item)).join(', ')}]`;
}

function yamlString(value: string): string {
  return JSON.stringify(value);
}

/** Serialize a valid draft to the definition file contents. */
export function draftToMarkdown(draft: AgentDraft): string {
  const frontmatter = ['---', `id: ${draft.id.trim()}`, `name: ${yamlString(draft.name.trim())}`];
  if (draft.description.trim().length > 0) {
    frontmatter.push(`description: ${yamlString(draft.description.trim())}`);
  }
  frontmatter.push('version: 0.1.0', `model: ${draft.model.trim()}`);
  if (draft.temperature.trim().length > 0) {
    frontmatter.push(`temperature: ${Number(draft.temperature)}`);
  }
  frontmatter.push(
    `tools: ${yamlList(csv(draft.tools))}`,
    `skills: ${yamlList(csv(draft.skills))}`,
    `mcp: ${yamlList(csv(draft.mcp))}`,
  );
  if (draft.icon.trim().length > 0) {
    frontmatter.push(`icon: ${yamlString(draft.icon.trim())}`);
  }
  if (draft.color.trim().length > 0) {
    frontmatter.push(`color: ${yamlString(draft.color.trim())}`);
  }
  frontmatter.push('---', '', draft.prompt.trim(), '');
  return frontmatter.join('\n');
}

/** A draft for a copy of `draft` with a unique id. */
export function duplicateDraft(draft: AgentDraft, taken: string[]): AgentDraft {
  const base = `${draft.id.trim() || 'agent'}-copy`;
  let id = base;
  let n = 2;
  while (taken.includes(id)) {
    id = `${base}-${n}`;
    n += 1;
  }
  return { ...draft, id, name: `${draft.name} copy` };
}
