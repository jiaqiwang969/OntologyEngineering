RDFLib 7.6.0 license retained for the Rust ports of compare.py Color/_TripleCanonicalizer and the source graph grammar reductions/input preparation from plugins/sparql/parser.py and term.py BNode. Original source and exact source spans are frozen privately; this is an implementation attribution, not engineering adoption authority.

BSD 3-Clause License

Copyright (c) 2002-2026, RDFLib Team
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

TriX private ingestion in `crates/semantica-runtime/src/dataset/source_ingest.rs` also ports the state transitions of RDFLib 7.6.0 `rdflib/plugins/parsers/trix.py` (SHA256 821fe6b2a0f11bcc6e88d514ade7b8d8a6c32541d18e6aa9f596e3c18de40509). The retained RDFLib BSD-3-Clause license and notices apply. XML events use the existing locked quick-xml dependency; explicit source/core coverage and atomic publication belong to the existing Rust DatasetSession.

Source UPDATE preparation in `vendor/spargebra/src/source_update.rs`, `parser/source_update.rs` and `parser/source_frontend.rs` follows the pinned RDFLib 7.6.0 parser.py/algebra.py/update.py operation and translation meanings under the retained RDFLib BSD-3-Clause notice. It reuses the original source-construction and triple-ordering ports; native execution semantics are not asserted by parsing acceptance.

Source Turtle namespace preparation in `crates/semantica-runtime/src/dataset/source_namespace.rs` and `source_turtle.rs` ports RDFLib 7.6.0 `namespace/__init__.py` and `plugins/serializers/turtle.py` under the same BSD-3-Clause notice. The exact default prefix table and Unicode 15.1 category membership are source-derived algorithmic data. This attribution does not claim complete Turtle serialization or acceptance of a new public format.
# Additional internal Turtle text port

`source_turtle_writer.rs` and `source_turtle_literal.rs` adapt RDFLib 7.6.0
`plugins/serializers/turtle.py` and `term.py` under the retained RDFLib BSD-3-Clause
license. The source Memory adapter reuses the existing source value and CPython
sort owners; their original license notices remain. Local finite-resource guards
and public protocol separation are project adaptations, not upstream behavior.

