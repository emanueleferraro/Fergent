# @fergents/plugin-sdk

The authoring SDK for Fergents plugin UI modules (slot API v1).

A plugin's `[ui].entry` bundle default-exports a definition produced by
`definePlugin`. The shell registers each contribution in its slot registry and
mounts the surface inside a shadow root, so plugin CSS can never leak into the
shell.

The SDK is UI-only: it grants no permissions and makes no kernel calls of its
own. Every call goes through the injected `bridge`, which the host binds to the
owning plugin id. The kernel remains the security boundary.

## Example

```tsx
import { definePlugin, type SlotProps } from '@fergents/plugin-sdk';

function RailBadge({ bridge }: SlotProps) {
  return (
    <button
      type="button"
      className="btn btn-sm"
      onClick={() => void bridge.call('rss.refresh')}
    >
      Refresh feeds
    </button>
  );
}

export default definePlugin({
  id: 'com.example.rss-reader',
  slots: [{ id: 'rail', slot: 'rail.item', order: 10, render: RailBadge }],
  commands: [{ id: 'refresh', label: 'Refresh feeds' }],
});
```

`definePlugin` throws on an invalid id, an unsupported `apiVersion`, an unknown
slot, a duplicate contribution, or a command without a label. `factory.test`
runs the same validation before a package is built.

## API

| Export | Purpose |
|---|---|
| `API_VERSION` | The slot API version this SDK targets (`1`). |
| `SLOTS` / `SlotName` / `isSlotName` | The 16 slot names of `UI.md` §22. |
| `isApiCompatible(required, current?)` | N and N−1 rule, identical to the kernel. |
| `PluginBridge` | `{ pluginId, apiVersion, call(method, params) }`. |
| `SlotProps` | `{ bridge }`, the props every surface receives. |
| `SlotSpec` | `{ id, slot, order?, render }`. |
| `CommandSpec` | `{ id, label, shortcut? }`; the palette shows `id` as `plugin.<plugin-id>.<id>`. |
| `definePlugin`, `PluginDefinition` | Validation plus defaults for a UI module. |

## Styling

Surfaces render inside a shadow root that already contains the shell's
`tokens.css` and `components.css`. Use the `style/` design tokens and component
classes (`btn`, `card`, `badge`, `input`, `field`, …) so plugin UI matches the
shell; do not ship your own reset or global selectors.

## Permissions

Declare permissions in `plugin.toml`, not in the UI module. The user approves
them at install time, and the kernel rejects any call the plugin did not
declare in `[ui].kernel_calls` (or `[kernel]` for tool calls).
