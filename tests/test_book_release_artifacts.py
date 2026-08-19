from __future__ import annotations

import base64
import csv
import hashlib
import io
import json
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

from scripts import book_release_artifacts as release


ROOT = Path(__file__).resolve().parents[1]


class BookReleaseArtifactTests(unittest.TestCase):
    def test_direct_script_bootstraps_repository_imports(self) -> None:
        script = ROOT / "scripts" / "book_release_artifacts.py"
        code = (
            "import runpy; from pathlib import Path; "
            f"ns=runpy.run_path({str(script)!r}); "
            f"report=ns['_privacy_report'](Path({str(ROOT)!r})); "
            "assert report['scope'] == 'tracked-and-unignored-worktree'"
        )
        with tempfile.TemporaryDirectory() as temporary:
            completed = subprocess.run(
                [sys.executable, "-c", code],
                cwd=temporary,
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertEqual(0, completed.returncode, completed.stderr)

    def _rewrite_wheel(self, source: Path, target: Path, mutate: object) -> None:
        with zipfile.ZipFile(source) as archive:
            members = {name: archive.read(name) for name in archive.namelist()}
        record = next(name for name in members if name.endswith(".dist-info/RECORD"))
        mutate(members)  # type: ignore[operator]
        output = io.StringIO(newline="")
        writer = csv.writer(output, lineterminator="\n")
        for name in sorted(members):
            if name == record:
                writer.writerow((name, "", ""))
                continue
            raw = members[name]
            digest = base64.urlsafe_b64encode(hashlib.sha256(raw).digest()).rstrip(b"=")
            writer.writerow((name, "sha256=" + digest.decode("ascii"), len(raw)))
        members[record] = output.getvalue().encode("utf-8")
        with zipfile.ZipFile(target, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for name, raw in members.items():
                archive.writestr(name, raw)

    def test_locked_wheel_inventory_binds_all_29_packages_and_assets(self) -> None:
        lock = json.loads((ROOT / release.SOURCE_LOCK_PATH).read_text(encoding="utf-8"))
        wheel = ROOT / "runtime" / "vendor" / lock["artifact"]["filename"]
        packages = release.package_inventory(wheel)
        self.assertEqual(29, len(packages))
        self.assertGreater(sum(len(item["assets"]) for item in packages), 300)
        self.assertEqual(
            sorted(item["package_id"] for item in packages),
            [item["package_id"] for item in packages],
        )

    def test_canonical_json_rejects_format_drift_and_duplicate_keys(self) -> None:
        with self.assertRaises(release.ReleaseArtifactError):
            release._load_json_bytes(b'{"b":2, "a":1}\n', label="test", canonical=True)
        with self.assertRaises(release.ReleaseArtifactError):
            release._load_json_bytes(b'{"a":1,"a":2}\n', label="test", canonical=False)
        for hostile in (b'{"x":NaN}\n', b'{"x":Infinity}\n', b'{"x":-Infinity}\n'):
            with self.subTest(hostile=hostile):
                with self.assertRaises(release.ReleaseArtifactError):
                    release._load_json_bytes(hostile, label="test", canonical=False)
                with self.assertRaises(release.ReleaseArtifactError):
                    release._load_json_bytes(hostile, label="test", canonical=True)
        value = {"b": 2, "a": 1}
        self.assertEqual(
            value,
            release._load_json_bytes(
                release.canonical_bytes(value), label="test", canonical=True
            ),
        )

    def test_artifact_reference_rejects_drift_and_path_escape(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "evidence.json"
            artifact.write_text("{}\n", encoding="utf-8")
            reference = release.artifact_ref(root, "evidence.json")
            self.assertEqual(
                artifact,
                release.verify_artifact_ref(root, reference, label="evidence"),
            )
            artifact.write_text('{"changed":true}\n', encoding="utf-8")
            with self.assertRaises(release.ReleaseArtifactError):
                release.verify_artifact_ref(root, reference, label="evidence")
            with self.assertRaises(release.ReleaseArtifactError):
                release._safe_file(root, "../outside", label="hostile")

    def test_pending_authority_and_blocked_package_force_candidate(self) -> None:
        manifest = {
            "governance": {
                "publication_approval": {"status": "pending"},
                "rights": {"status": "pending"},
            },
            "packages": [
                {
                    "execution": {
                        "receipt": None,
                        "regression_gate": "not_run",
                        "release_gate": "blocked",
                    },
                    "package_id": "example.package",
                    "release_status": "blocked",
                    "status": "partial",
                }
            ],
            "verification": {"runtime_identity": {"status": "passed"}},
        }
        blockers = release.compute_release_blockers(manifest)
        self.assertIn("governance.rights", blockers)
        self.assertIn("governance.publication_approval", blockers)
        self.assertIn("package.example.package", blockers)

    def test_cli_create_write_verify_roundtrip_stays_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            semantica_root = root / "semantica-checkout"
            semantica_root.mkdir()

            def write_json(relative: str, document: object) -> None:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(release.canonical_bytes(document))

            wheel_relative = "runtime/vendor/semantica-roundtrip.whl"
            wheel = root / wheel_relative
            wheel.parent.mkdir(parents=True)
            wheel.write_bytes(b"roundtrip wheel fixture\n")
            source_commit = "1" * 40
            source_tree = "2" * 40
            write_json(
                release.SOURCE_LOCK_PATH,
                {
                    "$schema": release.SOURCE_LOCK_SCHEMA,
                    "artifact": {
                        "filename": wheel.name,
                        "sha256": release.sha256_file(wheel),
                    },
                    "source": {
                        "canonical_repository": (
                            "https://github.com/jiaqiwang969/semantica.git"
                        ),
                        "commit": "3" * 40,
                        "version": "0.0+roundtrip",
                    },
                },
            )

            evidence_documents = {
                "authoring_locks": {"passed": True},
                "book_source_bindings": {"passed": True, "status": "passed"},
                "ontology_engineering_regression": {"passed": True},
                "pdf_qa": {"passed": True},
                "privacy": {"ok": True},
                "runtime_identity": {"ok": True},
                "semantica_regression": {"passed": True},
            }
            for kind, relative in release.EVIDENCE_PATHS.items():
                write_json(relative, evidence_documents[kind])

            source_lock_reference = release.artifact_ref(root, release.SOURCE_LOCK_PATH)
            write_json(
                release.GOVERNANCE_PATHS["rights"],
                {
                    "$schema": "ontology-engineering.rights-evidence/v1",
                    "reason": "A rights authority has not approved publication.",
                    "required_authority": "rights-holder",
                    "sources": [source_lock_reference],
                    "status": "pending",
                },
            )
            rights_digest = release.sha256_file(
                root / release.GOVERNANCE_PATHS["rights"]
            )
            write_json(
                release.GOVERNANCE_PATHS["publication_approval"],
                {
                    "$schema": "ontology-engineering.publication-approval/v1",
                    "reason": "Publication approval has not been granted.",
                    "required_authority": "publisher",
                    "rights_evidence_sha256": rights_digest,
                    "scope": "two-book-artifact-publication",
                    "status": "blocked",
                },
            )

            lock_paths = {
                "vol1": [
                    "references/ontology-engineering-book/authoring-sources.sha256"
                ],
                "vol2": [
                    "references/product-trustworthiness-book/handbook/current-source.sha256",
                    "references/product-trustworthiness-book/handbook/authoring-assets.sha256",
                    "references/product-trustworthiness-book/handbook/formal-search-guides.sha256",
                ],
            }
            for paths in lock_paths.values():
                for relative in paths:
                    path = root / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("fixture lock\n", encoding="utf-8")
            for spec in release.BOOK_SPECS.values():
                pdf = root / spec["pdf"]
                pdf.parent.mkdir(parents=True, exist_ok=True)
                pdf.write_bytes(b"%PDF-1.4\nroundtrip fixture\n%%EOF\n")

            package_inventory = [
                {
                    "assets": [],
                    "chapter": "ch01",
                    "manifest": {
                        "member": (
                            "semantica/chapter_packages/vol1/ch01/manifest.yaml"
                        ),
                        "sha256": "4" * 64,
                    },
                    "package_id": "semantica.chapter_packages.vol1.ch01",
                    "release_status": "complete",
                    "status": "complete",
                    "version": "0.0+roundtrip",
                    "volume": "vol1",
                }
            ]

            with (
                mock.patch.object(
                    release, "_git_source_boundary", return_value=source_tree
                ),
                mock.patch.object(release, "_verify_semantica_checkout"),
                mock.patch.object(release, "_verify_technical_evidence"),
                mock.patch.object(release, "_pdf_pages", return_value=7),
                mock.patch.object(
                    release, "package_inventory", return_value=package_inventory
                ),
            ):
                create_output = io.StringIO()
                with mock.patch("sys.stdout", create_output):
                    create_code = release.main(
                        [
                            "--root",
                            str(root),
                            "create",
                            "--oe-source-commit",
                            source_commit,
                            "--semantica-root",
                            str(semantica_root),
                        ]
                    )
                create_report = json.loads(create_output.getvalue())

                manifest_path = root / release.MANIFEST_PATH
                sidecar_path = root / f"{release.MANIFEST_PATH}.sha256"
                self.assertEqual(0, create_code)
                self.assertTrue(manifest_path.is_file())
                self.assertTrue(sidecar_path.is_file())
                self.assertEqual(
                    f"{release.sha256_file(manifest_path)}  {manifest_path.name}\n",
                    sidecar_path.read_text(encoding="ascii"),
                )
                stored_manifest = release._load_json_bytes(
                    manifest_path.read_bytes(),
                    label="roundtrip manifest",
                    canonical=True,
                )
                self.assertEqual("candidate", stored_manifest["artifact_status"])
                self.assertEqual(
                    {"publication_approval": "blocked", "rights": "pending"},
                    {
                        kind: item["status"]
                        for kind, item in stored_manifest["governance"].items()
                    },
                )
                self.assertTrue(create_report["passed"])
                self.assertEqual("candidate", create_report["artifact_status"])
                self.assertFalse(create_report["release_eligible"])
                self.assertEqual(
                    ["governance.publication_approval", "governance.rights"],
                    create_report["release_blockers"],
                )

                verify_output = io.StringIO()
                with mock.patch("sys.stdout", verify_output):
                    verify_code = release.main(
                        [
                            "--root",
                            str(root),
                            "verify",
                            "--semantica-root",
                            str(semantica_root),
                        ]
                    )
                verify_report = json.loads(verify_output.getvalue())

            self.assertEqual(0, verify_code)
            self.assertTrue(verify_report["passed"])
            self.assertEqual("candidate", verify_report["artifact_status"])
            self.assertFalse(verify_report["release_eligible"])
            self.assertEqual(create_report, verify_report)

    def test_package_readiness_comes_only_from_bound_wheel_metadata(self) -> None:
        base = {
            "governance": {
                "publication_approval": {"status": "approved"},
                "rights": {"status": "approved"},
            },
            "packages": [
                {
                    "package_id": "example.package",
                    "release_status": "complete",
                    "status": "complete",
                }
            ],
            "verification": {"all": {"status": "passed"}},
        }
        self.assertEqual(
            ["governance.publication_approval", "governance.rights"],
            release.compute_release_blockers(base),
        )
        base["packages"][0]["status"] = "partial"
        base["packages"][0]["execution"] = {
            "receipt": {"path": "forged.json"},
            "regression_gate": "passed",
            "release_gate": "passed",
        }
        self.assertEqual(
            [
                "governance.publication_approval",
                "governance.rights",
                "package.example.package",
            ],
            release.compute_release_blockers(base),
        )

    def test_unsigned_governance_cannot_self_approve(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            document = {
                "$schema": "ontology-engineering.rights-evidence/v1",
                "reason": "self assertion",
                "required_authority": "publisher",
                "sources": [],
                "status": "approved",
            }
            with self.assertRaises(release.ReleaseArtifactError):
                release._verify_governance_evidence(
                    root, kind="rights", document=document
                )

    def test_wheel_asset_tamper_is_detected(self) -> None:
        lock = json.loads((ROOT / release.SOURCE_LOCK_PATH).read_text(encoding="utf-8"))
        source = ROOT / "runtime" / "vendor" / lock["artifact"]["filename"]
        with tempfile.TemporaryDirectory() as temporary:
            tampered = Path(temporary) / source.name
            with (
                zipfile.ZipFile(source) as original,
                zipfile.ZipFile(
                    tampered, "w", compression=zipfile.ZIP_DEFLATED
                ) as target,
            ):
                changed = False
                for info in original.infolist():
                    data = original.read(info.filename)
                    if (
                        not changed
                        and "/chapter_packages/vol1/ch01/" in info.filename
                        and not info.filename.endswith("manifest.yaml")
                        and not info.is_dir()
                    ):
                        data += b"\n# tampered\n"
                        changed = True
                    target.writestr(info, data)
            self.assertTrue(changed)
            with self.assertRaises(release.ReleaseArtifactError):
                release.package_inventory(tampered)

    def test_wheel_rejects_recorded_but_undeclared_package_member(self) -> None:
        lock = json.loads((ROOT / release.SOURCE_LOCK_PATH).read_text(encoding="utf-8"))
        source = ROOT / "runtime" / "vendor" / lock["artifact"]["filename"]
        with tempfile.TemporaryDirectory() as temporary:
            tampered = Path(temporary) / source.name

            def mutate(members: dict[str, bytes]) -> None:
                members[
                    "semantica/chapter_packages/vol1/ch01/unmanifested-secret.txt"
                ] = b"hidden\n"

            self._rewrite_wheel(source, tampered, mutate)
            with self.assertRaises(release.ReleaseArtifactError):
                release.package_inventory(tampered)

    def test_wheel_rejects_package_id_coordinate_spoof(self) -> None:
        lock = json.loads((ROOT / release.SOURCE_LOCK_PATH).read_text(encoding="utf-8"))
        source = ROOT / "runtime" / "vendor" / lock["artifact"]["filename"]
        member = "semantica/chapter_packages/vol1/ch01/manifest.yaml"
        with tempfile.TemporaryDirectory() as temporary:
            tampered = Path(temporary) / source.name

            def mutate(members: dict[str, bytes]) -> None:
                document = json.loads(members[member])
                document["package_id"] = "semantica.chapter_packages.vol9.ch99"
                members[member] = release.canonical_bytes(document)

            self._rewrite_wheel(source, tampered, mutate)
            with self.assertRaises(release.ReleaseArtifactError):
                release.package_inventory(tampered)

    def test_wheel_rejects_duplicate_zip_member(self) -> None:
        lock = json.loads((ROOT / release.SOURCE_LOCK_PATH).read_text(encoding="utf-8"))
        source = ROOT / "runtime" / "vendor" / lock["artifact"]["filename"]
        duplicate = "semantica/chapter_packages/vol1/ch01/manifest.yaml"
        with tempfile.TemporaryDirectory() as temporary:
            tampered = Path(temporary) / source.name
            with (
                zipfile.ZipFile(source) as original,
                zipfile.ZipFile(
                    tampered, "w", compression=zipfile.ZIP_DEFLATED
                ) as target,
            ):
                for info in original.infolist():
                    target.writestr(info, original.read(info.filename))
                target.writestr(duplicate, original.read(duplicate))
            with self.assertRaises(release.ReleaseArtifactError):
                release.package_inventory(tampered)

    def test_wheel_rejects_fifo_or_other_special_member(self) -> None:
        info = zipfile.ZipInfo("semantica/fifo-marker")
        info.create_system = 3
        info.external_attr = (stat.S_IFIFO | 0o644) << 16
        with self.assertRaises(release.ReleaseArtifactError):
            release._safe_wheel_member(info)


if __name__ == "__main__":
    unittest.main()
