from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SEARCH_SCRIPT = ROOT / "scripts" / "search_ontology_sources.py"
VOL1_CHAPTER_DIRS = (
    "ch01-introduction",
    "ch02-ontology-foundations",
    "ch03-ontology-methodology",
    "ch04-ontology-languages",
    "ch05-reasoning",
    "ch06-applications",
    "ch07-knowledge-graph",
    "ch08-ontology-llm",
    "ch09-capstone-manufacturing",
)
VOL2_CHAPTER_DIRS = (
    "ch01-introduction",
    "ch02-concepts-terminology",
    "ch03-safety-management",
    "ch04-concept-hara",
    "ch05-system-development",
    "ch06-hardware-development",
    "ch07-software-development",
    "ch08-asil-decomposition-dfa",
    "ch09-production-operation",
    "ch10-supporting-processes",
    "ch11-claim-ontology",
    "ch12-identity-ontology",
    "ch13-governance-ontology",
    "ch14-context-hazard-ontology",
    "ch15-requirements-ontology",
    "ch16-measurement-ontology",
    "ch17-change-ontology",
    "ch18-dependency-ontology",
    "ch19-field-ontology",
    "ch20-assurance-ontology",
)
VOL2_APPENDICES = (
    "appendices/appendix-a-semiconductor.md",
    "appendices/appendix-b-motorcycle-truck.md",
    "appendices/appendix-c-glossary.md",
    "appendices/appendix-d-method-tables.md",
)


