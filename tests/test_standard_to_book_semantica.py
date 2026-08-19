from __future__ import annotations

import argparse
import csv
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "skills" / "standard-to-book" / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import init_book as INIT  # noqa: E402
import validate_book as VALIDATE  # noqa: E402


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_hash(value: object) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def write_csv(path: Path, header: list[str], rows: list[list[str]]) -> None:
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(header)
        writer.writerows(rows)


def replace_yaml_values(path: Path, replacements: dict[str, str]) -> None:
    lines: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        key = line.split(":", 1)[0] if ":" in line else ""
        if key in replacements:
            lines.append(f'{key}: {json.dumps(replacements[key], ensure_ascii=False)}')
        else:
            lines.append(line)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def build_candidate(parent: Path, slug: str = "welding-quality") -> Path:
    return INIT.build_package(
        argparse.Namespace(
            slug=slug,
            title="焊接质量工程导读",
            standard="SYNTHETIC-STD",
            audience="中小型制造企业工程师",
            output=parent,
        )
    )


def make_complete_release(target: Path) -> None:
    cq_ids = [f"BOOK-CQ-{index:03d}" for index in range(1, 11)]
    charter = target / "book-charter.md"
    charter_text = charter.read_text(encoding="utf-8").replace("TODO", "已由测试审阅")
    charter_text += "\n" + "\n".join(
        f"- 读者问题 {index} 应如何回答？" for index in range(1, 11)
    ) + "\n"
    charter.write_text(charter_text, encoding="utf-8")
    replace_yaml_values(
        target / "book.yaml",
        {
            "status": "release-candidate",
            "rights_status": "approved",
            "technical_review_status": "approved",
            "reader_review_status": "accepted",
        },
    )
    write_csv(
        target / "sources" / "source-register.csv",
        VALIDATE.CSV_HEADERS["sources/source-register.csv"],
        [[
            "SRC:001", "Controlled synthetic source", "1", "test-author",
            "lawful-access-reviewed", "cleared-for-declared-use", "PRIVATE:SRC:001",
            "a" * 64, "no", "approved", "No source text in package",
        ]],
    )
    write_csv(
        target / "chapters" / "chapter-register.csv",
        VALIDATE.CSV_HEADERS["chapters/chapter-register.csv"],
        [[
            "CH:001", "Synthetic chapter", "Explain a decision boundary",
            ";".join(cq_ids), "SRC:001", "FIG:001", "approved",
        ]],
    )
    write_csv(
        target / "propositions" / "proposition-register.csv",
        VALIDATE.CSV_HEADERS["propositions/proposition-register.csv"],
        [[
            "PROP:001", "CH:001", ";".join(cq_ids), "SRC:001",
            "A synthetic proposition for release-integrity testing",
            "teaching-assumption", "Not a compliance conclusion",
            "The bound Semantica CQ report is passed", "approved",
        ]],
    )

    proposal = target / "semantica" / "package-proposal.yaml"
    proposal.write_text(
        proposal.read_text(encoding="utf-8").replace(
            'proposal_status: "draft"', 'proposal_status: "accepted"'
        ),
        encoding="utf-8",
    )
    binding = target / "semantica" / "package-binding.yaml"
    binding_text = binding.read_text(encoding="utf-8")
    binding_text = binding_text.replace(
        'semantica_package_version: "unbound"',
        'semantica_package_version: "1.0.0"',
    ).replace('binding_status: "proposed"', 'binding_status: "bound"')
    binding_text = binding_text.replace(
        "bound_cq_ids:\n",
        "bound_cq_ids:\n" + "".join(
            f"  - {json.dumps(cq_id)}\n" for cq_id in cq_ids
        ),
    )
    binding.write_text(binding_text, encoding="utf-8")

    book_path = target / "book" / "reader.md"
    book_path.parent.mkdir(parents=True)
    book_path.write_text("# Synthetic reader book\n", encoding="utf-8")
    figure_path = target / "figures" / "assets" / "fig-001.svg"
    figure_path.parent.mkdir(parents=True)
    figure_path.write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">'
        '<rect width="10" height="10" fill="#336699"/></svg>\n',
        encoding="utf-8",
    )
    write_csv(
        target / "figures" / "figure-register.csv",
        VALIDATE.CSV_HEADERS["figures/figure-register.csv"],
        [[
            "FIG:001", "CH:001", "SRC:001", "What is the decision object?",
            "One synthetic object", "author-owned synthetic input", "author-owned",
            "deterministic SVG", digest(figure_path), "Synthetic figure",
            "A blue synthetic square", "approved",
        ]],
    )
    (target / "skill" / "SKILL.md").write_text(
        """---
name: "welding-quality"
description: "Route the reviewed welding-quality book to its bound Semantica package."
---

# Welding quality book Skill

## Authority boundary

Answer only the bound questions and escalate real compliance decisions.

## Workflow

Bind this release, route through ontology_engineering.semantica_runtime, and report gaps.
""",
        encoding="utf-8",
    )
    replace_yaml_values(
        target / "privacy" / "public-export.yaml",
        {"human_privacy_review": "approved"},
    )

    package_id = "semantica.books.welding_quality"
    package_digest = "b" * 64
    runtime_commit = "c" * 40
    artifact_digest = "d" * 64
    timestamp = "2026-08-19T12:00:00Z"
    source_lock = {
        "$schema": "ontology-engineering.book-semantica-source-lock/v1",
        "book_slug": "welding-quality",
        "package_id": package_id,
        "package_version": "1.0.0",
        "package_digest": package_digest,
        "runtime_version": "0.6.5+oe.1",
        "runtime_commit": runtime_commit,
        "runtime_artifact_sha256": artifact_digest,
        "source_register_sha256": digest(target / "sources" / "source-register.csv"),
        "chapter_register_sha256": digest(target / "chapters" / "chapter-register.csv"),
        "proposition_register_sha256": digest(
            target / "propositions" / "proposition-register.csv"
        ),
        "source_hashes": {"SRC:001": "a" * 64},
        "created_at": timestamp,
    }
    source_lock_path = target / "release" / "semantica-source-lock.json"
    source_lock_path.write_text(
        json.dumps(source_lock, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    def report(kind: str, payload: object) -> dict[str, object]:
        content = {"kind": kind, "status": "passed", "payload": payload}
        return {**content, "sha256": canonical_hash(content)}

    reports = {
        "capability_report": report("capability", {"available": True}),
        "cq_report": report(
            "cq", {"competency_question_ids": cq_ids, "status": "passed"}
        ),
        "shacl_report": report("shacl", {"conforms": True}),
        "oracle_report": report("oracle", {"passed": True}),
    }
    provenance_content = {
        "schema_version": "1.0",
        "bundle_id": "urn:test:provenance",
        "generated_at": timestamp,
        "bindings": {"package_id": package_id},
        "records": [{"entity_id": "urn:test:record", "sequence_id": 1}],
    }
    provenance = {
        **provenance_content,
        "bundle_sha256": canonical_hash(provenance_content),
    }
    receipt_content = {
        "schema_version": "1.0",
        "created_at": timestamp,
        "runtime_version": "0.6.5+oe.1",
        "runtime_commit": runtime_commit,
        "runtime_artifact_sha256": artifact_digest,
        "package_id": package_id,
        "package_version": "1.0.0",
        "package_digest": package_digest,
        "asset_hashes": {"manifest.yaml": "e" * 64},
        "chapter_contract_sha256": "f" * 64,
        "dataset_sha256": "1" * 64,
        "dataset_quad_count": 1,
        "dataset_revision": 1,
        **reports,
        "output_hashes": {},
        "provenance_bundle": provenance,
    }
    receipt = {
        **receipt_content,
        "receipt_sha256": canonical_hash(receipt_content),
    }
    receipt_path = target / "release" / "semantica-runtime-receipt.json"
    receipt_path.write_text(
        json.dumps(receipt, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    verdict = {
        "schema_version": "1.0",
        "status": "complete",
        "receipt_sha256": receipt["receipt_sha256"],
        "checked_at": timestamp,
        "checks": [
            {"check_id": "receipt.integrity", "passed": True, "message": "verified"}
        ],
        "reasons": [],
    }
    verdict_path = target / "release" / "semantica-release-verdict.json"
    verdict_path.write_text(
        json.dumps(verdict, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    specs = [
        ("ASSET:BOOK", "book/reader.md", "reader-book", ""),
        ("ASSET:FIG", "figures/assets/fig-001.svg", "figure", "FIG:001"),
        ("ASSET:SKILL", "skill/SKILL.md", "skill", ""),
        (
            "ASSET:LOCK", "release/semantica-source-lock.json", "source-lock", "",
        ),
        (
            "ASSET:RECEIPT", "release/semantica-runtime-receipt.json",
            "runtime-receipt", "",
        ),
        (
            "ASSET:VERDICT", "release/semantica-release-verdict.json",
            "release-verdict", "",
        ),
    ]
    asset_rows = [
        [
            asset_id, relative, role, "CH:001", figure_id, digest(target / relative),
            "deterministic test fixture", "author-owned synthetic artifact",
            "author-owned", "no", "approved", "approved", "approved",
        ]
        for asset_id, relative, role, figure_id in specs
    ]
    write_csv(
        target / "release" / "public-assets.csv",
        VALIDATE.CSV_HEADERS["release/public-assets.csv"],
        asset_rows,
    )
    VALIDATE.write_package_lock(target)


class SemanticaOnlyBookTests(unittest.TestCase):
    def test_initializer_creates_only_book_corpus_and_binding_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = build_candidate(Path(temporary))
            self.assertEqual(VALIDATE.run(target, "structure"), [])
            self.assertTrue((target / "semantica" / "package-proposal.yaml").is_file())
            self.assertTrue((target / "semantica" / "package-binding.yaml").is_file())
            for forbidden in ("cqs", "ontology", "cases", "queries", "shapes", "rules"):
                self.assertFalse((target / forbidden).exists())
            suffixes = {path.suffix for path in target.rglob("*") if path.is_file()}
            self.assertTrue(
                suffixes.isdisjoint({".ttl", ".owl", ".rdf", ".rq", ".sparql", ".shacl"})
            )

    def test_structure_rejects_a_book_local_semantic_payload(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = build_candidate(Path(temporary))
            payload = target / "book" / "ontology" / "shadow.ttl"
            payload.parent.mkdir(parents=True)
            payload.write_text("shadow semantic payload\n", encoding="utf-8")
            errors = VALIDATE.run(target, "structure")
            self.assertTrue(any("parallel semantic root is forbidden" in item for item in errors))
            self.assertTrue(any("executable semantic artifact is forbidden" in item for item in errors))

    def test_release_fails_closed_without_native_semantica_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = build_candidate(Path(temporary))
            errors = VALIDATE.run(target, "release")
            self.assertTrue(any("package proposal is not accepted" in item for item in errors))
            self.assertTrue(any("package binding is not bound" in item for item in errors))
            self.assertTrue(any("exactly one Semantica source-lock" in item for item in errors))
            self.assertTrue(any("exactly one Semantica runtime-receipt" in item for item in errors))
            self.assertTrue(any("exactly one Semantica release-verdict" in item for item in errors))

    def test_complete_native_binding_passes_and_tampering_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = build_candidate(Path(temporary))
            make_complete_release(target)
            self.assertEqual(VALIDATE.run(target, "release"), [])

            receipt_path = target / "release" / "semantica-runtime-receipt.json"
            receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
            receipt["dataset_revision"] = 2
            receipt_path.write_text(
                json.dumps(receipt, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
            )
            asset_path = target / "release" / "public-assets.csv"
            header, rows = VALIDATE.csv_rows(asset_path)
            for row in rows:
                if row["asset_role"] == "runtime-receipt":
                    row["sha256"] = digest(receipt_path)
            with asset_path.open("w", encoding="utf-8", newline="") as handle:
                writer = csv.DictWriter(handle, fieldnames=header)
                writer.writeheader()
                writer.writerows(rows)
            VALIDATE.write_package_lock(target)
            errors = VALIDATE.run(target, "release")
            self.assertTrue(any("receipt content hash does not verify" in item for item in errors))

    def test_validator_never_imports_semantica_directly(self) -> None:
        for path in (SCRIPTS / "init_book.py", SCRIPTS / "validate_book.py"):
            text = path.read_text(encoding="utf-8")
            self.assertNotIn("from semantica", text)
            self.assertNotIn("import semantica", text)


if __name__ == "__main__":
    unittest.main()
