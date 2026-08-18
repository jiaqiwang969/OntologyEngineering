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

Store these in a separate controlled root. Public manifests use logical IDs and hashes, never private
locations or verbatim restricted text. Package-level denylisted paths are emergency protection only;
never treat `.gitignore` as permission to co-locate private evidence with the public book.

## Allow only after review

- original explanations and propositions;
- synthetic cases clearly marked as synthetic;
- self-authored ontology, queries, constraints, fixtures and scripts;
- figures whose inputs, identity risks and rights were reviewed;
- cleaned PDFs and assets with explicit release status.

## Provider boundary

Image or language model providers are external dependencies. Open-source the project's prompt
contract, adapters, deterministic normalization, provenance schema and quality gates; do not claim to
open-source a hosted model, platform-internal Skill or private session.

## Required checks

Before release, scan paths and content for secrets and personal locations; inspect document/image
metadata; review the public asset register; inspect Git history; and obtain human technical, rights and
privacy decisions. A scanner pass cannot replace these decisions.
