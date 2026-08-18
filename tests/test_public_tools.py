from __future__ import annotations

import argparse
import csv
import hashlib
import importlib.util
import json
import os
import re
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


PRIVACY = load_module("check_public_privacy", ROOT / "scripts" / "check_public_privacy.py")
INIT_BOOK = load_module(
    "init_book", ROOT / "skills" / "standard-to-book" / "scripts" / "init_book.py"
)
VALIDATE_BOOK = load_module(
    "validate_book",
    ROOT / "skills" / "standard-to-book" / "scripts" / "validate_book.py",
)
ENGRAVE_ISO = load_module("engrave_iso", ROOT / "scripts" / "engrave_iso.py")


def write_register(path: Path, rows: list[list[str]]) -> None:
    header = VALIDATE_BOOK.CSV_HEADERS[path.relative_to(path.parents[1]).as_posix()]
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(header)
        writer.writerows(rows)


def replace_yaml_values(path: Path, replacements: dict[str, str]) -> None:
    lines: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        key = line.split(":", 1)[0] if ":" in line else ""
        if key in replacements:
            lines.append(f'{key}: "{replacements[key]}"')
        else:
            lines.append(line)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def make_valid_release(target: Path) -> None:
    charter = target / "book-charter.md"
    charter.write_text(
        charter.read_text(encoding="utf-8").replace("TODO", "已由测试审阅"),
        encoding="utf-8",
    )
    replace_yaml_values(
        target / "book.yaml",
        {
            "status": "release-candidate",
            "rights_status": "approved",
            "technical_review_status": "approved",
            "reader_review_status": "accepted",
        },
    )

    write_register(
        target / "sources" / "source-register.csv",
        [[
            "SRC:001", "Controlled synthetic source", "1", "test-author",
            "lawful-access-reviewed", "cleared-for-declared-use", "PRIVATE:SRC:001",
            "a" * 64, "no", "approved", "No source text in package",
        ]],
    )
    cq_rows = [
        [
            f"CQ:{index:03d}", f"Question {index}?", "reader decision", "synthetic evidence",
            "binding", f"oracle-{index}", "approved",
        ]
        for index in range(1, 11)
    ]
    write_register(target / "cqs" / "cq-register.csv", cq_rows)
    write_register(
        target / "chapters" / "chapter-register.csv",
        [[
            "CH:001", "Synthetic chapter", "Explain the decision boundary",
            ";".join(row[0] for row in cq_rows), "SRC:001", "FIG:001", "approved",
        ]],
    )
    write_register(
        target / "propositions" / "proposition-register.csv",
        [[
            "PROP:001", "CH:001", ";".join(row[0] for row in cq_rows), "SRC:001",
            "A synthetic proposition used only to test release graph integrity",
            "teaching-assumption", "Not a compliance or production conclusion",
            "All ten synthetic CQs resolve to the declared fixture", "approved",
        ]],
    )

    files = {
        "book/reader.md": "# Synthetic reader book\n",
        "figures/assets/fig-001.svg": (
            '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">'
            '<rect width="10" height="10" fill="#336699"/></svg>\n'
        ),
        "ontology/tbox.ttl": "@prefix ex: <urn:test:> . ex:Thing a ex:Class .\n",
        "ontology/abox.ttl": "@prefix ex: <urn:test:> . ex:item a ex:Thing .\n",
        "ontology/queries.rq": "SELECT ?s WHERE { ?s ?p ?o }\n",
        "ontology/constraints.shacl": "@prefix sh: <http://www.w3.org/ns/shacl#> .\n",
        "ontology/positive.ttl": "@prefix ex: <urn:test:> . ex:positive ex:ok true .\n",
        "ontology/negative.ttl": "@prefix ex: <urn:test:> . ex:negative ex:ok false .\n",
        "ontology/runner.py": "print('synthetic ontology fixture')\n",
    }
    for relative, content in files.items():
        path = target / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    (target / "skill" / "SKILL.md").write_text(
        """---
name: "release-safety"
description: "Use when testing the synthetic release-safety book package."
---

# Synthetic release Skill

## Authority boundary

Bind this exact release and answer only its registered competency questions.
Escalate every real compliance or production decision to an accountable reviewer.

## Workflow

Resolve the exact release, answer covered questions, and report every escalation boundary.
""",
        encoding="utf-8",
    )

    figure_path = target / "figures" / "assets" / "fig-001.svg"
    write_register(
        target / "figures" / "figure-register.csv",
        [[
            "FIG:001", "CH:001", "SRC:001", "What is the decision object?",
            "One synthetic object with no real enterprise identity", "author-owned synthetic input",
            "author-owned", "deterministic test SVG", digest(figure_path), "Synthetic figure",
            "A blue square representing a synthetic object", "approved",
        ]],
    )

    ontology_manifest = target / "ontology" / "package-manifest.yaml"
    ontology_manifest.write_text(
        """schema_version: "1.0"
book_slug: "{book_slug}"
namespace: "urn:ontology:release-safety"
competency_question_register: "cqs/cq-register.csv"
tbox: "ontology/tbox.ttl"
controlled_abox_or_adapter: "ontology/abox.ttl"
queries: "ontology/queries.rq"
constraints: "ontology/constraints.shacl"
positive_fixtures: "ontology/positive.ttl"
single_fault_negative_fixtures: "ontology/negative.ttl"
runner: "ontology/runner.py"
status: "passed"
""".format(book_slug=target.name),
        encoding="utf-8",
    )
    replace_yaml_values(
        target / "privacy" / "public-export.yaml",
        {"human_privacy_review": "approved"},
    )

    report_path = target / "release" / "ontology-test-report.json"
    report_path.write_text(
        json.dumps(
            {
                "schema_version": "1.0",
                "book_slug": target.name,
                "status": "passed",
                "command": "python3 ontology/runner.py",
                "tool": "synthetic-test-runner/1.0",
                "executed_at": "2026-08-16T12:00:00Z",
                "runner": "ontology/runner.py",
                "runner_sha256": digest(target / "ontology" / "runner.py"),
                "ontology_manifest_sha256": digest(ontology_manifest),
                "covered_cq_ids": [row[0] for row in cq_rows],
                "covered_proposition_ids": ["PROP:001"],
                "checks": [{"check_id": "synthetic-positive-negative", "status": "passed"}],
            },
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )

    asset_specs = [
        ("ASSET:BOOK", "book/reader.md", "reader-book", ""),
        ("ASSET:FIG", "figures/assets/fig-001.svg", "figure", "FIG:001"),
        ("ASSET:TBOX", "ontology/tbox.ttl", "ontology", ""),
        ("ASSET:ABOX", "ontology/abox.ttl", "ontology", ""),
        ("ASSET:QUERY", "ontology/queries.rq", "query", ""),
        ("ASSET:SHACL", "ontology/constraints.shacl", "constraint", ""),
        ("ASSET:POS", "ontology/positive.ttl", "fixture", ""),
        ("ASSET:NEG", "ontology/negative.ttl", "fixture", ""),
        ("ASSET:RUN", "ontology/runner.py", "script", ""),
        ("ASSET:SKILL", "skill/SKILL.md", "skill", ""),
        ("ASSET:REPORT", "release/ontology-test-report.json", "test-report", ""),
    ]
    asset_rows = []
    for asset_id, relative, role, figure_id in asset_specs:
        asset_rows.append([
            asset_id, relative, role, "CH:001", figure_id, digest(target / relative),
            "deterministic test fixture", "author-owned synthetic artifact", "author-owned",
            "no", "approved", "approved", "approved",
        ])
    write_register(target / "release" / "public-assets.csv", asset_rows)
    VALIDATE_BOOK.write_package_lock(target)


class PrivacyGateTests(unittest.TestCase):
    def test_safe_tree_passes_and_personal_path_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "safe.md").write_text("synthetic manufacturing example\n", encoding="utf-8")
            findings, checked = PRIVACY.run(root, tracked_only=False, include_ignored=True)
            self.assertEqual(checked, 1)
            self.assertEqual(findings, [])

            personal_path = "/" + "Users" + "/alice/private/source.pdf"
            (root / "unsafe.md").write_text(personal_path + "\n", encoding="utf-8")
            findings, checked = PRIVACY.run(root, tracked_only=False, include_ignored=True)
            self.assertEqual(checked, 2)
            self.assertTrue(any(item.rule == "macOS personal absolute path" for item in findings))

    def test_env_example_is_allowed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / ".env.example").write_text("PLACEHOLDER=not-a-secret\n", encoding="utf-8")
            findings, _ = PRIVACY.run(root, tracked_only=False, include_ignored=True)
            self.assertEqual(findings, [])

    def test_nul_byte_in_declared_text_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "reader.md").write_bytes(b"# Reader\n\x00binary\n")
            findings, _ = PRIVACY.run(root, tracked_only=False, include_ignored=True)
            self.assertTrue(any("NUL byte" in item.rule for item in findings))

    def test_symbolic_link_is_rejected_without_following_target(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            private_target = "/" + "Users" + "/alice/private/book.pdf"
            (root / "external-link").symlink_to(private_target)
            findings, checked = PRIVACY.run(root, tracked_only=False, include_ignored=True)
            self.assertEqual(checked, 1)
            self.assertTrue(any("symbolic link" in item.rule for item in findings))

    def test_special_filesystem_node_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fifo = root / "unmanifested-payload"
            os.mkfifo(fifo)
            findings, checked = PRIVACY.run(root, tracked_only=False, include_ignored=True)
            self.assertEqual(checked, 1)
            self.assertTrue(any("special filesystem node" in item.rule for item in findings))
            with self.assertRaises(ValueError):
                VALIDATE_BOOK.write_package_lock(root)


class BookInitializerTests(unittest.TestCase):
    def test_initializer_builds_contract_and_refuses_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="welding-quality",
                title="焊接质量工程导读",
                standard="SYNTHETIC-STD-001",
                audience="中小型制造企业工程师",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            self.assertTrue((target / "book-charter.md").is_file())
            self.assertTrue((target / "sources" / "source-register.csv").is_file())
            self.assertTrue((target / "privacy" / "public-export.yaml").is_file())
            self.assertNotIn("standard original", (target / "book-charter.md").read_text())
            self.assertEqual(VALIDATE_BOOK.run(target, "structure"), [])
            self.assertTrue(VALIDATE_BOOK.run(target, "charter"))
            self.assertTrue(VALIDATE_BOOK.run(target, "release"))
            with self.assertRaises(FileExistsError):
                INIT_BOOK.build_package(args)

    def test_structure_requires_generated_book_skill(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="skill-required",
                title="Skill Required",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            (target / "skill" / "SKILL.md").unlink()
            errors = VALIDATE_BOOK.run(target, "structure")
            self.assertTrue(any("missing required file: skill/SKILL.md" in error for error in errors))

    def test_validator_blocks_private_paths_and_false_release_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            (target / "sessions").mkdir()
            (target / "sessions" / "private-chat.txt").write_text("private\n", encoding="utf-8")
            structure_errors = VALIDATE_BOOK.run(target, "structure")
            self.assertTrue(any("sessions" in error for error in structure_errors))

            asset_path = target / "release" / "public-assets.csv"
            with asset_path.open("a", encoding="utf-8", newline="") as handle:
                writer = csv.writer(handle)
                writer.writerow(
                    [
                        "ASSET-001",
                        "../../private/restricted.pdf",
                        "bogus",
                        "unknown",
                        "",
                        "unknown",
                        "test",
                        "pending",
                        "pending",
                        "unknown",
                        "pending",
                        "pending",
                        "approved",
                    ]
                )
            release_errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("asset path escapes package" in error for error in release_errors))
            self.assertTrue(any("invalid sha256" in error for error in release_errors))
            self.assertTrue(any("rights basis is unresolved" in error for error in release_errors))

    def test_complete_release_passes_then_detects_unlisted_and_mutated_files(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)
            self.assertEqual(VALIDATE_BOOK.run(target, "release"), [])

            hidden = target / "customer-data.bin"
            hidden.write_bytes(b"synthetic but unlisted")
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("unmanifested public file: customer-data.bin" in error for error in errors))
            self.assertTrue(any("package lock is missing file: customer-data.bin" in error for error in errors))
            hidden.unlink()

            charter = target / "book-charter.md"
            charter.write_text(charter.read_text(encoding="utf-8") + "mutation\n", encoding="utf-8")
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("package lock row" in error and "sha256 mismatch" in error for error in errors))

    def test_release_rejects_fake_cross_refs_control_assets_and_binding_reuse(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)

            assets_path = target / "release" / "public-assets.csv"
            header, rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in rows:
                if row["asset_role"] == "reader-book":
                    row["relative_path"] = "README.md"
                    row["chapter_ids"] = "NO-SUCH-CHAPTER"
                    row["sha256"] = digest(target / "README.md")
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)

            manifest = target / "ontology" / "package-manifest.yaml"
            text = manifest.read_text(encoding="utf-8")
            for key in VALIDATE_BOOK.ONTOLOGY_BINDINGS:
                text = re.sub(
                    rf'(?m)^{re.escape(key)}: ".*"$',
                    f'{key}: "README.md"',
                    text,
                )
            manifest.write_text(text, encoding="utf-8")
            VALIDATE_BOOK.write_package_lock(target)

            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("unknown ID NO-SUCH-CHAPTER" in error for error in errors))
            self.assertTrue(any("package control file" in error for error in errors))
            self.assertTrue(any("reuses the" in error for error in errors))
            self.assertTrue(any("requires asset role" in error for error in errors))

    def test_figure_source_must_belong_to_its_chapter(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)

            source_path = target / "sources" / "source-register.csv"
            source_header, source_rows = VALIDATE_BOOK.csv_rows(source_path)
            source_rows.append(
                {
                    "source_id": "SRC:002",
                    "title": "Second synthetic source",
                    "edition": "1",
                    "content_owner": "test-author",
                    "rights_basis": "lawful-access-reviewed",
                    "rights_status": "cleared-for-declared-use",
                    "private_logical_id": "PRIVATE:SRC:002",
                    "sha256": "b" * 64,
                    "public_distribution": "no",
                    "technical_review": "approved",
                    "notes": "Not linked to CH:001",
                }
            )
            with source_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=source_header)
                writer.writeheader()
                writer.writerows(source_rows)

            figure_path = target / "figures" / "figure-register.csv"
            figure_header, figure_rows = VALIDATE_BOOK.csv_rows(figure_path)
            figure_rows[0]["source_ids"] = "SRC:002"
            with figure_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=figure_header)
                writer.writeheader()
                writer.writerows(figure_rows)
            VALIDATE_BOOK.write_package_lock(target)

            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(
                any("figure FIG:001 uses source SRC:002 outside chapter CH:001" in error for error in errors)
            )

    def test_initializer_rejects_unsafe_slug(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="Unsafe Name",
                title="Unsafe",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            with self.assertRaises(ValueError):
                INIT_BOOK.build_package(args)

    def test_initializer_quotes_skill_frontmatter_and_rejects_multiline_input(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="quality-guide",
                title='ISO 9001: "质量管理"',
                standard="ISO 9001",
                audience="中小制造企业",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            frontmatter = VALIDATE_BOOK.simple_frontmatter(
                (target / "skill" / "SKILL.md").read_text(encoding="utf-8")
            )
            self.assertIsNotNone(frontmatter)
            self.assertIn('ISO 9001: "质量管理"', frontmatter["description"])

            args.slug = "multiline-guide"
            args.title = "unsafe\nsecond line"
            with self.assertRaises(ValueError):
                INIT_BOOK.build_package(args)

    def test_malformed_yaml_returns_errors_instead_of_crashing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="malformed-package",
                title="Malformed Package",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            book = target / "book.yaml"
            book.write_text(
                book.read_text(encoding="utf-8").replace('status: "charter"', "status: 1"),
                encoding="utf-8",
            )
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("book.yaml status" in error for error in errors))

    def test_book_identity_and_charter_sections_are_required(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="identity-check",
                title="Identity Check",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            book = target / "book.yaml"
            filtered = [
                line
                for line in book.read_text(encoding="utf-8").splitlines()
                if not line.startswith(("slug:", "title:", "standard_family:", "audience:", "mission:"))
            ]
            book.write_text("\n".join(filtered) + "\n", encoding="utf-8")
            (target / "book-charter.md").write_text("# Minimal\n", encoding="utf-8")
            errors = VALIDATE_BOOK.run(target, "charter")
            self.assertTrue(any("book.yaml slug is missing" in error for error in errors))
            self.assertTrue(any("book.yaml title is missing" in error for error in errors))
            self.assertTrue(any("lacks required section: 目标读者" in error for error in errors))

    def test_duplicate_yaml_invalid_utf8_and_nested_private_paths_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="parser-safety",
                title="Parser Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            book = target / "book.yaml"
            book.write_text(
                'schema_version : "999"\n' + book.read_text(encoding="utf-8"),
                encoding="utf-8",
            )
            nested = target / "book" / "sessions" / "reader.md"
            nested.parent.mkdir(parents=True)
            nested.write_text("private session fixture\n", encoding="utf-8")
            errors = VALIDATE_BOOK.run(target, "structure")
            self.assertTrue(any("duplicate top-level key schema_version" in error for error in errors))
            self.assertTrue(any("book/sessions" in error for error in errors))

            book.write_bytes(b"\xff\xfe")
            errors = VALIDATE_BOOK.run(target, "structure")
            self.assertTrue(any("book.yaml: file is not readable UTF-8" in error for error in errors))

    def test_release_rejects_unsupported_schema_versions(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)
            for relative in (
                "book.yaml",
                "ontology/package-manifest.yaml",
                "privacy/public-export.yaml",
            ):
                path = target / relative
                path.write_text(
                    path.read_text(encoding="utf-8").replace(
                        'schema_version: "1.0"', 'schema_version: "999"'
                    ),
                    encoding="utf-8",
                )
            report_path = target / "release" / "ontology-test-report.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["schema_version"] = "999"
            report["ontology_manifest_sha256"] = digest(
                target / "ontology" / "package-manifest.yaml"
            )
            report_path.write_text(
                json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
            )
            assets_path = target / "release" / "public-assets.csv"
            asset_header, asset_rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in asset_rows:
                if row["asset_role"] == "test-report":
                    row["sha256"] = digest(report_path)
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=asset_header)
                writer.writeheader()
                writer.writerows(asset_rows)
            VALIDATE_BOOK.write_package_lock(target)

            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("book.yaml schema_version is unsupported" in error for error in errors))
            self.assertTrue(any("ontology schema_version is unsupported" in error for error in errors))
            self.assertTrue(any("privacy schema_version is unsupported" in error for error in errors))
            self.assertTrue(any("test report" in error and "schema_version" in error for error in errors))

    def test_release_requires_complete_privacy_policy_lists(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)
            privacy_path = target / "privacy" / "public-export.yaml"
            lines = privacy_path.read_text(encoding="utf-8").splitlines()
            filtered: list[str] = []
            dropping = False
            for line in lines:
                if line.startswith(("forbidden_package_paths:", "forbidden_public_content:")):
                    dropping = True
                    continue
                if dropping and line.startswith("  - "):
                    continue
                dropping = False
                filtered.append(line)
            privacy_path.write_text("\n".join(filtered) + "\n", encoding="utf-8")
            VALIDATE_BOOK.write_package_lock(target)
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("privacy forbidden_package_paths" in error for error in errors))
            self.assertTrue(any("privacy forbidden_public_content" in error for error in errors))

    def test_report_rejects_impossible_timestamp_and_duplicate_json_keys(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)
            report_path = target / "release" / "ontology-test-report.json"
            assets_path = target / "release" / "public-assets.csv"

            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["executed_at"] = "2026-08-16T12:00:00+05:99"
            report_path.write_text(
                json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
            )
            header, rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in rows:
                if row["asset_role"] == "test-report":
                    row["sha256"] = digest(report_path)
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)
            VALIDATE_BOOK.write_package_lock(target)
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("executed_at is not an ISO-8601 timestamp" in error for error in errors))

            duplicate_json = report_path.read_text(encoding="utf-8").replace(
                '{\n  "schema_version":', '{\n  "status": "failed",\n  "schema_version":', 1
            )
            report_path.write_text(duplicate_json, encoding="utf-8")
            header, rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in rows:
                if row["asset_role"] == "test-report":
                    row["sha256"] = digest(report_path)
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)
            VALIDATE_BOOK.write_package_lock(target)
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("not valid UTF-8 JSON" in error for error in errors))

            report_path.write_text("[" * 20_000 + "0" + "]" * 20_000, encoding="utf-8")
            header, rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in rows:
                if row["asset_role"] == "test-report":
                    row["sha256"] = digest(report_path)
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)
            VALIDATE_BOOK.write_package_lock(target)
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("not valid UTF-8 JSON" in error for error in errors))

            nonstandard_json = report_path.read_text(encoding="utf-8").replace(
                '"status": "failed",', '"nonstandard": NaN,', 1
            )
            report_path.write_text(nonstandard_json, encoding="utf-8")
            header, rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in rows:
                if row["asset_role"] == "test-report":
                    row["sha256"] = digest(report_path)
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)
            VALIDATE_BOOK.write_package_lock(target)
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("not valid UTF-8 JSON" in error for error in errors))

    def test_book_release_uses_full_privacy_rule_set(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)
            reader = target / "book" / "reader.md"
            windows_path = "C:" + "\\Users\\Alice\\secret\\source.pdf"
            aws_key = "AKIA" + "ABCDEFGHIJKLMNOP"
            clipboard_path = "/var/" + "folders/ab/" + "codex-" + "clipboard-secret"
            reader.write_text(
                "\n".join((windows_path, aws_key, clipboard_path, "\x00binary")) + "\n",
                encoding="utf-8",
            )
            assets_path = target / "release" / "public-assets.csv"
            header, rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in rows:
                if row["asset_role"] == "reader-book":
                    row["sha256"] = digest(reader)
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)
            VALIDATE_BOOK.write_package_lock(target)

            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("personal Windows path" in error for error in errors))
            self.assertTrue(any("AWS access key" in error for error in errors))
            self.assertTrue(any("clipboard attachment identifier" in error for error in errors))
            self.assertTrue(any("invalid control character in text public candidate" in error for error in errors))

    def test_only_canonical_skill_can_satisfy_release(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)
            canonical = target / "skill" / "SKILL.md"
            alternate = target / "alternate" / "SKILL.md"
            alternate.parent.mkdir(parents=True)
            alternate.write_bytes(canonical.read_bytes())

            assets_path = target / "release" / "public-assets.csv"
            header, rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in rows:
                if row["relative_path"] == "skill/SKILL.md":
                    row["asset_role"] = "metadata"
            rows.append(
                {
                    "asset_id": "ASSET:ALT-SKILL",
                    "relative_path": "alternate/SKILL.md",
                    "asset_role": "skill",
                    "chapter_ids": "CH:001",
                    "figure_id": "",
                    "sha256": digest(alternate),
                    "creator_or_method": "deterministic test fixture",
                    "rights_basis": "author-owned synthetic artifact",
                    "rights_status": "author-owned",
                    "contains_personal_data": "no",
                    "technical_review": "approved",
                    "privacy_review": "approved",
                    "release_status": "approved",
                }
            )
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)
            VALIDATE_BOOK.write_package_lock(target)
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("canonical skill/SKILL.md" in error for error in errors))

    def test_missing_ontology_manifest_returns_errors_instead_of_crashing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)
            (target / "ontology" / "package-manifest.yaml").unlink()
            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(
                any("missing required file: ontology/package-manifest.yaml" in error for error in errors)
            )

    def test_release_rejects_semantic_placeholders_and_malformed_skill_yaml(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                slug="release-safety",
                title="Release Safety",
                standard="SYNTHETIC",
                audience="test",
                output=Path(temporary),
            )
            target = INIT_BOOK.build_package(args)
            make_valid_release(target)

            proposition_path = target / "propositions" / "proposition-register.csv"
            header, rows = VALIDATE_BOOK.csv_rows(proposition_path)
            rows[0]["authority_limit"] = "TODO: decide later"
            with proposition_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)

            skill_path = target / "skill" / "SKILL.md"
            skill_path.write_text(
                """---
name: "release-safety"
description: invalid: yaml
---

# malformed
""",
                encoding="utf-8",
            )
            assets_path = target / "release" / "public-assets.csv"
            asset_header, asset_rows = VALIDATE_BOOK.csv_rows(assets_path)
            for row in asset_rows:
                if row["asset_role"] == "skill":
                    row["sha256"] = digest(skill_path)
            with assets_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=asset_header)
                writer.writeheader()
                writer.writerows(asset_rows)
            VALIDATE_BOOK.write_package_lock(target)

            errors = VALIDATE_BOOK.run(target, "release")
            self.assertTrue(any("unresolved placeholder in authority_limit" in error for error in errors))
            self.assertTrue(any("unfinished or lacks frontmatter" in error for error in errors))


class EngravingSafetyTests(unittest.TestCase):
    def test_missing_controlled_source_fails_before_write(self) -> None:
        output = ROOT / "references" / "iso-normative-ontology" / "part1-vocabulary.ttl"
        before = hashlib.sha256(output.read_bytes()).hexdigest()
        previous = ENGRAVE_ISO.SRC
        ENGRAVE_ISO.SRC = None
        try:
            with self.assertRaises(SystemExit):
                ENGRAVE_ISO.engrave_part1()
        finally:
            ENGRAVE_ISO.SRC = previous
        after = hashlib.sha256(output.read_bytes()).hexdigest()
        self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
