/**
 * Regression tests for issue #830: plugin registry shouldLoad predicates.
 *
 * Imports the production predicates from pluginRegistryPredicates.ts so that
 * a regression in GraphWorkspace.tsx is detected here. The key invariant: no
 * predicate may read temporalState — doing so caused a render loop because
 * temporalState.currentTime is non-null from startup, which triggered eager
 * plugin loads on every scrubber update and continuously cancelled in-flight
 * load() calls before they could register the plugin.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

const {
  explorationEffectsShouldLoad,
  neighborhoodPanelShouldLoad,
  temporalOverlayShouldLoad,
} = require("../src/workspaces/GraphWorkspace/pluginRegistryPredicates.ts");

// ── temporal-overlay ─────────────────────────────────────────────────────────

test("temporal-overlay shouldLoad: false when panel is closed and no scrubber time", () => {
  assert.equal(
    temporalOverlayShouldLoad({ panelState: { "temporal-panel": false } }),
    false,
  );
});

test("temporal-overlay shouldLoad: false when panel is closed even if scrubber time is set", () => {
  // Before the fix, a non-null currentTime caused an eager load on every scrubber update.
  assert.equal(
    temporalOverlayShouldLoad({
      panelState: { "temporal-panel": false },
      temporalState: { currentTime: new Date() },
    }),
    false,
  );
});

test("temporal-overlay shouldLoad: true only when the panel is explicitly opened", () => {
  assert.equal(
    temporalOverlayShouldLoad({ panelState: { "temporal-panel": true } }),
    true,
  );
});

test("temporal-overlay shouldLoad: true when panel opened even without a scrubber time", () => {
  assert.equal(
    temporalOverlayShouldLoad({
      panelState: { "temporal-panel": true },
      temporalState: { currentTime: null },
    }),
    true,
  );
});

// ── other entries — confirm they also gate only on panelState ─────────────────

test("exploration-effects shouldLoad: gates only on effects-panel state", () => {
  assert.equal(explorationEffectsShouldLoad({ panelState: { "effects-panel": false } }), false);
  assert.equal(explorationEffectsShouldLoad({ panelState: { "effects-panel": true } }), true);
});

test("neighborhood-panel shouldLoad: gates only on neighborhood-panel state", () => {
  assert.equal(neighborhoodPanelShouldLoad({ panelState: { "neighborhood-panel": false } }), false);
  assert.equal(neighborhoodPanelShouldLoad({ panelState: { "neighborhood-panel": true } }), true);
});

test("all three shouldLoad conditions are consistent: none reference temporalState", () => {
  // A regressed predicate reading temporalState?.currentTime would return true
  // for a closed panel when currentTime is set — detecting the loop bug.
  const nonNullTemporalState = { currentTime: new Date(), activeNodeCount: 6 };

  assert.equal(
    temporalOverlayShouldLoad({ panelState: { "temporal-panel": false }, temporalState: nonNullTemporalState }),
    false,
    "temporal-overlay must not load when panel is closed, regardless of scrubber time",
  );
  assert.equal(
    explorationEffectsShouldLoad({ panelState: { "effects-panel": false }, temporalState: nonNullTemporalState }),
    false,
  );
  assert.equal(
    neighborhoodPanelShouldLoad({ panelState: { "neighborhood-panel": false }, temporalState: nonNullTemporalState }),
    false,
  );
});
