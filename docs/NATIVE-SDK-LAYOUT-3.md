# Native SDK layout /3 content inspection

The generated CodeNativeSdkInspectPort loads a private ArtifactReference through the existing SkillRoot and returns CodeNativeSdkLayout3 only after two complete read-only content checks. It neither executes tools nor grants owner, native availability, compiled-source or engineering-adoption authority. SDK complete dependency closure remains unknown.

The layout preserves four logical roles: SDKSettings and libSystem regular target pins, plus libc/libm raw leaf-link records. When logical libSystem is itself a link, its separate third raw edge is mandatory. Every edge binds its actual lstat mode, original UTF-8 readlink bytes and digest, logical path and the same regular System target. Only the declared finite graph can be followed; outside-root, absolute, parent traversal, undeclared hops and cycles fail. The SDK root and directory ancestors must be canonical directories without symlinks. Non-UTF-8 links and non-Unix permission semantics are explicitly uncovered.

The actual SDK input was frozen before implementation from xcrun and lstat/readlink. Current SDK root is a real directory; System points to libSystem.B.tbd, while libc/libm point to logical System. The positive test takes the explicit OE_NATIVE_ACTUAL_SDK_LAYOUT fixed-input path and does not copy or modify SDK resources. Mutation controls use private synthetic files only.

The preceding profile/recipe /2 and generic regular-file verifier retain their original byte semantics and reject all leaf links. /3 is a distinct content domain for an explicit later full-profile reference. It does not silently enable the reserved archive algorithm or publish a new registry capability. Original broad 13494, module/protocol 19 and OWNER22 scopes remain incomplete.
