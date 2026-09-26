# S11 curved-surface feature method gate

Use this reference when a CAD operation maps planar or spatial source geometry onto a curved target, or when two tools claim the same nominal depth but use different direction fields.

## Core rule

A curved-surface feature is not identified by depth magnitude alone. Record:

- source profiles;
- target surface;
- mapping method;
- signed depth;
- depth direction frame;
- participant body ownership;
- method-specific postcondition.

Do not treat a fixed-vector Extrude Cut as equivalent to a surface-normal or radial Deboss unless the direction fields and resulting geometry are proven equivalent.

## Cylinder Deboss witness

For an inward radial Deboss on a cylinder:

```text
floor radius = target radius - inward depth
```

S11 example:

```text
0.200 in - 0.010 in = 0.190 in
```

The readback must find the intended number of cylindrical floor faces at that radius. A screenshot alone cannot prove depth direction.

## Planar-to-curved dependency

The operation plan must preserve the dependency chain:

```text
source plane -> source sketch/profiles -> target curved face -> mapping method -> signed depth -> topology witness
```

If a Fusion API rejects a multi-profile input despite common sketch ownership, an ordered single-profile fallback is acceptable only when:

- every feature uses the same method and depth expression;
- every feature targets the intended surface;
- the final topology witness covers the complete profile set;
- the fallback is disclosed in provenance.

## Appearance ownership and localization

Treat appearance as an owned semantic assignment, not a global visual side effect.

- Body-level appearance belongs to the body.
- Detail appearance belongs to explicit faces.
- Record canonical asset identity, exact localized library identity, library name, and design-copy name.
- Do not pass an appearance gate by checking only for a non-null appearance; inherited defaults create false positives.

S11 canonical/localized pairs:

```text
Paint - Enamel Glossy (Yellow)
\u6d82\u6599 - \u74f7\u91c9\u6709\u5149\u6cfd(\u9ec4\u8272)

Plastic - Glossy (Black)
\u5851\u6599 - \u6709\u5149\u6cfd(\u9ed1\u8272)
```

Use Unicode normalization when resolving localized names.

## Transactional Fusion mutation

Resolve external assets and validate risky preconditions before the first mutation in a script. A later exception can roll back every earlier mutation in that transaction.

Required recovery evidence:

- failed operation and exact error;
- whether earlier mutations persisted or rolled back;
- revised strategy;
- independent post-recovery readback.

Never assume a tool call's early successful steps persisted after a later exception.

## Safe readback composition

On the S11 Fusion build, these getters entered an editing rollback path and must not be relied on without a minimal regression fixture:

```text
EmbossFeature.inputFaces
EmbossFeature.profiles
ExtrudeFeature.participantBodies
```

Use stable composition evidence instead:

- feature count, name, health, and depth expression;
- explicit provenance attributes written at mutation time;
- body count and bounding box;
- analytic face radii and expected face count;
- exact appearance identities;
- visual evidence as a secondary check.

## Constraint honesty

An exact current result is not automatically change-safe. Report sketch constraint state independently. Underconstrained geometry can pass a lesson's current-state shape gate while remaining a warning for replay or production reuse.

## Drawing callout semantics

An N-X leader counts occurrences of the same edge or feature across views, not additional fillets. TYP declares the default radius for unlabeled edges.

## Drawing scale

Drawing and detail scales are presentation metadata. Never multiply model dimensions by sheet scale.

## Promotion and next-cycle use

This reference is accepted for immediate use by the next curriculum lesson. Broader changes to the shared ontology, Fusion MCP, or video-learning skill remain candidates until recurrence, a regression fixture, or explicit review supports promotion.

This curriculum reference does not authorize semantic release or production activation.
