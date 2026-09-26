# Canonical contribution policy

`ontology-engineering/skills/cad-agent` is the canonical CAD module. The
former top-level `~/.codex/skills/cad-agent` copy is legacy compatibility
content and may be archived after caller migration; do not edit it as an independent source.
Contribute operational knowledge, references, templates, validators, and safe
wrappers directly here. The directory must remain a real directory, not a
symlink to a development checkout.

## Authority and merge direction

- The legacy `~/120-agent-cad/.../.agents/skills/cad-agent` tree is not the
  source of truth and must not overwrite this directory.
- Never run source-to-canonical `rsync --delete` or an old standalone build
  script against this directory.
- When an external repository produces a required runtime or formal ontology
  delta, compare both trees and merge only the reviewed files into this
  canonical directory. Preserve canonical-only files and lessons.
- A mirror or backup may be exported outward from this directory only when the
  user explicitly requests it; an export does not change authority.

## Validation after a contribution

1. Run the skill-creator structural validator on this directory and on
   `assembly/` when that module changed.
2. Run the affected bundled self-tests and static checks.
3. Run `doctor.sh`; `--profile` adds a read-only direct-NX host probe.
   Use a fresh isolated job for native acceptance, not a live user document.
4. Compute the deterministic canonical tree digest from any working directory:

   ```bash
   python3 scripts/canonical_content_digest.py
   ```

   The digest binds file paths and bytes while excluding `BUILD_INFO` (to avoid
   self-reference), `.venv`, cache directories, and generated Python bytecode.
5. Update the canonical content stamp in `BUILD_INFO` with that digest without
   rewriting the historical migration wheel's source provenance or the historical
   Fusion-only runtime wheel identity. Re-run the digest;
   excluding `BUILD_INFO` means the value must remain unchanged.

Direct NX execution is source-based. Fusion is removed from the current tool scope;
its archived wheel identity remains historical provenance and grants no installation
or execution path. Preserve engineering methods when removing software-specific
adapters. Distribution must not reintroduce a retired runtime from old overrides.

Do not claim that a reference snapshot, self-reported manifest, animation, or
plausible viewport proves a native engineering result. Keep PASS/HOLD/UNKNOWN
boundaries fail-closed.
