# Privacy-first release rules

Use default deny and an explicit public allowlist.

## Keep private

- raw standards and restricted extracts;
- purchase, account and download records;
- enterprise, customer, supplier, worker, product and incident data;
- model chats, sessions, rollouts, hidden prompts and attachment caches;
- passwords, tokens, cookies, keys and `.env` files;
- personal absolute paths and local network inventory;
- reference images or outputs with unclear input rights;
- unpublished review comments and identity-bearing metadata.

Store them in a separate controlled root. Public registers use logical IDs and
hashes, never private locations or verbatim restricted text. A denylist is
emergency protection, not permission to co-locate private evidence.

## The semantic boundary

The book is the external specification. Do not publish book-local ontology,
CQ, shape, query, case, rule, fixture or runner payloads. These are not merely
private: they are architecturally forbidden because Semantica is their single
authoritative and executable home.

The public book may contain stable Semantica IDs, package proposal/binding
metadata, an artifact/source lock, a native execution receipt and its release
verdict. All OntologyEngineering calls into Semantica must be routed through
`ontology_engineering.semantica_runtime`; no direct backend import is allowed.

## Allow only after review

- original explanations and non-quoting proposition summaries;
- narrative synthetic examples clearly marked as synthetic;
- source/chapter/proposition maps containing logical IDs and hashes;
- figures whose inputs, identity risks and rights were reviewed;
- cleaned books, the routing Skill and Semantica evidence with explicit release status.

## Provider boundary

Image or language model providers are external dependencies. Open-source the
project's prompt contract, adapters, deterministic normalization, provenance
schema and quality gates only when rights permit; do not claim to open-source
a hosted model, platform-internal Skill or private session.

## Required checks

Before release, scan paths and content for secrets and personal locations;
reject semantic payload suffixes and parallel semantic roots; inspect document
and image metadata; review the public asset allowlist; inspect Git history;
cross-check the Semantica source lock, receipt and complete verdict; and obtain
human technical, rights and privacy decisions. A scanner or Semantica pass
cannot replace those decisions.
