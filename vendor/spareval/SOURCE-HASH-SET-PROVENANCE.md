# Source identity hashing and RHS set provenance

`src/source_hash.rs` and `src/eval/source_set.rs` adapt the relevant algorithms
from CPython 3.13.11. They are algorithm adaptations, not just behavior references.
The fixed upstream files are:

| Source | SHA256 |
| --- | --- |
| [Objects/setobject.c](https://raw.githubusercontent.com/python/cpython/v3.13.11/Objects/setobject.c) | `4edbb0c0da67ec653874c5fe2a138180b9029c3a7169ba56e8f86a5c12b11e88` |
| [Python/pyhash.c](https://raw.githubusercontent.com/python/cpython/v3.13.11/Python/pyhash.c) | `703939f6ce6197d9454fb147092d6854eab598dfc905b811c875d5d3020f2e09` |
| [Python/bootstrap_hash.c](https://raw.githubusercontent.com/python/cpython/v3.13.11/Python/bootstrap_hash.c) | `dd239752ab373d85f01a00a87a8516ba9feb0fdf510adbca402c78d94245400b` |
| [Include/internal/pycore_pyhash.h](https://raw.githubusercontent.com/python/cpython/v3.13.11/Include/internal/pycore_pyhash.h) | `75e733c20f64c03b9f851d4f279708ceb0d29195eae15062d08cfa039fd8c98e` |
| [Objects/typeobject.c](https://raw.githubusercontent.com/python/cpython/v3.13.11/Objects/typeobject.c) | `b4b6c696e7b1a8b38b568409c47d788270c85c9025a82961d604f679593b2831` |

Copyright (c) 2001-2024 Python Software Foundation; All Rights Reserved.
The complete Python license remains in `LICENSE-CPYTHON`. SipHash in pyhash.c
also carries the separate copyright (c) 2013 Marek Majkowski and MIT grant,
retained verbatim in `LICENSE-SIPHASH`; the PSF notice does not replace it.
The existing spareval MIT and Apache notices describe their upstream code.

The hash module fixes the observed 64-bit, little-endian, SipHash13/cutoff-zero
configuration. Unicode is streamed in CPython's canonical one-, two- or four-byte
code-point representation; no UTF-8 substitution or large UTF-32 copy occurs.
It includes seed-zero and the fixed nonzero 32-bit LCG, empty string hash, None's
fixed hash, pointer rotation, and the independent `-1` slot normalization. The
source Literal helper hashes lexical text, optional lowercased language and
optional source datatype; it never hashes the cached numeric/error value. Raw
row XOR is separate from outer normalization. FrozenBindings partial cache
mutation and lifetime belong to the row owner, not the pure hash module.

The caller owns the secret and the identity-origin policy. An explicitly seeded
run can reproduce that hash configuration. A new Rust object uses its own
identity address; it is not a recovered object from an earlier Python process.
No hidden interpreter secret is read. Secret bytes have no Debug/serialization
implementation here. Rust `str` excludes lone surrogates; this input type does
not claim to reconstruct arbitrary Python strings outside the existing text
carrier. Entropy generation, external identity facts and query source keys are
separate integration responsibilities.

The set adaptation covers generic iterable construction with immutable row
objects: eight initial slots, ten-position bounded linear probes, perturbation,
existing/incoming equality direction, same-object shortcut, first representative
retention, both size-dependent growth factors, clean reinsert in old slot order,
and final physical-slot iteration. It does not use Rust HashSet or insertion
order. The caller computes each incoming hash, including repeated insertion of
the same row; stored hashes are reused during resize with no hash/EQ callbacks.
Deletion/dummies, set/dict source fast paths, arbitrary callback mutation and
Python reference-count cleanup are outside this immutable-row construction API.

Rust changes use safe indices, inline initial storage, fallible Vec reservation,
checked capacity/index arithmetic and caller cancellation. Callback errors retain
the original typed object. Cancellation is checked at entry, before equality and
during probing/allocation initialization/rehashing. These are explicit active
resource contracts, not claims about CPython cancellation or allocator failures.
There is no extra row cap or numeric/text output budget. After any insertion
failure the query owner discards the private RHS set and publishes no partial
result from its construction.

Independent expectations are frozen in record SHA256
`9069b3597b3867008827f74397d9bacdab7eb43dcf941f9d1d715d314d45eeb4`:
17 original `set(generator)` cases and three explicit-seed hash runs. Small cases
retain complete callback traces, including Boolean equality outcomes. The 78643
row growth case retains a complete stream digest, count and edge samples, plus
the full permutation digest; its no-collision schedule was also checked by
independent integer arithmetic. Fresh object address/hash observations establish
only the pointer-rotation relationship and repeatability in that process, not
portable address constants. Literal/Frozen XOR expectations are explicitly
source-derived recipes over original built-in hashes, not RDFLib executions.

The actual reference is the previously pinned Nix CPython 3.13.11 executable and
libpython. Its declared patches do not modify the relevant hash/set source, but
it is not claimed to be a locally reproduced pristine-tag binary. The pure-kernel
implementation author prepared the synthetic inputs/runner; a different agent
reviewed, executed and independently checked the original observations before
production implementation. Candidate output did not supply the expected data.
The receiving fixture is a mechanical projection without private source paths.
Actual build/test receipts remain outside this compiled provenance input.

This module introduces no Python runtime dependency, new query interpreter or
public profile. It does not by itself qualify source Join/Minus/context behavior,
FrozenBindings cache lifetime, or full Semantica. Those consumers and their
startup/fault/domain contracts are received separately.

## Default source graph tuple keys

The private immutable source graph also uses the 64-bit tuplehash from [CPython 3.13.11 Objects/tupleobject.c](https://raw.githubusercontent.com/python/cpython/v3.13.11/Objects/tupleobject.c), source SHA256 `6dce7db55610ae4b4caa6cde981f1a4df2a3235489dc29e22788c7f468d2c949`. The PSF copyright/license above applies. Component source term hashes are computed in order before the pure tuple kernel; tuplehash uses its own final `-1` replacement, 1546275796. The caller does not substitute Rust tuple hashing.

`source_memory_graph_cases.json` binds actual original Graph/Memory insertion, all SPO binding patterns, exact returned objects and native tuple hash observations made before source graph implementation. Eleven protocols are received; the DP08 error-cell baseline is retained without qualification. The default context set is copied using the existing no-deletion SourceSet API before physical iteration. The source graph owns one fact set and its derived indexes; these are distinct from an RDF-key metadata cache. Only the immutable default graph/value transport domain is covered. Deletion/dummy slots, named contexts, original compiled BGP and other full source context behavior remain required. The root authored the new probes and receivers; no new independent-agent review is claimed.

## Mutable sets and explicit unquoted context storage

The same CPython set adaptation now includes dummy deletion slots, used/fill, reinsertion, copy, merge and table shrink. The immutable-row callback restrictions remain. `source_mutable_set_cases.json` receives79 original actions; its first observer accidentally retained a mutable alias in full traces. The failed receiver and both original records remain private. The corrected original rerun changed only copied full traces; hashes, counts, edge samples, states and errors were identical. No expected output was taken from the candidate.

`source_memory/contexts.rs` adapts fixed RDFLib7.6.0 `plugins/stores/memory.py` SHA256 `a5f0808447f3cd1932a7a8011dfea7e8c8feab35e322b55041dad598f51426c2`; RDFLib copyright and BSD terms are retained in `LICENSE-RDFLIB`. The existing SourceMemoryGraph owns union/per-context sets, indexes and ordered memberships. Twelve original protocols receive59 actions and2360 reads, including retained parent objects, empty graphs, removal and registration. Single-graph refusal and all previous receipts retain their original scope.

These records qualify neither full query GRAPH evaluation nor persistent DatasetSession, Variable cells, quoted graphs, external Graph instance replacement or complete UPDATE execution. The root authored probes and receivers; no independent-agent acceptance or engineering/method-benefit claim is made.
