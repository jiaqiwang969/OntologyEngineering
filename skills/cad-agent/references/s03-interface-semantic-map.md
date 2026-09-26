# S03 interface semantic map

## Source-grounded interpretation

S03 teaches nine shared interface terms: Data Panel, Application Bar, Toolbar, Browser, Assembly in the Canvas, ViewCube, Marking Menu, Navigation Bar, and Timeline.

The goal is common professional terminology. For an agent, that terminology must become semantic routing knowledge, not a set of screen coordinates.

## Nine-region routing map

| Region | Stable responsibility | Preferred current anchor |
| --- | --- | --- |
| Data Panel | Hubs, projects, designs, samples, collaboration data | DataPanelCommand |
| Application Bar | File, save, undo/redo, documents, status, help, account | Application-level command set |
| Toolbar | Workspace, tabs, panels, contextual tools, commands | Workspace ID, tab ID, panel ID |
| Browser | Active-design object hierarchy and visibility | Browser palette and model API |
| Assembly in the Canvas | Graphical objects and selection | Application.activeViewport |
| ViewCube | Named and incremental orientation | ViewCube command and camera |
| Marking Menu | Context-sensitive frequent commands | Current right-click context |
| Navigation Bar | Orbit, pan, zoom, display, camera, grid | Viewport and navigation command IDs |
| Timeline | Ordered parametric operations and dependencies | Design.timeline |

## Critical distinctions

- Data Panel contains project and design data. Browser contains objects inside the active design.
- Workspace defines the task context. Tab defines a logical domain. Panel groups tools. Command performs an action.
- ViewCube controls orientation. Navigation Bar controls view motion and display. Canvas contains graphical selection.
- Marking Menu, contextual tabs, and contextual environments depend on current selection and command state.
- Timeline order carries dependency meaning; earlier edits may update later operations.
- Display controls (visual style, environment, camera projection, grid) change presentation state only; any geometry change must be traceable to a feature/timeline operation.

## Current contact

Fusion version 2704.1.23 was mapped read-only in Chinese localization. FusionSolidEnvironment and SolidTab were active. Browser was a visible palette. The existing design exposed 4 occurrences, 3 root bodies, 2 root sketches, and a 17-entry timeline.

Current UI adds or exposes tabs such as Assembly and Manage beyond the 2023 arrangement. Do not assume the source grouping is current.

## Recovery order

1. Current API object or command ID.
2. Current Autodesk semantic documentation.
3. Current localized label and tooltip.
4. Command search or contextual menu.
5. Current documented shortcut.
6. Human visual confirmation.

Never click a coordinate remembered from a dated video.

The source interface URL is stale. Rediscover the current official topic while preserving the stale-link provenance.

## Mac shortcut snapshot

- Data Panel: Option+Command+P
- Browser: Option+Command+B
- ViewCube: Option+Command+V
- Navigation Bar: Option+Command+N
- Reset default layout: Option+Command+R

Shortcuts are current-evidence fallbacks, not timeless axioms.

## Evaluation boundary

S03 may advance the curriculum but does not prove production readiness. Semantic release and production activation remain separately gated.
