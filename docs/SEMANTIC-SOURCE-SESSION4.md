# Explicit source-session /4

This candidate connects the existing source DatasetSession, shared action dispatch,
namespace lifecycle and Turtle writer to the kernel and bounded same-program CLI.
Request `/4` and profile `/4` are explicit; `/1–3` retain their contracts. Public
query `/1–4` and the refusal of query `/5` are separate.

One request owns one ephemeral session. Each executed action retains namespace
state before execution, after execution and after the existing live state observer,
plus a typed observation. Capturing a checkpoint does not initialize a manager or
add an observation. Successful Turtle returns exact original text and a complete
writer transcript; NT/NQ and diff retain their existing result shapes. A failed
action preserves its original failure and known effects; later mutations stop,
while admitted reads continue. A capture refusal stops all remaining actions and
is partial, including an initial or final capture refusal. Unretained state is
unknown, not an empty state or rollback. Original failures are not normalized.

The retained initial, completed step/witness and final payloads share a 32 MiB
budget; bounded failure/skip metadata remains in the 64 MiB output envelope.
The host's 60 s absolute deadline includes child execution and receiving. Kernel
checks bind request, source, steps, phase chain, typed state, Turtle reads and
exact text. Host checks additionally bind the actual executable, input replay,
raw subprocess result and unchanged execution closure. Self-consistent JSON
alone does not authenticate execution or establish engineering adoption.

Frozen independent original observations cover 10 histories and 68 actions,
including syntax errors, lossy serialization refusal, partial updates, cache
reuse, selected graphs, base aliases, snapshots, diffs and post-failure reads.
Their private inputs stay outside distribution. Separate controls cover changed
identity, coverage, phase chain, writer text/reads, charges, typed observations,
capture limits, deadline and protocol isolation. Actual pass/fail and source
identity are in maintenance receipts; this design text grants no test pass.

macOS and Windows use the same generated contracts and Rust owners. Library,
actual CLI, explicit new-root installation, platform receiving and formal
selection require their own receipts. Existing selected versions are not changed
by this candidate. The original 63 goals, full Semantica modules/protocols/owners,
source books, CAD16, manufacturing, inspection, final metrology, Project/work/
Harness and feedback obligations remain. This increment establishes no RRSI net
benefit and does not resolve historical UP0078/UP0726.
