# Full native3 through the existing build owner

`oev2 versions supervise-full-native-build --input ...` resolves the original
`CodeBuildSupervisionRequest` /1 through the existing registry authority. The
caller supplies no recipe, tool, target, side, budget, capture or success flag.
Only explicit policy /3, the existing `supervise_build` actor action and the
selected total-budget context /2 authorize dispatch. Full3 content inspection
cannot supply that authority.

`NativeVersion::Full3` is chosen by the entry function, never inferred from JSON.
The appended native section has one common supervision body for native2/full3,
the original owner folder and nonblocking `selection.lock`, and one monotonic
start/deadline including content inspection, lock acquisition, process execution
and final verification. Each remaining body budget subtracts elapsed time and
the existing 250 ms cleanup reserve. Target, stage and evidence freshness are
checked before and after the same lock. The original legacy 15226 byte prefix
is retained exactly.

The full3 branch constructs actual generated side /3 and bounded request /3,
executes `build-full-native-budgeted`, and decodes actual capture /3. Acceptance
requires stdout to equal `native-full-capture.json` byte for byte, unchanged
profile/prior manifest/selected driver/tools/target and artifact identity, the
original debug runner path, the bootstrap normalizer path, compiled Rust package
version/target, the full-native capture verifier and final source/context/side/
lock/deadline checks. The original five-stage pipeline builds the normalizer at
`target/debug/oev2-design-bootstrap` and the target runner at
`target/<platform>/debug/oe-code-runner`.

Failure retains raw process and observation, unknown compilation origin and
release eligibility, no accepted artifact/capture and no selection or activation.
The tests execute only approved false or shell failure fixtures and held-lock /
early-denial controls. Nonexecuted synthetic native-tool pins and actual SDK3
content inspection do not establish native build provenance, signatures, readable
DWARF, byte reproducibility, project adoption or production confinement. Successful
full builds and whole 13494 identities / 19 modules / 19 protocols / 22 owners,
all platforms and engineering delivery still require independent evidence.
