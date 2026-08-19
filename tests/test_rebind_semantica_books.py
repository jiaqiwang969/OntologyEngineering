import hashlib
import json
from pathlib import Path

from scripts.rebind_semantica_books import VOL1_GUIDE_DIRS, rebind


def _write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def _sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _make_fixture(tmp_path: Path) -> tuple[Path, Path]:
    book_root = tmp_path / "ontology-engineering"
    semantica_root = tmp_path / "semantica"
    package_root = semantica_root / "semantica" / "chapter_packages"
    records = []
    migration = {"schema_version": "1.0", "mappings": []}

    for volume, count in (("vol1", 9), ("vol2", 20)):
        for number in range(1, count + 1):
            chapter = f"ch{number:02d}"
            package_id = f"semantica.chapter_packages.{volume}.{chapter}"
            relative_manifest = f"{volume}/{chapter}/manifest.yaml"
            package_dir = package_root / volume / chapter
            package_dir.mkdir(parents=True, exist_ok=True)
            (package_dir / "contract.yaml").write_text(
                json.dumps(
                    {
                        "external_specification": {
                            "kind": "book",
                            "stone": True,
                            "source_anchor": f"old/{volume}/{chapter}.md",
                            "source_sha256": "a" * 64,
                        }
                    },
                    indent=2,
                )
                + "\n",
                encoding="utf-8",
            )
            (package_dir / "ontology.ttl").write_text(
                "non-semantic rebind fixture\n", encoding="utf-8"
            )
            assets = [
                {
                    "asset_id": "chapter-contract",
                    "role": "chapter_contract",
                    "path": "contract.yaml",
                    "sha256": "b" * 64,
                    "source_anchor": f"historical/{volume}/{chapter}.yaml",
                    "source_sha256": "c" * 64,
                },
                {
                    "asset_id": "ontology",
                    "role": "ontology",
                    "path": "ontology.ttl",
                    "sha256": _sha(package_dir / "ontology.ttl"),
                    "source_anchor": f"old/{volume}/{chapter}.md",
                    "source_sha256": "a" * 64,
                },
            ]
            if volume == "vol1":
                tex_anchor = (
                    "references/ontology-engineering-book/handbook/chapters/"
                    f"{chapter}.tex"
                )
                source_path = book_root / tex_anchor
                source_path.parent.mkdir(parents=True, exist_ok=True)
                source_path.write_text(f"volume one {chapter}\n", encoding="utf-8")
                guide_path = (
                    book_root
                    / "references"
                    / "ontology-engineering-book"
                    / VOL1_GUIDE_DIRS[chapter]
                    / "README.md"
                )
                guide_path.parent.mkdir(parents=True, exist_ok=True)
                guide_path.write_text(f"guide {chapter}\n", encoding="utf-8")
                manifest = {
                    "chapter": {
                        "external_specification": {
                            "source_anchor": f"old/{volume}/{chapter}.md",
                            "source_sha256": "a" * 64,
                        }
                    },
                    "assets": assets,
                }
            else:
                chapter_dir = book_root / "references" / "product-trustworthiness-book" / chapter
                chapter_dir.mkdir(parents=True, exist_ok=True)
                chapter_path = chapter_dir / "chapter.md"
                chapter_path.write_text(f"volume two {chapter}\n", encoding="utf-8")
                (chapter_dir / "README.md").write_text(
                    f"guide {chapter}\n", encoding="utf-8"
                )
                tex_path = (
                    book_root
                    / "references"
                    / "product-trustworthiness-book"
                    / "handbook"
                    / "fragments"
                    / f"readme-{chapter}.tex"
                )
                tex_path.parent.mkdir(parents=True, exist_ok=True)
                tex_path.write_text(f"generated {chapter}\n", encoding="utf-8")
                source_anchor = (
                    "ontology-engineering/references/product-trustworthiness-book/"
                    f"{chapter}/chapter.md"
                )
                manifest = {
                    "book_source": {
                        "logical_anchor": source_anchor,
                        "sha256": "a" * 64,
                    },
                    "assets": assets,
                }
                manifest["assets"][1]["source_anchor"] = source_anchor
                migration["mappings"].append(
                    {
                        "new_package": package_id,
                        "new_asset_id": "chapter-contract",
                        "new_sha256": "b" * 64,
                    }
                )
            _write_json(package_root / relative_manifest, manifest)
            records.append(
                {
                    "volume": volume,
                    "chapter": chapter,
                    "package_id": package_id,
                    "manifest": relative_manifest,
                }
            )

    _write_json(package_root / "registry.yaml", {"packages": records})
    _write_json(package_root / "vol2" / "migration-map.json", migration)
    return book_root, semantica_root


def test_rebind_is_explicit_reproducible_and_updates_contract_migration_hashes(
    tmp_path: Path,
) -> None:
    book_root, semantica_root = _make_fixture(tmp_path)

    dry = rebind(book_root=book_root, semantica_root=semantica_root, write=False)
    assert dry["package_count"] == 29
    assert dry["error_count"] == 0
    assert dry["changed_count"] == 59
    assert dry["passed"] is False

    written = rebind(book_root=book_root, semantica_root=semantica_root, write=True)
    assert written["passed"] is True
    clean = rebind(book_root=book_root, semantica_root=semantica_root, write=False)
    assert clean["passed"] is True
    assert clean["changed_count"] == 0

    package_root = semantica_root / "semantica" / "chapter_packages"
    manifest = json.loads(
        (package_root / "vol2" / "ch01" / "manifest.yaml").read_text()
    )
    contract_path = package_root / "vol2" / "ch01" / "contract.yaml"
    contract = json.loads(contract_path.read_text())
    migration = json.loads(
        (package_root / "vol2" / "migration-map.json").read_text()
    )
    expected_contract_hash = _sha(contract_path)
    assert manifest["book_source"]["tex_anchor"].endswith(
        "fragments/readme-ch01.tex"
    )
    assert manifest["book_source"]["guide_anchor"].endswith("ch01/README.md")
    assert contract["external_specification"]["tex_sha256"]
    assert manifest["assets"][0]["sha256"] == expected_contract_hash
    assert migration["mappings"][0]["new_sha256"] == expected_contract_hash


def test_intentional_book_edit_is_detected_until_rebound(tmp_path: Path) -> None:
    book_root, semantica_root = _make_fixture(tmp_path)
    rebind(book_root=book_root, semantica_root=semantica_root, write=True)
    chapter = (
        book_root
        / "references"
        / "product-trustworthiness-book"
        / "ch01"
        / "chapter.md"
    )
    chapter.write_text("revised\n", encoding="utf-8")

    report = rebind(book_root=book_root, semantica_root=semantica_root, write=False)

    assert report["passed"] is False
    assert any(path.endswith("vol2/ch01/manifest.yaml") for path in report["changed_files"])
