from __future__ import annotations

from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from ontology_engineering import semantica_runtime
from scripts import book_release_artifacts as release
from scripts import update_book_authoring_locks as authoring


class BookReleaseArtifactNegativeTests(unittest.TestCase):
    def test_v1_rejects_unsigned_self_approval_for_every_governance_gate(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "controlled-source.txt"
            source.write_text("controlled\n", encoding="utf-8")
            rights = {
                "$schema": "ontology-engineering.rights-evidence/v1",
                "reason": "unsigned self assertion",
                "required_authority": "rights-holder",
                "sources": [release.artifact_ref(root, source.name)],
                "status": "approved",
            }
            approval = {
                "$schema": "ontology-engineering.publication-approval/v1",
                "reason": "unsigned self assertion",
                "required_authority": "publisher",
                "rights_evidence_sha256": "a" * 64,
                "scope": "two-book-artifact-publication",
                "status": "approved",
            }

            with self.subTest(kind="rights"):
                with self.assertRaisesRegex(
                    release.ReleaseArtifactError, "status is invalid"
                ):
                    release._verify_governance_evidence(
                        root, kind="rights", document=rights
                    )
            with self.subTest(kind="publication_approval"):
                with self.assertRaisesRegex(
                    release.ReleaseArtifactError, "status is invalid"
                ):
                    release._verify_governance_evidence(
                        root,
                        kind="publication_approval",
                        document=approval,
                        rights_evidence_sha256="a" * 64,
                    )

            blockers = release.compute_release_blockers(
                {
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
            )
            self.assertIn("governance.rights", blockers)
            self.assertIn("governance.publication_approval", blockers)
            with self.assertRaisesRegex(
                release.ReleaseArtifactError, "v1 cannot claim release"
            ):
                release.create_manifest(
                    root,
                    oe_source_commit="a" * 40,
                    claim_release=True,
                )

    def test_package_shape_rejects_forged_execution_gate_fields(self) -> None:
        forged = {
            "assets": [],
            "chapter": "ch01",
            "execution": {
                "receipt": {"sha256": "a" * 64},
                "regression_gate": "passed",
                "release_gate": "passed",
            },
            "manifest": {"member": "manifest.yaml", "sha256": "b" * 64},
            "package_id": "example.package",
            "release_status": "complete",
            "status": "complete",
            "version": "1",
            "volume": "vol1",
        }
        with self.assertRaisesRegex(release.ReleaseArtifactError, "keys differ"):
            release._validate_package_shape(forged, label="packages[0]")

    def test_forged_green_authoring_evidence_differs_from_fresh_replay(self) -> None:
        expected = {"passed": False, "results": [{"volume": "vol1"}]}
        forged = {"passed": True, "results": [{"volume": "vol1"}]}
        with (
            tempfile.TemporaryDirectory() as temporary,
            mock.patch.object(authoring, "check_or_write", return_value=expected),
        ):
            with self.assertRaisesRegex(
                release.ReleaseArtifactError, "fresh local replay"
            ):
                release._verify_technical_evidence(
                    Path(temporary),
                    kind="authoring_locks",
                    document=forged,
                    manifest={},
                    semantica_root=None,
                    replay_regressions=False,
                )

    def test_forged_green_book_binding_differs_from_fresh_replay(self) -> None:
        expected = {"passed": False, "status": "blocked", "bindings": []}
        forged = {"passed": True, "status": "passed", "bindings": []}
        with (
            tempfile.TemporaryDirectory() as temporary,
            mock.patch.object(
                semantica_runtime,
                "verify_book_source_bindings",
                return_value=expected,
            ),
        ):
            with self.assertRaisesRegex(
                release.ReleaseArtifactError, "fresh local replay"
            ):
                release._verify_technical_evidence(
                    Path(temporary),
                    kind="book_source_bindings",
                    document=forged,
                    manifest={},
                    semantica_root=None,
                    replay_regressions=False,
                )

    def test_forged_green_runtime_identity_differs_from_fresh_replay(self) -> None:
        expected = {
            "checks": [
                {"code": "installed_record", "level": "error", "message": "bad"}
            ],
            "mode": "doctor",
            "ok": False,
        }
        forged = {
            "checks": [{"code": "installed_record", "level": "ok", "message": "good"}],
            "mode": "doctor",
            "ok": True,
        }
        with (
            tempfile.TemporaryDirectory() as temporary,
            mock.patch.object(release, "_runtime_report", return_value=expected),
        ):
            with self.assertRaisesRegex(
                release.ReleaseArtifactError, "fresh local replay"
            ):
                release._verify_technical_evidence(
                    Path(temporary),
                    kind="runtime_identity",
                    document=forged,
                    manifest={},
                    semantica_root=None,
                    replay_regressions=False,
                )

    def test_forged_green_privacy_evidence_differs_from_fresh_replay(self) -> None:
        expected = {
            "findings": [{"code": "private-path", "path": "source.txt"}],
            "ok": False,
            "scope": "tracked-and-unignored-worktree",
        }
        forged = {
            "findings": [],
            "ok": True,
            "scope": "tracked-and-unignored-worktree",
        }
        with (
            tempfile.TemporaryDirectory() as temporary,
            mock.patch.object(release, "_privacy_report", return_value=expected),
        ):
            with self.assertRaisesRegex(
                release.ReleaseArtifactError, "fresh local replay"
            ):
                release._verify_technical_evidence(
                    Path(temporary),
                    kind="privacy",
                    document=forged,
                    manifest={},
                    semantica_root=None,
                    replay_regressions=False,
                )

    def test_source_boundary_allows_only_fixed_generated_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(
                ["git", "config", "user.name", "Release Test"],
                cwd=root,
                check=True,
            )
            subprocess.run(
                ["git", "config", "user.email", "release@test.invalid"],
                cwd=root,
                check=True,
            )
            source = root / "source.txt"
            source.write_text("source v1\n", encoding="utf-8")
            unicode_pdf = root / release.BOOK_SPECS["vol1"]["pdf"]
            unicode_pdf.parent.mkdir(parents=True)
            unicode_pdf.write_bytes(b"old pdf bytes")
            subprocess.run(
                ["git", "add", "source.txt", unicode_pdf.relative_to(root)],
                cwd=root,
                check=True,
            )
            subprocess.run(
                ["git", "commit", "-q", "-m", "source baseline"],
                cwd=root,
                check=True,
            )
            commit = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=root,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
            tree = subprocess.run(
                ["git", "rev-parse", f"{commit}^{{tree}}"],
                cwd=root,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()

            fixed = root / release.MANIFEST_PATH
            fixed.parent.mkdir(parents=True, exist_ok=True)
            fixed.write_text("{}\n", encoding="utf-8")
            unicode_pdf.write_bytes(b"new pdf bytes")
            self.assertEqual(tree, release._git_source_boundary(root, commit))

            non_fixed = root / "build" / "unbound-output.json"
            non_fixed.parent.mkdir(parents=True)
            non_fixed.write_text("{}\n", encoding="utf-8")
            with self.assertRaisesRegex(
                release.ReleaseArtifactError, "outside the fixed generated-artifact"
            ):
                release._git_source_boundary(root, commit)

            non_fixed.unlink()
            source.write_text("source v2\n", encoding="utf-8")
            with self.assertRaisesRegex(
                release.ReleaseArtifactError, "outside the fixed generated-artifact"
            ):
                release._git_source_boundary(root, commit)

    def test_regression_evidence_rejects_unbound_claim_fields(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            log_path = (
                root / release.REGRESSION_LOG_PATHS["ontology_engineering_regression"]
            )
            log_path.parent.mkdir(parents=True)
            log_path.write_text("2 passed in 0.01s\n", encoding="utf-8")
            base = {
                "$schema": release.REGRESSION_SCHEMA,
                "command": list(
                    release.REGRESSION_COMMANDS["ontology_engineering_regression"]
                ),
                "commit": "a" * 40,
                "log": release.artifact_ref(
                    root,
                    release.REGRESSION_LOG_PATHS["ontology_engineering_regression"],
                ),
                "passed": True,
                "passed_count": 2,
                "repository": release.REGRESSION_REPOSITORIES[
                    "ontology_engineering_regression"
                ],
                "return_code": 0,
                "summary": "2 passed in 0.01s",
            }
            manifest = {
                "ontology_engineering": {"source_commit": "a" * 40},
                "semantica": {"commit": "b" * 40},
            }
            mutations = {
                "empty command": {"command": []},
                "empty summary": {"summary": ""},
                "wrong repository": {
                    "repository": "https://example.invalid/forged.git"
                },
                "wrong commit": {"commit": "c" * 40},
            }
            for label, mutation in mutations.items():
                with self.subTest(label=label):
                    document = {**base, **mutation}
                    with self.assertRaisesRegex(
                        release.ReleaseArtifactError,
                        "inconsistent or commit-unbound",
                    ):
                        release._verify_technical_evidence(
                            root,
                            kind="ontology_engineering_regression",
                            document=document,
                            manifest=manifest,
                            semantica_root=None,
                            replay_regressions=False,
                        )

    def test_syntactically_green_fake_log_fails_fresh_regression_replay(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            relative = release.REGRESSION_LOG_PATHS["ontology_engineering_regression"]
            log_path = root / relative
            log_path.parent.mkdir(parents=True)
            log_path.write_text("999 passed in 0.01s\n", encoding="utf-8")
            document = {
                "$schema": release.REGRESSION_SCHEMA,
                "command": list(
                    release.REGRESSION_COMMANDS["ontology_engineering_regression"]
                ),
                "commit": "a" * 40,
                "log": release.artifact_ref(root, relative),
                "passed": True,
                "passed_count": 999,
                "repository": release.REGRESSION_REPOSITORIES[
                    "ontology_engineering_regression"
                ],
                "return_code": 0,
                "summary": "999 passed in 0.01s",
            }
            replay = subprocess.CompletedProcess(
                args=["runtime-python", "-m", "pytest"],
                returncode=0,
                stdout="2 passed in 0.01s\n",
                stderr="",
            )
            with (
                mock.patch.object(release, "_run_regression", return_value=replay),
                self.assertRaisesRegex(
                    release.ReleaseArtifactError, "replay test count differs"
                ),
            ):
                release._verify_technical_evidence(
                    root,
                    kind="ontology_engineering_regression",
                    document=document,
                    manifest={
                        "ontology_engineering": {"source_commit": "a" * 40},
                        "semantica": {"commit": "b" * 40},
                    },
                    semantica_root=None,
                    replay_regressions=True,
                )


if __name__ == "__main__":
    unittest.main()
