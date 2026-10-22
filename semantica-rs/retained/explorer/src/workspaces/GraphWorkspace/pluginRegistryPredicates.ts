/**
 * shouldLoad predicates for the GraphWorkspace lazy plugin registry.
 *
 * Extracted into a pure module so the predicates can be unit-tested without
 * importing the full GraphWorkspace React component. Each predicate gates
 * whether a plugin's module is lazily imported; none reference temporalState
 * so temporal scrubber updates never retrigger plugin loading (issue #830).
 */

export type PluginShouldLoadContext = {
  panelState: Record<string, boolean>;
};

export function explorationEffectsShouldLoad({ panelState }: PluginShouldLoadContext): boolean {
  return Boolean(panelState["effects-panel"]);
}

export function neighborhoodPanelShouldLoad({ panelState }: PluginShouldLoadContext): boolean {
  return Boolean(panelState["neighborhood-panel"]);
}

export function temporalOverlayShouldLoad({ panelState }: PluginShouldLoadContext): boolean {
  return Boolean(panelState["temporal-panel"]);
}
