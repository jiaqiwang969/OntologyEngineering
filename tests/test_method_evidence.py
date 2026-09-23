"""Integrity tests for the fact adapter, never a copy of Semantica methods.

Every input is synthetic. Profiles are transport fixtures so these checks do not
need a released bundle and cannot be mistaken for semantic regression results.
"""
from __future__ import annotations

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from ontology_engineering import method_evidence as adapter


NS = "urn:semantica:engineering-evidence:"
SPEC = {
    "package_id": "semantica.synthetic.integrity-fixture",
    "package_version": "0.1.0",
    "package_sha256": "a" * 64,
    "sha256": "b" * 64,
}
REGISTRY = {
    "namespace": NS,
    "focus_type": NS + "ReviewClaim",
    "profiles": {
        "synthetic-transport": {
            "fields": {"label": "string", "value": "number", "flag": "boolean", "items": "strings"},
            # Adapter must not implement method completeness or applicability.
            "required": ["label", "value", "flag", "items"],
        }
    },
}


class MethodEvidenceIntegrityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.evidence = self.root / "evidence"
        self.evidence.mkdir()
        self.source = self.evidence / "facts.json"
        self.record_path = self.root / "record.json"
        self.document = {"label": "synthetic observation", "value": 1.25, "flag": False, "items": ["a", "b"]}
        self.record = {
            "schema": adapter.SCHEMA,
            "record_id": "synthetic-record",
            "claim": {"id": "synthetic-claim", "statement": "transport integrity only", "subject_revision": "v1", "scope": "synthetic-scope"},
            "method": "synthetic-transport",
            "sources": [{"id": "s1", "path": "facts.json", "sha256": "", "media_type": "application/json"}],
            "facts": {field: {"source_id": "s1", "pointer": "/" + field} for field in self.document},
        }
        self.profile_mock = mock.patch.object(adapter, "profiles", return_value=(deepcopy(SPEC), deepcopy(REGISTRY)))
        self.profile_mock.start()
        self.addCleanup(self.profile_mock.stop)
        self.write_source()

    def write_source(self, raw=None):
        if raw is None:
            raw = json.dumps(self.document, ensure_ascii=False).encode("utf-8")
        elif isinstance(raw, str):
            raw = raw.encode("utf-8")
        self.source.write_bytes(raw)
        self.record["sources"][0]["sha256"] = hashlib.sha256(raw).hexdigest()

    def project(self):
        self.record_path.write_text(json.dumps(self.record, ensure_ascii=False), encoding="utf-8")
        return adapter.project_record(self.record_path, self.evidence)

    def test_valid_projection_preserves_lineage_and_denies_semantic_verdict(self):
        rdf, audit = self.project()
        text = rdf.decode()
        self.assertIn(f"<{NS}value> 1.25 .", text)
        self.assertIn(f"<{NS}flag> false .", text)
        self.assertEqual(audit["source_integrity"], "verified")
        self.assertEqual(audit["semantic_execution"], "not_run")
        self.assertEqual(audit["engineering_verdict"], "not_assessed")
        self.assertEqual(audit["projection_sha256"], hashlib.sha256(rdf).hexdigest())
        self.assertEqual(audit["record_sha256"], hashlib.sha256(self.record_path.read_bytes()).hexdigest())
        self.assertEqual(audit["field_lineage"]["value"]["pointer"], "/value")
        self.assertEqual(audit["field_lineage"]["value"]["source_sha256"], self.record["sources"][0]["sha256"])
        self.assertEqual(audit["query_asset"], "findings-synthetic-transport")
        self.assertEqual(audit["shape_asset"], "shape-synthetic-transport")

    def test_modified_source_bytes_are_rejected(self):
        self.source.write_text('{"value": 9}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "source hash mismatch"):
            self.project()

    def test_forged_declared_hash_is_rejected(self):
        self.record["sources"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "source hash mismatch"):
            self.project()

    def test_duplicate_json_keys_in_source_are_rejected_even_with_current_hash(self):
        self.write_source('{"value":1,"value":2}')
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            self.project()

    def test_duplicate_json_keys_in_record_are_rejected(self):
        raw = json.dumps(self.record)
        raw = raw[:-1] + ',"method":"synthetic-transport"}'
        self.record_path.write_text(raw, encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            adapter.project_record(self.record_path, self.evidence)

    def test_duplicate_nested_claim_key_is_rejected(self):
        self.project()
        raw = self.record_path.read_text().replace('"scope": "synthetic-scope"', '"scope": "synthetic-scope", "scope": "other"')
        self.record_path.write_text(raw)
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            adapter.project_record(self.record_path, self.evidence)

    def test_boolean_is_not_a_number(self):
        for value in (False, True):
            with self.subTest(value=value):
                self.document["value"] = value
                self.write_source()
                with self.assertRaisesRegex(ValueError, "fact type mismatch: value"):
                    self.project()

    def test_numeric_zero_or_one_is_not_boolean(self):
        for value in (0, 1, 0.0, 1.0):
            with self.subTest(value=value):
                self.document["flag"] = value
                self.write_source()
                with self.assertRaisesRegex(ValueError, "fact type mismatch: flag"):
                    self.project()

    def test_nonfinite_json_numbers_are_rejected(self):
        for token in ("NaN", "Infinity", "-Infinity", "1e999"):
            with self.subTest(token=token):
                self.record["facts"] = {"value": {"source_id": "s1", "pointer": "/value"}}
                self.write_source('{"value":' + token + '}')
                with self.assertRaises(ValueError):
                    self.project()

    def test_string_list_rejects_wrong_members_duplicates_and_blank_values(self):
        for value in (["a", 1], ["a", False], ["a", "a"], ["a", ""], ["a", "   "]):
            with self.subTest(value=value):
                self.document["items"] = value
                self.write_source()
                with self.assertRaisesRegex(ValueError, "fact type mismatch: items"):
                    self.project()

    def test_empty_list_and_missing_list_remain_distinct_without_semantic_inference(self):
        self.document["items"] = []
        self.write_source()
        rdf_empty, audit_empty = self.project()
        self.assertIn(f"<{NS}itemsComplete> true .", rdf_empty.decode())
        self.assertNotIn(f"<{NS}items> ", rdf_empty.decode())
        del self.record["facts"]["items"]
        rdf_missing, audit_missing = self.project()
        self.assertNotIn(f"<{NS}itemsComplete>", rdf_missing.decode())
        self.assertIn("items", audit_empty["field_lineage"])
        self.assertNotIn("items", audit_missing["field_lineage"])
        self.assertEqual(audit_missing["semantic_execution"], "not_run")

    def test_omitted_required_profile_fact_is_left_for_semantica(self):
        del self.record["facts"]["value"]
        rdf, audit = self.project()
        self.assertNotIn(f"<{NS}value>", rdf.decode())
        self.assertEqual(audit["engineering_verdict"], "not_assessed")

    def test_json_pointer_escapes_and_array_indices_select_exact_source_values(self):
        self.document = {"a/b": {"m~n": ["exact source value"]}, "": {"value": 7}}
        self.record["facts"] = {"label": {"source_id": "s1", "pointer": "/a~1b/m~0n/0"}, "value": {"source_id": "s1", "pointer": "//value"}}
        self.write_source()
        rdf, audit = self.project()
        self.assertIn('"exact source value"', rdf.decode())
        self.assertIn(f"<{NS}value> 7 .", rdf.decode())
        self.assertEqual(audit["field_lineage"]["label"]["pointer"], "/a~1b/m~0n/0")

    def test_unresolved_and_noncanonical_array_pointers_are_rejected(self):
        self.document = {"array": ["a", "b"]}
        self.write_source()
        for pointer in ("/missing", "/array/01", "/array/-1", "/array/-", "/array/2", "/array/~2", "array/0"):
            with self.subTest(pointer=pointer):
                self.record["facts"] = {"label": {"source_id": "s1", "pointer": pointer}}
                with self.assertRaises(ValueError):
                    self.project()

    def test_unknown_source_reference_is_rejected(self):
        self.record["facts"]["value"]["source_id"] = "not-declared"
        with self.assertRaisesRegex(ValueError, "unknown fact source"):
            self.project()

    def test_duplicate_source_ids_are_rejected(self):
        self.record["sources"].append(deepcopy(self.record["sources"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate source ID"):
            self.project()

    def test_fact_literal_or_unknown_field_cannot_bypass_source_pointer(self):
        for mutation in ("literal", "extra"):
            with self.subTest(mutation=mutation):
                before = deepcopy(self.record["facts"])
                if mutation == "literal":
                    self.record["facts"]["value"] = 1.25
                else:
                    self.record["facts"]["undeclared"] = {"source_id": "s1", "pointer": "/value"}
                with self.assertRaises(ValueError):
                    self.project()
                self.record["facts"] = before

    def test_unknown_method_cannot_select_an_unlocked_profile(self):
        self.record["method"] = "not-in-locked-profile"
        with self.assertRaisesRegex(ValueError, "locked Semantica package"):
            self.project()

    def test_source_paths_cannot_escape_or_use_ambiguous_segments(self):
        outside = self.root / "outside.json"
        outside.write_bytes(self.source.read_bytes())
        for path in ("../outside.json", str(outside), "sub/../../outside.json", "./facts.json", "sub//facts.json", "sub/../facts.json", "sub\\facts.json"):
            with self.subTest(path=path):
                self.record["sources"][0]["path"] = path
                with self.assertRaisesRegex(ValueError, "relative file|escapes"):
                    self.project()

    def test_symlink_source_and_parent_directory_are_rejected(self):
        alias = self.evidence / "alias.json"
        alias.symlink_to(self.source)
        directory_alias = self.evidence / "alias-dir"
        directory_alias.symlink_to(self.evidence, target_is_directory=True)
        for path in ("alias.json", "alias-dir/facts.json"):
            with self.subTest(path=path):
                self.record["sources"][0]["path"] = path
                with self.assertRaisesRegex(ValueError, "symlink"):
                    self.project()

    def test_media_type_must_describe_supported_json_fact_snapshot(self):
        self.record["sources"][0]["media_type"] = "text/plain"
        with self.assertRaisesRegex(ValueError, "controlled JSON fact snapshot"):
            self.project()

    def test_claim_iri_and_literals_cannot_inject_rdf_statements(self):
        self.record["claim"]["id"] = 'synthetic > . <urn:bad> <urn:bad> "injected"'
        self.record["claim"]["statement"] = 'line one\n"quoted" \\ slash'
        rdf, audit = self.project()
        self.assertNotIn("<urn:bad>", rdf.decode())
        self.assertIn("%3E", audit["focus"])
        self.assertIn('line one\\n\\"quoted\\"', rdf.decode())


if __name__ == "__main__":
    unittest.main()
