# sparopt 0.3.7 / OE arithmetic-chain transport

The original crate archive is pinned to SHA256 `973ee503ab3d6c3de66b4b459233cd5b22b44dad0fb7b8d0931daf73d2ede935`, Oxigraph commit `0e81a29d27575ab69e7f85ea9c7f1058b277fcd0`. `UPSTREAM-PROVENANCE.json` records original source and manifest bytes; both same-commit MIT/Apache-2.0 license texts are retained. Those baseline hashes do not identify the current patched files.

The local patch appends `AdditiveChain` and `MultiplicativeChain` to the existing expression enum and reuses the operator tags from spargebra. Forward/reverse lowering preserves first, ordered tail and nested boundaries directly, including existing graph, aggregate and complex-order transport. The variable visitor traverses every operand. Existing enum discriminants are retained; tests compare all 23 prior variants with a test-only original enum snapshot using both recorded hash bytes and the existing hasher.

Chain normalization returns the entire chain unchanged, loop-join fitness is false and type inference returns ANY. These conservative arms do not make whole-query optimization safe for the private nullable lane: that lane must skip the entire optimizer. Default parser paths still build their previous ASTs. The retained package version plus workspace path and actual compiled-source identity identifies this local implementation; no public nullable query profile is added.

Cross-component previous-source identity and finite applicability are recorded in `../spareval/NULLABLE-CHAIN-PROVENANCE.json`; the maintained contract and explanation are `../../specs/modules/semantic-nullable-chains.toml` and `../../docs/SEMANTIC-NULLABLE-CHAINS.md`. Test presence and source ownership are not current behavior qualification or installation receipts.
