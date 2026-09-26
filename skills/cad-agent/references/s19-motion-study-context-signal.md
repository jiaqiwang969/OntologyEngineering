# S19 — Motion Study context and signal playback

State: Candidate working memory grounded in `S19/attempt-0001`. It is advisory
and release-local; it does not authorize native bridging, model mutation, or
runtime activation.

## Use this route

Use this reference when a saved Fusion Motion Study appears empty, when a
study must be reopened without mouse/keyboard input, when motion curves need a
signal companion, or when course credit depends on Fusion's native Play/Stop/
Repeat controls rather than an agent-side joint sampler.

Classify the situation before acting:

```text
saved kinematic program
  -> exact document/version and owned MotionStudy identity
  -> MotionStudySelection context
  -> native edit dialog and curve rows
  -> transport control and visible pose/cursor change
  -> clean close/reopen readback
```

Do not collapse this into dynamics, stress, fatigue, lifetime, or continuous
collision evidence. A Motion Study is a kinematic signal program. Its positive
use is that one driver propagating through the joint/Motion-Link chain is a
cheap whole-machine kinematic sanity check — it is signal, not acceptance.

## Context-restoration rule

`EditMotionStudyCmd` without its native `MotionStudySelection` context can open
a valid zero-row dialog. That observation does not prove that the persisted
study is empty. The bounded recovery order is:

1. reopen the exact lineage/version;
2. resolve the root Component and require the intended owned Motion Study;
3. match each study operand to its public Joint token;
4. compare every persisted curve point, driver flag, and ordering;
5. construct the native selection context before invoking the edit command;
6. query the live dialog for the expected row and grip cardinalities;
7. leave the saved model and neutral assembly pose unchanged.

For the frozen S19 episode, the accepted model has one study, seven revolute
rows, 24 explicit driver points, and 38 total nodes when implicit endpoints are
included. Those values identify that episode; later studies must bind their own
cardinalities rather than copying S19's PASS.

## Native playback and independent signal view

Native playback credit requires the live Fusion dialog and controls:

- Play creates a native animation task and advances the curve cursor;
- different visible frames show different poses/cursor/value states;
- Stop removes the task without changing the stopped cursor position;
- Move to Start returns the cursor to zero;
- Repeat is a native dialog mode, not a loop around API joint writes.

Unclaimed credit: the source video also teaches play-once, forward-backward,
and speed-rate playback semantics; all three are taught-but-unprobed and need a
mode/speed probe on the pinned release before any credit claim.

The HTML companion
`curriculum-learning/S19/attempt-0001/artifacts/S19_motion_signal_playback.html`
exposes the same seven programmed signals for inspection. It is useful as a
portable dynamic-time signal graph, but it is not Fusion playback authority
and cannot earn native P2 transport credit.

The no-input native bridge is pinned to Fusion `2704.1.36`, Qt `6.8.3`, exact
binary anchors, widget classes, and native window identities. Revalidate those
facts after a Fusion/Qt update. Do not infer compatibility from compilation
alone and do not silently degrade to event injection.

## Evidence and credit boundary

Primary lesson evidence:

- `curriculum-learning/S19/attempt-0001/practice-log.md`
- `curriculum-learning/S19/attempt-0001/checks/motion-study-persisted-readback-v1.json`
- `curriculum-learning/S19/attempt-0001/checks/motion-study-native-edit-direct-v5-final-visible.json`
- `curriculum-learning/S19/attempt-0001/checks/motion-study-native-control-play-v3.json`
- `curriculum-learning/S19/attempt-0001/checks/motion-study-native-control-stop-v2.json`
- `curriculum-learning/S19/attempt-0001/checks/motion-study-native-playback-clean-readback-v1.json`

The episode earns its recorded P2 native edit/playback/display evidence only
for that exact release and observed controls. The reusable lesson is the typed
context-restoration and validation route; historical curve values and native
addresses are not reusable results.

