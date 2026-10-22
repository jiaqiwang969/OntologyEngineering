# Scope assignment

Each `<owner>.json` lists the frozen Semantica scope items (ids from `runtime/semantica-scope.json`) that the owner must give a disposition in `semantica-rs/ledger/<owner>.json`. Generated from the frozen scope; do not edit by hand.

Port order (waves; a wave may use earlier ones):

1. sem-utils
2. sem-provenance
3. sem-normalize, sem-parse, sem-pipeline, sem-ingest, sem-conflicts, sem-dedup, sem-embeddings, sem-change
4. sem-extract
5. sem-kg
6. sem-reasoning, sem-vector-store, sem-triplet-store, sem-export, sem-visualization, sem-llms, sem-seed
7. sem-ontology, sem-context
8. sem-core, sem-mcp, sem-explorer, sem-integrations, semantica
9. repo
