# Source Turtle preparation

The subsequent [internal full-text candidate](SOURCE-TURTLE-WRITER.md) now uses
this preparation and receives the original complete strings. The description
below records the earlier preparation boundary. Public protocols and platform
selection still require their own reception.

This candidate implements a prerequisite of original-compatible Turtle output.
The existing source-session protocols 1–3 still admit NT/NQ only. The internal
preparation method returns ordered source triples, base and document namespace
declarations; it does not return Turtle text or grant format acceptance.

The source Semantica wrapper parses into a temporary Graph/Dataset, then copies
triples. Input prefixes are not imported into the live store. Turtle may allocate
`ns1`, `ns2`, etc.; clearing RDF leaves these bindings intact. The default graph
and Dataset graph access share bindings but retain separate QName caches and
namespace tries. The implementation stores these with the same DatasetSession,
not a second RDF store. They are presently exercised only through an internal
preparation seam, without altering public load/query/NT/NQ behavior.

The preparation ports RDFLib 7.6.0 namespace splitting, 29 default prefixes,
predicate-only prefix generation, datatype prefix lookup, base handling and
Turtle local-name escaping. Unicode categories use the pinned Python 3.13.11
Unicode 15.1 data through the existing semantica-json lexical owner. This avoids
changing source semantics with the Rust toolchain's Unicode version. Namespace
strings and entries have finite retained budgets; budget refusal is distinct
from an original absent-prefix exception. Generated bindings and prior caches
survive a later refusal, while RDF content/revision are unaffected.

The original `split_uri` XML branch has a non-roundtrip corner case when its XML
namespace occurs twice. Original observations and the compatibility behavior are
retained. This module is not evidence that arbitrary source Turtle output is
lossless or suitable for engineering adoption.

Validation binds 106 before-implementation original QName/pname actions and
three 13-action wrapper sequences. For the wrapper sequences, compare load/clear
results, original loss refusal, namespace declarations, persistent bindings and
revision. Full original Turtle strings are frozen for future writer validation;
preparation does not count as reproducing those strings. Runtime checks also
cover resource refusal, the existing public admission boundary and Unicode/base
corners. Mac and Windows results must be recorded separately; compilation or
one platform's result never stands in for the other.

Remaining work: source term ordering and literal presentation, recursive blank
nodes/lists, complete document writing, live namespace lifecycle beyond this
preparation seam, typed result witnesses and explicit protocol admission, then
current binary/installed-consumer acceptance on both platforms. Other source
formats and complete Rust Semantica remain open. Original engineering goals,
failure observations and current Project/work/Harness retain their meaning.
