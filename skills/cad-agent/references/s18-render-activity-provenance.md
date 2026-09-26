# S18 configuration-bound rendering

## Purpose

Use this reference for Fusion Render workspace tasks involving a posed
assembly, Named View, Scene Settings, Environment Library or custom HDRI,
in-canvas rendering, local rendering, or a saved rendered image.

This is compact P1/P2 working memory. It guides real execution and verification;
it does not add release gates to an ordinary rendering exercise.

## Engineering model

A rendered image is the output of a configuration-bound activity:

```text
RenderActivity
  -> exact source document/version
  -> AssemblyConfiguration
  -> NamedView / CameraSetting
  -> SceneSetting
  -> EnvironmentMap
  -> render mode + quality + resolution + transparent-background flag
  -> RenderedArtifact
```

Keep these inputs independent. A good-looking image does not prove the source
assembly, pose, camera, scene, HDRI, quality, completion state, or persistence.
The transparent-background flag keeps the EnvironmentMap as the lighting source
while removing the backdrop from the saved image; it is a compositing output
choice, not a scene-lighting change.

For a jointed assembly, the render pose is an explicit assembly configuration.
Record the relevant joint coordinates or snapshot before rendering. Do not bake
the pose into detached geometry merely to obtain a picture.

## Flat Fusion execution

1. Bind and activate the exact document label and lineage. When multiple Fusion
   labels or projects are present, switch to the designated label and leave the
   others untouched.
2. Confirm Fusion is idle and obtain the `Design` product from that exact
   document.
3. Establish or recall the intended assembly configuration and named camera.
4. Set scene data through `design.renderManager.sceneSettings`.
5. Start the native in-canvas or local renderer through the same Design's
   `RenderManager`.
6. Read the native completion state and save the generated image.
7. Save the source document, close/reopen the same lineage, and read back the
   pose, named view, camera, environment, and scene settings.

These are meaningful engineering stages; do not create a receipt or gate for
each API property assignment.

## Stable Render API mapping

Prefer the formal, Design-bound API:

```python
render_manager = design.renderManager
scene = render_manager.sceneSettings

environment = adsk.fusion.RenderEnvironment.loadCustomEnvironment(hdr_path)
scene.backgroundEnvironment = environment
scene.isGroundDisplayed = True
scene.isGroundFlattened = True
scene.isGroundReflections = False
scene.cameraType = adsk.core.CameraTypes.PerspectiveCameraType
scene.cameraFocalLength = 50.0

in_canvas = render_manager.inCanvasRendering
local = render_manager.rendering
```

Ground scale (magnitudes around 134) and environment rotation (sun direction
relative to the subject) also belong to Scene Settings: they control the
subject's apparent scale and the lighting direction. If the 2704 API does not
expose them, record them as UI-only observations rather than omitting them.

Do not execute or attach event handlers to internal command definitions such as
`RenderingEnvCmd` to mutate scene data. A UI command lifecycle is not a scene
data API and can invalidate Fusion. Do not use accessibility-tree input to
drive floating Scene Settings controls when the formal API covers the state.

For P2 course UI credit, observe the live control path and resulting visible
state separately. API reproduction can establish the P1 engineering result but
must not be reported as an exact UI action.

## Environment-map identity

A custom environment is an external engineering input, not merely a filename.
Record:

- source and license;
- local path used by Fusion;
- `.hdr` or `.exr` format;
- 2:1 equirectangular dimensions;
- content digest;
- intended background/lighting role.

Attaching an environment and rendering from it are separate state transitions.
The former changes Scene Settings; the latter generates a new artifact.

## Native acceptance

For a routine lesson, enough evidence is:

- exact target document and Design binding;
- expected assembly snapshot/joint coordinates;
- expected Named View and camera type/focal length;
- native Scene Settings readback, including ground scale and environment
  rotation (or their UI-only observation when the API does not expose them);
- environment identity and digest;
- native render start and completion state;
- image file existence, dimensions, and digest;
- document save plus same-lineage reopen readback;
- proportional assembly validity checks when the pose changed.

Peekaboo screenshots may show the visible Render workspace or reopened scene.
They do not replace `SceneSettings`, `RenderFuture`, saved-image, assembly, or
reopen evidence.

## S18 reproduced precedent

`S18_Minifigure_Render_A0001` established the working pattern with:

- `S18 Render Walking Pose`;
- `S18 Render Hero View`;
- an 8K `dikhololo_night` HDRI;
- ground displayed and flattened, reflections off;
- perspective camera at 50 mm;
- native in-canvas render;
- Print 3300 × 2550 local render at quality 100;
- zero unexpected assembly interference after reopen.

The reusable lesson is the relation structure and API boundary above, not those
product-specific pose angles or asset choices.
