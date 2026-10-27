/**
 * The plugin UI host: slot registry and shadow-root surfaces (`UI.md` §22).
 */
export { createPluginBridge, type PluginBridge } from './bridge';
export { PluginSlot, ShadowHost, type PluginSlotProps, type ShadowHostProps } from './ShadowHost';
export {
  filterByApiVersion,
  isSlotName,
  SLOT_NAMES,
  SlotRegistry,
  sortContributions,
  surfaceKey,
  type SlotContribution,
  type SlotName,
  type SlotSurfaceProps,
} from './slots';
export { acquireSurface, resetPluginSurfaces, SHADOW_STYLES, surfaceCount } from './surfaces';
export { useSlotContributions } from './useSlots';
export { HOST_API_VERSION, isApiCompatible } from './version';
