# Native CPU model slice

Neutral Rust executes actual source-selected BERT encoder and Llama causal
weights using locked Candle 0.9.1 and tokenizers 0.21.4. It reads immutable
configuration, safetensors, tokenizer and pipeline assets through the existing
neutral SourceProvider. It downloads no model and delegates to no Python SDK.
Asset role, length and SHA are mandatory and checked before and after execution.
The caller selects source identity; this creates no engineering adoption or
additional registry authority.

The native request/result and ports are generated from the annotated spec.
`providers native` borrows the existing explicit owner credential pool solely as
an input/result secrecy guard, without lease or network. Harness remains the
sole model-call budget owner shared with ordinary HTTP providers. It reserves
before invoking native execution; failure consumes the reservation, exhausted
budget makes no invocation and original empty batch makes no inference.
Observed input/output token counts are distinct from unknown API usage and cost.
The same DatasetSession and provenance record construction retain model,
request, source and executable identities. Completion admits no engineering fact.

BERT respects the actual pipeline pooling and intrinsic Normalize; requesting
normalize=false cannot cancel BGE's Normalize layer. BGEStore also enforces
its own normalize_embeddings=True when no intrinsic Normalize layer exists,
and ignores embed options exactly as its original wrapper does. Padding masks affect both
attention and supported masked mean pooling. WordPiece/BPE tokenization and
actual learned 384-dimensional BGE embeddings are checked against independently
frozen weighted CPU references. Supplied vectors are not inference evidence.

The HF causal wrapper preserves its max_length/new-token aliases, temperature,
original sampling requirement and Unicode character slicing. Structured output
retains HF's own direct-JSON/outer-object parser boundaries. Real Llama logits
are checked against independent model weights. Native sampling has an explicit
new RNG profile; Torch RNG bit compatibility is not claimed. Argmax controls
have a separately named validation profile.

Large model assets remain private, selected through SEMANTICA_NATIVE_FIXTURE_ROOT.
Weighted acceptance tests are explicitly marked as requiring those registered
assets and must be run with --include-ignored for migration acceptance. Normal
library tests never download models. Mathematical expectations in the public
fixture were frozen before the Rust forward implementation; synthetic texts
and public model weights are its only inputs. No original assertion or numeric
tolerance may be changed to make the new implementation pass.

All 13,494 original gates remain in scope and incomplete. Other architectures,
GPU/device modes, full NER/relation/seq2seq pipelines, typed/instructor/Pydantic,
LiteLLM vendor dispatch, the actual FastEmbed backend and its wrappers, fallback,
registry, config, progress and other original methods remain pending. Supported
finite CPU operators do not stand in for those methods. Unimplemented TextEmbedder
options (including prompts/truncation/output conversion) and unsupported causal
generation defaults are explicitly refused; they are not silently discarded.
Strict Rust JSON handling does not claim Python json.loads duplicate-key or
non-finite compatibility. Candle CPU operators and SHA are optimized in the
review dev profile; hashing remains the same exact SHA-256 before/after forward.
