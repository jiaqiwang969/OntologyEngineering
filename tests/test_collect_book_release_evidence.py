from __future__ import annotations

import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from scripts import book_release_artifacts as release
from scripts import collect_book_release_evidence as collector


class BookReleaseEvidenceCollectorTests(unittest.TestCase):
    def _governance_root(self, root: Path) -> None:
        values = {
            "docs/PRIVACY-AND-RIGHTS.md": "privacy\n",
            "docs/PUBLIC-RELEASE-STATUS.md": "blocked\n",
            "references/product-trustworthiness-book/handbook/book-metadata.tex": (
                "rights pending\n"
            ),
        }
        for relative, content in values.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

    def test_governance_initializer_stays_pending_and_content_binds_rights(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self._governance_root(root)
            result = collector.initialize_governance(root)
            self.assertEqual(
                {"publication_approval": "pending", "rights": "pending"}, result
            )
            evidence = root / "references" / "release-evidence"
            rights_path = evidence / "rights.json"
            rights = release._load_json_bytes(
                rights_path.read_bytes(), label="rights", canonical=True
            )
            approval = release._load_json_bytes(
                (evidence / "publication-approval.json").read_bytes(),
                label="approval",
                canonical=True,
            )
            self.assertEqual(3, len(rights["sources"]))
            self.assertEqual(
                release.sha256_file(rights_path),
                approval["rights_evidence_sha256"],
            )

    def test_governance_initializer_never_overwrites_approval(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self._governance_root(root)
            collector.initialize_governance(root)
            approval = (
                root / "references" / "release-evidence" / "publication-approval.json"
            )
            document = json.loads(approval.read_text(encoding="utf-8"))
            document["status"] = "approved"
            approval.write_bytes(release.canonical_bytes(document))
            with self.assertRaises(collector.EvidenceCollectionError):
                collector.initialize_governance(root)

    def test_governance_initializer_preserves_blocked_decision(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self._governance_root(root)
            collector.initialize_governance(root)
            evidence = root / "references" / "release-evidence"
            rights_path = evidence / "rights.json"
            rights = json.loads(rights_path.read_text(encoding="utf-8"))
            rights["status"] = "blocked"
            rights["reason"] = "explicit rights-holder rejection"
            rights_path.write_bytes(release.canonical_bytes(rights))
            approval_path = evidence / "publication-approval.json"
            approval = json.loads(approval_path.read_text(encoding="utf-8"))
            approval["status"] = "blocked"
            approval["reason"] = "publication explicitly rejected"
            approval["rights_evidence_sha256"] = release.sha256_file(rights_path)
            approval_path.write_bytes(release.canonical_bytes(approval))

            result = collector.initialize_governance(root)
            self.assertEqual(
                {"publication_approval": "blocked", "rights": "blocked"}, result
            )
            self.assertEqual(
                "explicit rights-holder rejection",
                json.loads(rights_path.read_text(encoding="utf-8"))["reason"],
            )

    def test_font_embedding_parser_reads_emb_column_only(self) -> None:
        header = (
            "name                                 type              encoding         "
            "emb sub uni object ID"
        )
        row = (
            f"{'FakeFont':<37}{'CID Type 0C':<18}{'Identity-H':<17}"
            f"{'no':<4}{'no':<4}{'yes':<4}1 0"
        )
        completed = subprocess.CompletedProcess(
            args=["pdffonts", "fake.pdf"],
            returncode=0,
            stdout=f"{header}\n{'-' * len(header)}\n{row}\n",
            stderr="",
        )
        with mock.patch.object(release.subprocess, "run", return_value=completed):
            self.assertEqual((False, 1), release._pdf_font_report(Path("fake.pdf")))

    def test_regression_report_stores_logical_command_not_host_path(self) -> None:
        report = collector._regression_report(
            repository="https://example.invalid/repository.git",
            commit="a" * 40,
            display_command=("runtime-python", "-m", "pytest", "-q"),
            completed=subprocess.CompletedProcess(
                args=["runtime-python", "-m", "pytest", "-q"],
                returncode=0,
                stdout="3 passed in 0.01s\n",
                stderr="",
            ),
            log={
                "path": "references/release-evidence/test.log",
                "sha256": "a" * 64,
                "size_bytes": 17,
            },
        )
        self.assertTrue(report["passed"])
        self.assertEqual(["runtime-python", "-m", "pytest", "-q"], report["command"])
        self.assertNotIn(str(Path.home()), release.canonical_bytes(report).decode())


if __name__ == "__main__":
    unittest.main()
