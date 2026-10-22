# OE runtime 0.6.5+oe.7

This revision keeps the existing semantic authority and refreshes reviewed book-source identities without changing chapter completeness or declared capabilities. The source review records all 43 old/new file digests: 38 files change only image extensions, while five retain reviewed engineering-method additions. Missing source content and partial chapter status are not promoted by rebinding.

Refinery operations now reuse successful immutable run and receipt validation within a single exclusive operation. Every visit still reads the full CAS bytes and verifies their digests; event, index and contextual relationships remain checked. The memo is discarded on success or failure, with no cross-operation or stat-based trust. Tests cover unchanged-size/mtime content tampering, mutable relationship changes and cleanup after failure.

The Explorer lock updates affected transitive build dependencies within the existing declared ranges. Its test suites and production build are run after the update.