class SearchPortabilityTests(unittest.TestCase):
    @staticmethod
    def _write_lock(lock: Path, base: Path, relative_paths: list[str]) -> None:
        lock.parent.mkdir(parents=True, exist_ok=True)
        lines = []
        for relative in relative_paths:
            digest = hashlib.sha256((base / relative).read_bytes()).hexdigest()
            lines.append(f"{digest}  {relative}\n")
        lock.write_text("".join(lines), encoding="utf-8")

    def _portable_install(self, root: Path, *, include_vol2: bool = True) -> Path:
        skill = root / "installed ontology skill"
        scripts = skill / "scripts"
        references = skill / "references"
        scripts.mkdir(parents=True)
        shutil.copy2(SEARCH_SCRIPT, scripts / SEARCH_SCRIPT.name)

        vol1 = references / "ontology-engineering-book"
        vol1_entries = ["README.md", "resources/README.md"]
        vol1_entries.extend(f"{directory}/README.md" for directory in VOL1_CHAPTER_DIRS)
        vol1_entries.extend(
            [
                "handbook/README.md",
                "handbook/main.tex",
                "handbook/preamble.tex",
                "handbook/chapters/appB-glossary.tex",
                *(f"handbook/chapters/ch{index:02d}.tex" for index in range(1, 10)),
            ]
        )
        for relative in vol1_entries:
            path = vol1 / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            text = (
                "portable-dual-volume-token from volume one\n"
                if relative == "ch01-introduction/README.md"
                else f"formal Vol.1 fixture: {relative}\n"
            )
            path.write_text(text, encoding="utf-8")
        self._write_lock(
            vol1 / "authoring-sources.sha256",
            vol1,
            vol1_entries,
        )
        if include_vol2:
            vol2 = references / "product-trustworthiness-book"
            vol2_source_entries = [
                "front-matter/preface.md",
                *VOL2_APPENDICES,
                "handbook/book-metadata.tex",
                "handbook/main.tex",
                "handbook/preamble.tex",
                *(f"{directory}/chapter.md" for directory in VOL2_CHAPTER_DIRS),
            ]
            for relative in vol2_source_entries:
                path = vol2 / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                text = (
                    "portable-dual-volume-token from volume two\n"
                    if relative == "ch01-introduction/chapter.md"
                    else f"formal Vol.2 fixture: {relative}\n"
                )
                path.write_text(text, encoding="utf-8")
            author_tool = vol2 / "handbook" / "build_handbook.py"
            author_tool.parent.mkdir(parents=True, exist_ok=True)
            author_tool.write_text("author-tool-only-token\n", encoding="utf-8")
            self._write_lock(
                vol2 / "handbook" / "current-source.sha256",
                vol2,
                [*vol2_source_entries, "handbook/build_handbook.py"],
            )
            vol2_guide_entries = [
                "README.md",
                "propositions-index.md",
                "handbook/README.md",
                *(f"{directory}/README.md" for directory in VOL2_CHAPTER_DIRS),
            ]
            for relative in vol2_guide_entries:
                path = vol2 / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                text = (
                    "portable-guide-token\n"
                    if relative == "ch01-introduction/README.md"
                    else f"formal Vol.2 guide fixture: {relative}\n"
                )
                path.write_text(text, encoding="utf-8")
            self._write_lock(
                vol2 / "handbook" / "formal-search-guides.sha256",
                vol2,
                vol2_guide_entries,
            )
        return skill

    @staticmethod
    def _external_shadow(root: Path) -> Path:
        external = root / "external shadow workspace"
        for name in ("ontology-engineering-book", "product-trustworthiness-book"):
            book = external / name
            book.mkdir(parents=True)
            (book / "chapter.md").write_text(
                "external-shadow-only-token\n", encoding="utf-8"
            )
        return external

    def _run_search(
        self,
        skill: Path,
        cwd: Path,
        external: Path,
        query: str,
        *,
        scope: str = "book",
    ) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ)
        env["ONTOLOGY_ENGINEERING_ROOT"] = str(external)
        return subprocess.run(
            [
                sys.executable,
                str(skill / "scripts" / SEARCH_SCRIPT.name),
                "--scope",
                scope,
                "--root",
                str(external),
                "--limit",
                "12",
                "--json",
                query,
            ],
            cwd=cwd,
            env=env,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_book_scope_uses_both_installed_volumes_from_any_cwd(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology search space ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            unrelated_cwd = root / "unrelated working directory"
            unrelated_cwd.mkdir()

            completed = self._run_search(
                skill,
                unrelated_cwd,
                external,
                "portable-dual-volume-token",
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            payload = json.loads(completed.stdout)
            paths = {item["path"] for item in payload}
            self.assertIn("ontology-engineering-book/ch01-introduction/README.md", paths)
            self.assertIn(
                "product-trustworthiness-book/ch01-introduction/chapter.md", paths
            )
            self.assertTrue(all(not Path(item["path"]).is_absolute() for item in payload))
            self.assertTrue(
                all("provenance_warning" not in item for item in payload)
            )
            self.assertTrue(all("source_sha256" in item for item in payload))

            shadow = self._run_search(
                skill, unrelated_cwd, external, "external-shadow-only-token"
            )
            self.assertEqual(shadow.returncode, 0, shadow.stderr)
            self.assertEqual(json.loads(shadow.stdout), [])

    def test_archive_scope_is_explicit_and_every_hit_carries_a_warning(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology archive scope ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            book = skill / "references" / "product-trustworthiness-book"
            archived = book / "ch11-capstone-three-items" / "chapter.md"
            archived.parent.mkdir(parents=True)
            archived.write_text("historical-archive-only-token\n", encoding="utf-8")
            outline = book / "outlines" / "ch11-outline.md"
            outline.parent.mkdir(parents=True)
            outline.write_text("historical-archive-only-token\n", encoding="utf-8")

            ordinary = self._run_search(
                skill,
                root,
                external,
                "historical-archive-only-token",
            )
            self.assertEqual(ordinary.returncode, 0, ordinary.stderr)
            self.assertEqual([], json.loads(ordinary.stdout))

            archive = self._run_search(
                skill,
                root,
                external,
                "historical-archive-only-token",
                scope="archive",
            )
            self.assertEqual(archive.returncode, 0, archive.stderr)
            self.assertIn("PROVENANCE WARNING", archive.stderr)
            payload = json.loads(archive.stdout)
            self.assertEqual(2, len(payload))
            self.assertTrue(
                all(item["source_status"] == "archive_non_authoritative" for item in payload)
            )
            self.assertTrue(
                all("PROVENANCE WARNING" in item["provenance_warning"] for item in payload)
            )
            self.assertEqual(
                {
                    "product-trustworthiness-book/ch11-capstone-three-items/chapter.md",
                    "product-trustworthiness-book/outlines/ch11-outline.md",
                },
                {item["path"] for item in payload},
            )

    def test_locked_authoring_tool_is_not_treated_as_book_content(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology author tool ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)

            ordinary = self._run_search(
                skill, root, external, "author-tool-only-token"
            )
            self.assertEqual(ordinary.returncode, 0, ordinary.stderr)
            self.assertEqual([], json.loads(ordinary.stdout))

            archive = self._run_search(
                skill,
                root,
                external,
                "author-tool-only-token",
                scope="archive",
            )
            self.assertEqual(archive.returncode, 0, archive.stderr)
            payload = json.loads(archive.stdout)
            self.assertEqual(1, len(payload))
            self.assertEqual(
                "product-trustworthiness-book/handbook/build_handbook.py",
                payload[0]["path"],
            )
            self.assertEqual("archive_non_authoritative", payload[0]["source_status"])

    def test_book_scope_requires_each_volume_author_source_lock(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology missing lock ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            lock = (
                skill
                / "references"
                / "product-trustworthiness-book"
                / "handbook"
                / "current-source.sha256"
            )
            lock.unlink()

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Formal book source lock is missing", completed.stderr)

    def test_book_scope_requires_the_vol2_formal_search_guide_lock(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology missing guide lock ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            lock = (
                skill
                / "references"
                / "product-trustworthiness-book"
                / "handbook"
                / "formal-search-guides.sha256"
            )
            lock.unlink()

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Formal book source lock is missing", completed.stderr)
            self.assertIn("formal-search-guides.sha256", completed.stderr)

    def test_incomplete_vol2_guide_lock_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology incomplete guide lock ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            lock = (
                skill
                / "references"
                / "product-trustworthiness-book"
                / "handbook"
                / "formal-search-guides.sha256"
            )
            retained = [
                line
                for line in lock.read_text(encoding="utf-8").splitlines()
                if not line.endswith("  ch20-assurance-ontology/README.md")
            ]
            lock.write_text("\n".join(retained) + "\n", encoding="utf-8")

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Formal book source lock coverage mismatch", completed.stderr)
            self.assertIn("ch20-assurance-ontology/README.md", completed.stderr)

    def test_vol1_authoring_lock_must_include_every_reader_guide(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology incomplete vol1 lock ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            lock = (
                skill
                / "references"
                / "ontology-engineering-book"
                / "authoring-sources.sha256"
            )
            retained = [
                line
                for line in lock.read_text(encoding="utf-8").splitlines()
                if not line.endswith("  ch09-capstone-manufacturing/README.md")
            ]
            lock.write_text("\n".join(retained) + "\n", encoding="utf-8")

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Formal book source lock coverage mismatch", completed.stderr)
            self.assertIn("ch09-capstone-manufacturing/README.md", completed.stderr)

    def test_formal_source_drift_fails_closed_before_search_results(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology source drift ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            source = (
                skill
                / "references"
                / "product-trustworthiness-book"
                / "ch01-introduction"
                / "chapter.md"
            )
            source.write_text("drifted source\n", encoding="utf-8")

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Formal book source hash mismatch", completed.stderr)
            self.assertIn("current-source.sha256", completed.stderr)

    def test_vol1_guide_drift_is_caught_by_the_authoring_lock(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology vol1 guide drift ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            guide = (
                skill
                / "references"
                / "ontology-engineering-book"
                / "ch01-introduction"
                / "README.md"
            )
            guide.write_text("drifted volume one guide\n", encoding="utf-8")

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Formal book source hash mismatch", completed.stderr)
            self.assertIn("authoring-sources.sha256", completed.stderr)

    def test_vol2_guide_drift_is_caught_by_the_guide_lock(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology vol2 guide drift ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            guide = (
                skill
                / "references"
                / "product-trustworthiness-book"
                / "ch01-introduction"
                / "README.md"
            )
            guide.write_text("drifted volume two guide\n", encoding="utf-8")

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Formal book source hash mismatch", completed.stderr)
            self.assertIn("formal-search-guides.sha256", completed.stderr)

    def test_noncanonical_lock_path_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology unsafe lock path ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            lock = (
                skill
                / "references"
                / "ontology-engineering-book"
                / "authoring-sources.sha256"
            )
            lock.write_text(f"{'0' * 64}  ../escape.md\n", encoding="utf-8")

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Unsafe formal book source path", completed.stderr)

    def test_retired_vol1_handbook_lock_cannot_coexist(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology competing lock ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root)
            external = self._external_shadow(root)
            legacy = (
                skill
                / "references"
                / "ontology-engineering-book"
                / "handbook"
                / "authoring-sources.sha256"
            )
            legacy.parent.mkdir(parents=True, exist_ok=True)
            legacy.write_text("retired competing lock\n", encoding="utf-8")

            completed = self._run_search(skill, root, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("must not coexist", completed.stderr)

    def test_missing_bundled_volume_fails_even_when_external_copy_exists(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ontology missing volume ") as temporary:
            root = Path(temporary)
            skill = self._portable_install(root, include_vol2=False)
            external = self._external_shadow(root)
            cwd = root / "elsewhere"
            cwd.mkdir()

            completed = self._run_search(skill, cwd, external, "anything")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("Bundled book corpus is incomplete", completed.stderr)
            self.assertIn("product-trustworthiness-book", completed.stderr)
            self.assertIn("External roots cannot substitute", completed.stderr)


if __name__ == "__main__":
    unittest.main()
