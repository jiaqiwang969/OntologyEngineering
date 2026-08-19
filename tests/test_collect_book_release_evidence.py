from __future__ import annotations

import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from scripts import book_release_artifacts as release
from scripts import check_public_privacy as privacy
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

    def test_font_embedding_parser_accepts_a_name_past_the_header_width(self) -> None:
        header = (
            "name                                 type              encoding         "
            "emb sub uni object ID"
        )
        row = (
            "VASEJB+LMRomanSlant10-Regular-Identity-H "
            "CID Type 0C       Identity-H       yes yes yes    577  0"
        )
        completed = subprocess.CompletedProcess(
            args=["pdffonts", "fake.pdf"],
            returncode=0,
            stdout=f"{header}\n{'-' * len(header)}\n{row}\n",
            stderr="",
        )
        with mock.patch.object(release.subprocess, "run", return_value=completed):
            self.assertEqual((True, 1), release._pdf_font_report(Path("fake.pdf")))

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

    def test_regression_log_normalizes_only_known_checkout_roots(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary).resolve()
            root = parent / "ontology-engineering"
            semantica_root = parent / "semantica"
            unrelated = parent / "unrelated-private-path" / "secret.py"
            completed = subprocess.CompletedProcess(
                args=["runtime-python", "-m", "pytest", "-q"],
                returncode=0,
                stdout=(
                    f"{root}/tests/test_release.py:10: warning\n"
                    f"{semantica_root}/tests/test_ontology.py:20: warning\n"
                    f"{unrelated}:30: must remain visible\n"
                    "3 passed in 0.01s\n"
                ),
                stderr="",
            )

            normalized = collector._normalized_regression_output(
                completed,
                root=root,
                semantica_root=semantica_root,
            ).decode("utf-8")

            self.assertIn(
                "<ontology-engineering-root>/tests/test_release.py", normalized
            )
            self.assertIn("<semantica-root>/tests/test_ontology.py", normalized)
            self.assertNotIn(str(root), normalized)
            self.assertNotIn(str(semantica_root), normalized)
            self.assertIn(str(unrelated), normalized)

    def test_regression_log_has_exactly_one_terminal_lf_for_stdout_only(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary).resolve()
            completed = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout="1 passed in 0.01s\n\n",
                stderr="",
            )

            normalized = collector._normalized_regression_output(
                completed,
                root=parent / "ontology-engineering",
                semantica_root=parent / "semantica",
            )

            self.assertEqual(b"1 passed in 0.01s\n", normalized)

    def test_regression_log_joins_nonempty_stdout_and_stderr_once(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary).resolve()
            completed = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout="test output without terminal newline",
                stderr="warning output\r\n\r\n",
            )

            normalized = collector._normalized_regression_output(
                completed,
                root=parent / "ontology-engineering",
                semantica_root=parent / "semantica",
            )

            self.assertEqual(
                b"test output without terminal newline\nwarning output\n",
                normalized,
            )

    def test_regression_log_does_not_rewrite_similar_path_prefix(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary).resolve()
            root = parent / "ontology-engineering"
            semantica_root = parent / "semantica"
            lookalike = parent / "ontology-engineering-shadow" / "warning.py"
            completed = subprocess.CompletedProcess(
                args=[],
                returncode=0,
                stdout=f"{lookalike}:10: must remain visible\n1 passed in 0.01s\n",
                stderr="",
            )

            normalized = collector._normalized_regression_output(
                completed,
                root=root,
                semantica_root=semantica_root,
            ).decode("utf-8")

            self.assertIn(str(lookalike), normalized)

    def test_unrelated_personal_path_remains_visible_to_privacy_gate(self) -> None:
        personal_root = Path("/") / "Users" / "example-account"
        root = personal_root / "work" / "ontology-engineering"
        semantica_root = personal_root / "work" / "semantica"
        unrelated = personal_root / "other-project" / "secret.py"
        completed = subprocess.CompletedProcess(
            args=[],
            returncode=0,
            stdout=(
                f"{root}/tests/test_release.py:10: known root\n"
                f"{unrelated}:20: unrelated path\n"
                "1 passed in 0.01s\n"
            ),
            stderr="",
        )
        normalized = collector._normalized_regression_output(
            completed,
            root=root,
            semantica_root=semantica_root,
        ).decode("utf-8")

        with tempfile.TemporaryDirectory() as temporary:
            scan_root = Path(temporary)
            log_path = scan_root / "regression.log"
            log_path.write_text(normalized, encoding="utf-8")
            findings = privacy.content_findings(log_path, scan_root)

        self.assertIn("<ontology-engineering-root>/tests/test_release.py", normalized)
        self.assertNotIn(str(root), normalized)
        self.assertIn(str(unrelated), normalized)
        self.assertEqual(
            ["macOS personal absolute path"],
            [finding.rule for finding in findings],
        )


if __name__ == "__main__":
    unittest.main()
