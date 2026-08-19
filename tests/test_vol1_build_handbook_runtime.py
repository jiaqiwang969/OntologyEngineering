from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

from ontology_engineering import semantica_runtime as runtime


ROOT = Path(__file__).resolve().parents[1]
HANDBOOK = (
    ROOT
    / "references"
    / "ontology-engineering-book"
    / "handbook"
)
if str(HANDBOOK) not in sys.path:
    sys.path.insert(0, str(HANDBOOK))

import build_handbook as builder  # noqa: E402

COMMIT = "1" * 40
VERSION = "9.9.9+book.1"
WHEEL = "semantica-9.9.9+book.1-py3-none-any.whl"
DIGEST = "2" * 64


class Vol1BuilderRuntimeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.identity = runtime.RuntimeSourceLock(
            commit=COMMIT,
            version=VERSION,
            artifact_filename=WHEEL,
            artifact_sha256=DIGEST,
        )

    def test_formal_mode_is_default_and_uses_only_verified_lock_identity(self) -> None:
        with mock.patch.object(
            builder,
            "verify_runtime_source_identity",
            return_value=self.identity,
        ) as verify, mock.patch.object(
            builder, "read_staging_runtime_descriptor"
        ) as read_staging:
            provenance = builder.select_runtime_provenance()

        verify.assert_called_once_with()
        read_staging.assert_not_called()
        self.assertEqual("formal-source-lock", provenance["mode"])
        self.assertTrue(provenance["authoritative_runtime_identity"])
        self.assertEqual("runtime/semantica-source-lock.json", provenance["descriptor"])
        self.assertEqual(COMMIT, provenance["commit"])
        self.assertEqual(DIGEST, provenance["wheel_sha256"])

    def test_staging_mode_is_explicit_content_bound_and_non_authoritative(self) -> None:
        descriptor_path = Path("controlled/semantica-staging-runtime.json")
        descriptor = runtime.StagingRuntimeDescriptor(
            commit=COMMIT,
            version=VERSION,
            wheel_filename=WHEEL,
            wheel_sha256=DIGEST,
            descriptor_sha256="3" * 64,
        )
        with mock.patch.object(
            builder,
            "read_staging_runtime_descriptor",
            return_value=descriptor,
        ) as read_staging, mock.patch.object(
            builder,
            "verify_runtime_source_identity",
            return_value=self.identity,
        ) as verify:
            provenance = builder.select_runtime_provenance(descriptor_path)

        read_staging.assert_called_once_with(descriptor_path)
        verify.assert_called_once_with(staging_descriptor=descriptor_path)
        self.assertEqual("staging-non-authoritative", provenance["mode"])
        self.assertFalse(provenance["authoritative_runtime_identity"])
        self.assertEqual("3" * 64, provenance["descriptor_sha256"])
        self.assertNotIn(str(descriptor_path), json.dumps(provenance))
        self.assertIn("must not update", provenance["warning"])

    def test_identity_failure_cannot_delete_the_previous_fragment_snapshot(self) -> None:
        with mock.patch.object(
            builder,
            "select_runtime_provenance",
            side_effect=RuntimeError("identity mismatch"),
        ), mock.patch.object(builder, "clean_output") as clean:
            with self.assertRaisesRegex(RuntimeError, "identity mismatch"):
                builder.main([])
        clean.assert_not_called()

    def test_index_persists_exact_runtime_provenance_without_machine_path(self) -> None:
        provenance = {
            "mode": "staging-non-authoritative",
            "authoritative_runtime_identity": False,
            "descriptor": "explicit-staging-runtime-descriptor",
            "descriptor_sha256": "3" * 64,
            "commit": COMMIT,
            "version": VERSION,
            "wheel_filename": WHEEL,
            "wheel_sha256": DIGEST,
            "installed_identity_verified": True,
        }
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
            builder, "OUT", Path(temporary)
        ):
            builder.emit_section_fragments([], provenance)
            text = (Path(temporary) / "INDEX.md").read_text(encoding="utf-8")

        encoded = text.split("```json\n", 1)[1].split("\n```", 1)[0]
        self.assertEqual(provenance, json.loads(encoded))
        self.assertIn("## Fragment mapping", text)
        self.assertNotIn(temporary, text)


if __name__ == "__main__":
    unittest.main()
