# Base structured generation and embeddings slice

The original complete 13,494-gate scope remains incomplete. This slice extends
the existing native provider path; it does not replace full HuggingFace inference,
LiteLLM dispatch, typed Pydantic/instructor behavior or all embedding models.

The existing Harness is the only shared model-call owner. Chat and each
OpenAI embedding batch item use the same injected `SharedModelCallContext`.
Failures consume reserved attempts; unknown usage remains unknown and cost is
not estimated from successful calls. No second credential pool, registry,
provider cache, provenance session or budget ledger is introduced.

`providers base-structured` accepts the generated `BaseStructuredRequest`.
The original uppercase `JSON` check is case-sensitive. Empty/falsy responses
retry without sleeping; parsing/generation errors use the original 2/4-second
backoff, the final falsy response is returned, and nonpositive integer retries yield `[]`
without a dispatch. The generated field uses i64; values above 4096 are an explicit owner limit, while Python arbitrary-size/bool coercion remains a parity gap. Guard/budget refusals stop safely. Logical request identity
is separate from every actual augmented plain-generation request. The generated
port is implemented by `RustBaseStructuredHarness`; its calls reuse the same
neutral parser, HTTP transport and result guard.

`providers embed` accepts a generated `SemanticEmbeddingRequest` or sequential
`SemanticEmbeddingBatchRequest`. OpenAIStore forwards only `model` and `input`,
reads the first `data` element and casts to float32. Results encode dtype, shape
and IEEE754 bits, preserving rounding, negative zero and cast overflow/nonfinite
values without changing them into JSON null. This is a new explicit wire
encoding. Non-JSON NaN/Infinity literals, overflowed JSON exponents and exact
NaN-payload compatibility remain parity gaps in the original input domain.
Batch execution stops at the first failure and exposes no partial batch vector
value. Prior checked call observations and the failed request identity remain
traceable. Empty batch output follows original `np.array([])` float64 shape `[0]`;
ragged successful results fail under the original NumPy >= 2 semantics.

Chat and embedding transports have explicit operation scope and borrow the same
original credential pool. Scope mismatch fails before a lease or HTTP request.
Local synthetic execution accepts literal loopback addresses and is never
presented as live provider inference. Real credentials are supplied only through
the explicit owner-checked 0600 file interface and never enter generated DTOs.

`providers pool` implements the five documented finite f64 2D pooling algorithms
on supplied vectors. Hierarchical pooling averages each chunk first and then
weights chunks equally, including the shorter last chunk. The explicit profile
is `semantica.provided-vectors-finite-f64-2d/1`; other NumPy ranks/dtypes,
nonfinite propagation and custom method registry dispatch are not signed off.
Precomputed vectors do not demonstrate a model or semantic embedding generation.

Successful generation attempts use the same `DatasetSession`/PROV algorithm,
with actual input/wire/response identities, software agent/activity and unknown
cost. Call completion does not admit engineering facts or establish adoption.

Full remaining work includes Base typed schemas/instructor and repair modes;
native HuggingFace tokenizer/model/device/eval/causal and seq2seq generation;
LiteLLM's original model/vendor dispatch and response variants; FastEmbed,
sentence-transformers and BGE inference/model switching; nonsemantic deterministic
NumPy fallback; graph/vector manager formatting, configuration and custom method
registries. Abstract/random original placeholders remain original placeholders.
Missing current dependencies are a slice boundary, not a permanent omission.
Any native inference backend must bind approved model/tokenizer bytes and verify
real local operator behavior, while retaining the same upper owner authorities.

Declarations and mappings are edited in `specs/engineering.toml` and the existing
migration mappings. Shared types/ports are produced by locked official ggen;
slice tests and declarations never count as full original-gate verification.
