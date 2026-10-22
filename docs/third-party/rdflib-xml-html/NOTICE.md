The HTML serialization and optional-tag logic in rdflib_xml_html.rs is a Rust
translation of html5rdf 1.2.1 default serializer behavior. Its full original MIT
notice is included at the top of the algorithm source and copied from the actual
installed distribution into LICENSES/html5rdf-1.2.1/LICENSE.txt.

The private candidate uses official html5ever 0.35.0 and quick-xml 0.37.5. All 44
resolved package license declarations, actual source paths, source manifest
hashes, registry checksums, and local license copies are enumerated in LICENSES.json.
The 19 coordinates new to the baseline are marked individually. Reference
RDFLib 7.6.0 and Python 3.13.11 licenses are also preserved there.
mac 0.1.1 has actual published manifest/README license declarations but no
standalone license text in its package; no copyright text was inferred.
match_token shared repository license files were fetched from the exact commit
recorded by its published .cargo_vcs_info.json, with source URLs and hashes.

rdflib_xml_name_ranges.rs contains generated observations of the complete pinned
Expat 2.7.3 BMP NCName alphabet, through the real RDFLib _parseXML converter.
XML-NAME-ALPHABET.json is its frozen data source. It is generated character
classification data, not a copied business model or a registry of fixture keys.
The generator and reference source/binary hashes remain private proof artifacts.

No reference source, public source, accepted baseline lock or original expected
result was edited. Source identity/adoption and whole-domain parity remain unknown.

This pure helper has finite lexical qualification only and owns no host resource
budget. Host input structure/UTF8 and output limits must reject resources directly
at the DatasetSession call boundary, before/independent of the lexical parse-error
fallback. The DOM walker is recursive; synthetic HTML5 nodes mean input '<' count
is not an exact DOM depth. Host resource-bound proof and owner atomicity must be
qualified independently by the owner and are not implied by these lexical results.
