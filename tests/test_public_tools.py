from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

from scripts import check_public_privacy as PRIVACY  # noqa: E402


class PrivacyGateTests(unittest.TestCase):
    """Keep public-corpus safety tests separate from Semantica package tests."""

    def test_safe_tree_passes_and_personal_path_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "safe.md").write_text(
                "synthetic manufacturing example\n", encoding="utf-8"
            )
            findings, checked = PRIVACY.run(
                root, tracked_only=False, include_ignored=True
            )
            self.assertEqual(checked, 1)
            self.assertEqual(findings, [])

            personal_path = "/" + "Users" + "/alice/private/source.pdf"
            (root / "unsafe.md").write_text(personal_path + "\n", encoding="utf-8")
            findings, checked = PRIVACY.run(
                root, tracked_only=False, include_ignored=True
            )
            self.assertEqual(checked, 2)
            self.assertTrue(
                any(item.rule == "macOS personal absolute path" for item in findings)
            )

    def test_env_example_is_allowed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / ".env.example").write_text(
                "PLACEHOLDER=not-a-secret\n", encoding="utf-8"
            )
            findings, _ = PRIVACY.run(
                root, tracked_only=False, include_ignored=True
            )
            self.assertEqual(findings, [])

    def test_nul_byte_in_declared_text_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "reader.md").write_bytes(b"# Reader\n\x00binary\n")
            findings, _ = PRIVACY.run(
                root, tracked_only=False, include_ignored=True
            )
            self.assertTrue(any("NUL byte" in item.rule for item in findings))

    def test_wheel_is_treated_as_a_binary_release_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "semantica.whl").write_bytes(b"PK\x03\x04\x00release")
            findings, checked = PRIVACY.run(
                root, tracked_only=False, include_ignored=True
            )
            self.assertEqual(checked, 1)
            self.assertEqual(findings, [])

    def test_deleted_tracked_path_is_not_a_release_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(
                ["git", "init", "-q"], cwd=root, check=True, capture_output=True
            )
            removed = root / "removed.md"
            removed.write_text("obsolete\n", encoding="utf-8")
            subprocess.run(
                ["git", "add", "removed.md"], cwd=root, check=True, capture_output=True
            )
            removed.unlink()
            candidates = PRIVACY.git_candidates(root, tracked_only=True)
            self.assertEqual(candidates, [])

    def test_symbolic_link_is_rejected_without_following_target(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            private_target = "/" + "Users" + "/alice/private/book.pdf"
            (root / "external-link").symlink_to(private_target)
            findings, checked = PRIVACY.run(
                root, tracked_only=False, include_ignored=True
            )
            self.assertEqual(checked, 1)
            self.assertTrue(any("symbolic link" in item.rule for item in findings))

    def test_special_filesystem_node_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fifo = root / "unmanifested-payload"
            os.mkfifo(fifo)
            findings, checked = PRIVACY.run(
                root, tracked_only=False, include_ignored=True
            )
            self.assertEqual(checked, 1)
            self.assertTrue(
                any("special filesystem node" in item.rule for item in findings)
            )


if __name__ == "__main__":
    unittest.main()
