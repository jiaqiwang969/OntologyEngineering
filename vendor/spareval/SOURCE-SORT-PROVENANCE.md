# Source ORDER index sorting provenance

`src/eval/source_sort.rs` is a Rust adaptation of the natural-run, binary
insertion, Powersort, merge and galloping portions of CPython 3.13.11
[`Objects/listobject.c`](https://raw.githubusercontent.com/python/cpython/v3.13.11/Objects/listobject.c),
126949 bytes, SHA256
`fe0a926ac4f1ea51b818b7401696ac23df38569b75216e69084cc8991baaa6c2`.
It is an adaptation of those algorithms, not merely a reference to behavior.

Copyright (c) 2001-2024 Python Software Foundation; All Rights Reserved.
`LICENSE-CPYTHON` retains the complete license text from the same fixed upstream
CPython v3.13.11 source release. The existing spareval MIT and Apache notices
remain unchanged and describe their existing upstream code; they do not replace
the Python attribution for this local sorting adaptation.

Changes from the original: the Rust kernel moves `usize` indices into caller
owned key and row arrays; it does not own Python objects or evaluate expressions.
CPython pointer cursors are represented by safe Rust indices, including exclusive
backward cursors in `merge_hi`. Comparator failures return their original typed
value through `SourceSortError::Callback`. Real `try_reserve_exact` failures and
checked index overflow are explicit active resource failures. Cancellation is
checked at entry, before each comparison and during long moves/reversals. These
active resource boundaries are not claims about CPython allocation failures,
cancellation or operating-system scheduling. On failure the caller discards the
private permutation and must not publish any sorted row prefix.

Preserved source schedule: ascending-first natural-run detection, equal descending
subrun reversal, binary insertion to the original minrun, Powersort run powers
and pending merges, initial gallop trimming, both merge directions, adaptive
`min_gallop`, and DESC pre/post reversal while continuing to compare with LT.
Inconsistent-comparison exhaustion branches are retained; no total-order or
strict-weak-order assumption is introduced. Scratch grows from actual merge
need. There is no added row count limit derived from numeric or text budgets.
The source's Python reference-count cleanup and arbitrary user mutation of the
list are outside this immutable RDF key/index interface.

The pinned original source is separate from the independent observed reference.
The latter uses an installed Nix CPython 3.13.11 build with an identified raw
executable and linked libpython. Its declared patches do not modify listobject.c,
but its source archive was not locally retained for a reproduced build; the
observations are therefore attributed to that fixed Nix build, not to a newly
reproduced pristine-tag binary. Only original `builtins.sorted` and synthetic
index-tagged comparison keys were run by the independent reference author. No
candidate output supplied those expected comparison transcripts or permutations.

The original reference freeze, SHA256
`533a4a9aaf381db47da90ff1252ad442e817c0eb22a9ff01250b5dacfc5ba8ea`,
contains 25 vectors: 22 successful permutations and three original marker-error
prefixes, with 16007 recorded ordered comparison pairs including their Boolean
outcomes. Source branch certificates are separate from those observations.
The receiving fixtures and actual candidate executions bind their own source
identities; this provenance document records implementation and origin, not a
claim of execution success, public ORDER support, or complete Semantica parity.

No new sorting crate, Python runtime dependency, public profile, or second query
interpreter is introduced. Raw ORDER key construction and eager/deferred graph
startup belong to the existing source evaluator and are received independently
from this index kernel. Legacy/default expression and graph semantics are not
changed by importing this private module.
