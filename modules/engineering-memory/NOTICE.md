# Source and license notice

The source-window reader in `runtime/src/evidence/` was maintained alongside
[CASS](https://github.com/Dicklesworthstone/coding_agent_session_search).
The retained upstream license is [LICENSE-CASS](LICENSE-CASS), titled
**MIT License (with OpenAI/Anthropic Rider)**. Its additional rider is preserved
verbatim; this module is not described as unrestricted MIT.

The unified runtime retains the engineering-memory application contracts,
source-bound storage, correction history and client exchange implementation.
It no longer bundles a second ontology implementation or the CASS Memory
TypeScript application. CASS and CM installations are not required for these
native commands. Their external capabilities, if selected, must be installed
and configured separately.

The module shares the repository's `crates/semantica-runtime` and its dependency
notices. The macOS snapshot implementation retains the original rustix notice
under `runtime/notices/`. Preserve these notices in redistributions.
