/**
 * `@fergents/plugin-sdk` — the authoring surface for plugin UI modules.
 *
 * A plugin's `ui` bundle default-exports a `PluginDefinition` produced by
 * `definePlugin`. The shell reads the definition, registers each slot
 * contribution in its slot registry, and mounts the surface inside a shadow
 * root (`UI.md` §22, `PLUGINS.md` §10).
 *
 * The SDK is UI-only. It does not grant permissions and performs no kernel
 * calls of its own: every call goes through `SlotProps.bridge`, which the host
 * binds to the owning plugin id. The kernel remains the security boundary.
 */
/** The API version implemented by this SDK. */
export const API_VERSION = 1;

/** The 16 slots of `UI.md` §22 (slot API v1). */
export const SLOTS = [
  'rail.item',
  'sidebar.section',
  'main.tab',
  'editor.toolbar',
  'editor.gutter',
  'chat.message.actions',
  'chat.composer.extras',
  'chat.permission.card',
  'statusbar.item',
  'rightpanel.section',
  'dashboard.widget',
  'command.palette',
  'settings.section',
  'graph.overlay',
  'workflow.node',
  'theme.tokens',
] as const;

/** A named slot. */
export type SlotName = (typeof SLOTS)[number];

/** Whether `value` is one of the 16 slots. */
export function isSlotName(value: string): value is SlotName {
  return (SLOTS as readonly string[]).includes(value);
}

/**
 * Whether a surface targeting `required` can run on `current`.
 *
 * Mirrors `fergents_packages::api_compatible`: the shell supports its own API
 * version and the previous one (N-1).
 */
export function isApiCompatible(required: number, current: number = API_VERSION): boolean {
  return required === current || (current > 0 && required + 1 === current);
}

/**
 * Kernel access bound to the owning plugin.
 *
 * The host injects `plugin_id` on every call; the surface cannot spoof it.
 */
export interface PluginBridge {
  /** The plugin the surface belongs to. */
  readonly pluginId: string;
  /** The slot API version negotiated with the shell. */
  readonly apiVersion: number;
  /** Call a kernel method the plugin declared in `[ui].kernel_calls`. */
  call<T>(method: string, params?: Record<string, unknown>): Promise<T>;
}

/** Props every slot surface receives. */
export interface SlotProps {
  /** Kernel access bound to the owning plugin. */
  readonly bridge: PluginBridge;
}

/**
 * A surface component.
 *
 * Structurally identical to a React function component, so the SDK itself
 * needs no React dependency and a plugin can typecheck before adding React.
 */
export type SlotComponent = (props: SlotProps) => unknown;

/** One contribution to a slot. */
export interface SlotSpec {
  /** Unique within the plugin for this slot. */
  id: string;
  /** Where the surface renders. */
  slot: SlotName;
  /** Lower renders first; ties break on plugin id, then contribution id. */
  order?: number;
  /** The surface; it is mounted inside a shadow root. */
  render: SlotComponent;
}

/** A palette command contributed by the plugin. */
export interface CommandSpec {
  id: string;
  label: string;
  shortcut?: string;
}

/** A validated plugin UI module. */
export interface PluginDefinition {
  readonly id: string;
  readonly apiVersion: number;
  readonly slots: readonly SlotSpec[];
  readonly commands: readonly CommandSpec[];
}

/** What `definePlugin` accepts. */
export interface PluginDefinitionInput {
  id: string;
  /** Defaults to {@link API_VERSION}. */
  apiVersion?: number;
  slots?: readonly SlotSpec[];
  commands?: readonly CommandSpec[];
}

/** Mirrors the kernel's plugin id rule (`crates/plugins/src/manifest.rs`). */
function isValidId(id: string): boolean {
  if (id === '') {
    return false;
  }
  if (![...id].every((char) => /[a-z0-9._-]/.test(char))) {
    return false;
  }
  return !['.', '-', '_'].includes(id.charAt(0));
}

/**
 * Validate a plugin UI module and fill in defaults.
 *
 * Throws on an invalid id, an unsupported API version, an unknown slot, a
 * duplicate contribution, or a malformed command. Validation is duplicated at
 * build time by `factory.test`, so a bad module fails before it ships.
 */
export function definePlugin(input: PluginDefinitionInput): PluginDefinition {
  const id = input.id.trim();
  if (!isValidId(id)) {
    throw new Error(`invalid plugin id: ${JSON.stringify(input.id)}`);
  }

  const apiVersion = input.apiVersion ?? API_VERSION;
  if (!Number.isInteger(apiVersion) || apiVersion < 0 || !isApiCompatible(apiVersion)) {
    throw new Error(
      `api_version ${String(input.apiVersion)} is not supported by the SDK (${API_VERSION})`,
    );
  }

  const seenSlots = new Set<string>();
  const slots = [...(input.slots ?? [])].map((spec) => {
    if (!isSlotName(spec.slot)) {
      throw new Error(`unknown plugin slot: ${String(spec.slot)}`);
    }
    if (spec.id.trim() === '') {
      throw new Error(`slot contribution in ${spec.slot} needs an id`);
    }
    if (spec.order !== undefined && !Number.isFinite(spec.order)) {
      throw new Error(`slot contribution ${spec.id} has a non-numeric order`);
    }
    const key = `${spec.slot}:${spec.id}`;
    if (seenSlots.has(key)) {
      throw new Error(`duplicate slot contribution: ${key}`);
    }
    seenSlots.add(key);
    return spec;
  });

  const seenCommands = new Set<string>();
  const commands = [...(input.commands ?? [])].map((spec) => {
    if (spec.id.trim() === '') {
      throw new Error('a command needs an id');
    }
    if (spec.label.trim() === '') {
      throw new Error(`command ${spec.id} needs a label`);
    }
    if (seenCommands.has(spec.id)) {
      throw new Error(`duplicate command: ${spec.id}`);
    }
    seenCommands.add(spec.id);
    return spec;
  });

  return { id, apiVersion, slots, commands };
}
