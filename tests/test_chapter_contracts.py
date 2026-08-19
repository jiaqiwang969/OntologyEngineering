from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import unittest

from scripts import validate_chapter_contracts as contracts


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
VALIDATOR = REPOSITORY_ROOT / "scripts" / "validate_chapter_contracts.py"


class RepositoryChapterPackageTests(unittest.TestCase):
    def test_semantica_registry_is_the_only_29_chapter_contract_source(self) -> None:
        report = contracts.validate_contracts(REPOSITORY_ROOT)
        self.assertTrue(report.structurally_valid, "\n".join(report.errors))
        self.assertEqual(29, report.contract_count)
        self.assertEqual(29, len(report.blockers))
        self.assertEqual((), report.complete_contracts)
        self.assertFalse(report.release_ready)
        self.assertFalse((REPOSITORY_ROOT / "contracts").exists())

    def test_default_cli_passes_and_release_gate_fails_closed(self) -> None:
        default = subprocess.run(
            [sys.executable, str(VALIDATOR)],
            cwd=REPOSITORY_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(0, default.returncode, default.stderr)
        self.assertIn("contracts=29", default.stdout)
        self.assertIn("structurally_valid=1", default.stdout)

        release = subprocess.run(
            [sys.executable, str(VALIDATOR), "--release"],
            cwd=REPOSITORY_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(1, release.returncode)
        self.assertIn("release_ready=0", release.stdout)
        self.assertEqual(29, release.stderr.count("BLOCKED:"))

    def test_repository_contains_no_executable_semantic_asset_files(self) -> None:
        suffixes = {
            ".jsonld", ".nq", ".nt", ".owl", ".rdf", ".rq", ".shacl",
            ".sparql", ".swrl", ".trig", ".trix", ".ttl",
        }
        files = [
            path.relative_to(REPOSITORY_ROOT).as_posix()
            for path in REPOSITORY_ROOT.rglob("*")
            if path.is_file()
            and path.suffix.lower() in suffixes
            and ".venv" not in path.parts
        ]
        self.assertEqual([], files)


if __name__ == "__main__":
    unittest.main()
