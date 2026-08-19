from __future__ import annotations

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

from ontology_engineering import semantic_engagement as engagement
from ontology_engineering import semantica_runtime as runtime


PACKAGE_ID = "semantica.chapter_packages.vol1.ch01"
PACKAGE_VERSION = "1.0.0"
RECORDED_AT = "2026-08-19T12:00:00Z"
NATIVE_PACKAGE_ID = "industry.manufacturing_test"
NATIVE_EXECUTION_RECEIPT_SHA256 = "f" * 64
NATIVE_CAPABILITY = "semantic.package.load"


def write_json(path: Path, value: object) -> Path:
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return path


def package_binding(
    digest: str,
    *,
    semantic_api: str = runtime.NATIVE_REFINERY_CONTRACT,
    allowed_actions: list[str] | None = None,
) -> dict[str, object]:
    return {
        "$schema": engagement.BINDING_SCHEMA,
        "binding_id": "binding:test-package",
        "project": {
            "project_id": "project:test",
            "domain": "manufacturing-test",
        },
        "semantic_target": {
            "kind": "package",
            "package_id": PACKAGE_ID,
            "package_version": PACKAGE_VERSION,
        },
        "baseline": {"version": PACKAGE_VERSION, "digest": digest},
        "evidence": {"logical_root": "evidence:project-test"},
        "authority": {
            "fact": {
                "authority_id": "authority:test-facts",
                "scope": ["observations", "measurements"],
            },
            "decision": {
                "authority_id": "authority:test-review-board",
                "scope": ["candidate-verdict", "promotion", "publication"],
            },
        },
        "allowed_actions": allowed_actions or ["doctor", "discover", "run", "verify"],
        "lifecycle_actions": [],
        "promotion": {
            "target": engagement.PROMOTION_TARGET,
            "requires_decision_authority": True,
        },
        "semantic_api": semantic_api,
        "created_at": "2026-08-19T10:00:00Z",
    }


def workspace_binding(
    record_sha256: str,
    version: int,
    package_id: str,
    *,
    semantic_api: str = runtime.NATIVE_REFINERY_CONTRACT,
) -> dict[str, object]:
    return {
        "$schema": engagement.BINDING_SCHEMA,
        "binding_id": "binding:test-workspace",
        "project": {
            "project_id": "project:test",
            "domain": "manufacturing-test",
        },
        "semantic_target": {
            "kind": "workspace",
            "workspace_id": "workspace:test-domain",
            "package_id": package_id,
        },
        "baseline": {
            "version": "v{:04d}".format(version),
            "digest": record_sha256,
        },
        "evidence": {"logical_root": "evidence:project-test"},
        "authority": {
            "fact": {
                "authority_id": "authority:test-facts",
                "scope": ["observations", "measurements"],
            },
            "decision": {
                "authority_id": "authority:test-review-board",
                "scope": ["candidate-verdict", "promotion", "publication"],
            },
        },
        "allowed_actions": [
            "doctor",
            "discover",
            "open",
            "run",
            "propose",
            "commit",
            "verify",
            "history",
            "promote",
        ],
        "lifecycle_actions": [
            "candidate",
            "proposed",
            "committed",
            "regression_passed",
            "release_complete",
            "promoted",
        ],
        "promotion": {
            "target": engagement.PROMOTION_TARGET,
            "requires_decision_authority": True,
        },
        "semantic_api": semantic_api,
        "created_at": "2026-08-19T10:00:00Z",
    }


def task_envelope(actions: list[str]) -> dict[str, object]:
    return {
        "$schema": engagement.TASK_SCHEMA,
        "task_id": "task:test-001",
        "task_kind": "engineering-review",
        "intent": "Test a reusable manufacturing observation",
        "project": "project:test",
        "domain": "manufacturing-test",
        "requested_decision": "Decide whether the observation is a candidate",
        "evidence": [
            {
                "source_id": "source:observation-001",
                "uri": "evidence:project-test/observation-001",
                "sha256": "a" * 64,
                "media_type": "application/json",
                "captured_at": "2026-08-19T10:30:00Z",
            }
        ],
        "requested_actions": actions,
        "actor_id": "actor:test-engineer",
        "required_capabilities": [NATIVE_CAPABILITY],
        "created_at": "2026-08-19T11:00:00Z",
    }


def native_workspace_binding(empty_digest: str) -> dict[str, object]:
    value = workspace_binding(
        empty_digest,
        0,
        NATIVE_PACKAGE_ID,
        semantic_api=runtime.NATIVE_REFINERY_CONTRACT,
    )
    value["baseline"] = {"version": "0", "digest": empty_digest}
    return value


def native_delta(empty_digest: str) -> dict[str, object]:
    """Request Semantica's own complete executable acceptance delta."""

    self_reported_empty = runtime.native_refinery_empty_package_sha256()
    if empty_digest != self_reported_empty:
        raise AssertionError("test baseline differs from Semantica empty package")
    source = task_envelope(["propose"])["evidence"][0]
    assert isinstance(source, dict)
    return dict(
        runtime.native_refinery_acceptance_delta(
            package_id=NATIVE_PACKAGE_ID,
            source_evidence=source,
            created_by="authority:test-facts",
            created_at=RECORDED_AT,
            target_version="1.0.0",
        )
    )


def native_delta_schema_fixture(empty_digest: str) -> dict[str, object]:
    """Inert shape-only input for adapter unknown-field rejection tests."""

    asset = {
        "category": "ontology",
        "asset_id": "schema-only",
        "operation": "add",
        "media_type": "application/octet-stream",
        "sha256": "1e7b3c97f8221b1f5487a2ee19e880870a3cac68565ce3b59a900628c5e9c312",
        "content_base64": "c2NoZW1hLW9ubHk=",
    }
    return {
        "schema_version": "1.0",
        "package_id": NATIVE_PACKAGE_ID,
        "base_version": "0",
        "base_package_sha256": empty_digest,
        "target_version": "1.0.0",
        "rationale": "Exercise only the OE delta field contract.",
        "created_by": "authority:test-facts",
        "created_at": RECORDED_AT,
        "required_capabilities": [NATIVE_CAPABILITY],
        "source_evidence": task_envelope(["propose"])["evidence"],
        "book_impact": "none",
        "ontology": [asset],
        "competency_questions": [],
        "shapes": [],
        "queries": [],
        "rules": [],
        "cases": [],
        "contract": [],
        "provenance": [],
    }


def native_engagement_evidence() -> dict[str, object]:
    capability = NATIVE_CAPABILITY

    def phase(name: str, marker: str) -> dict[str, object]:
        return {
            "name": name,
            "status": "passed",
            "required_capabilities": [capability],
            "observed_capabilities": [capability],
            "evidence_sha256": marker * 64,
            "details": {"executed": True},
        }

    return {
        "engagement_id": "engagement:test-native-001",
        "execution": phase("execution", "1"),
        "regression": phase("regression", "2"),
        "receipt": phase("receipt", "3"),
        "release": phase("release", "4"),
        "learning": {
            "status": "candidate",
            "rationale": "The observation is reusable outside this project.",
        },
        "execution_receipts": [
            {
                "receipt_sha256": NATIVE_EXECUTION_RECEIPT_SHA256,
                "package_id": PACKAGE_ID,
                "package_version": PACKAGE_VERSION,
                "package_digest": "e" * 64,
            }
        ],
        "created_at": RECORDED_AT,
    }


def native_authorization(action: str, candidate_sha256: str) -> dict[str, object]:
    return {
        "$schema": engagement.AUTHORIZATION_SCHEMA,
        "authorization_id": "authorization:test-{}".format(action),
        "action": action,
        "actor_id": "authority:test-review-board",
        "authority_id": "authority:test-review-board",
        "authority_scope": ["candidate-verdict", "promotion", "publication"],
        "package_id": NATIVE_PACKAGE_ID,
        "delta_sha256": candidate_sha256,
        "promotion_target": engagement.PROMOTION_TARGET,
        "reason": "Reviewed against the bound project evidence.",
        "source": task_envelope(["commit"])["evidence"][0],
        "issued_at": RECORDED_AT,
        "decisions": [],
    }


class SemanticEngagementBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.package_digest = runtime.run_package(PACKAGE_ID).package_digest

    def test_strict_binding_and_task_bind_authority_without_physical_evidence_path(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binding_path = write_json(
                root / "binding.json", package_binding(self.package_digest)
            )
            task_path = write_json(root / "task.json", task_envelope(["run", "verify"]))
            binding = engagement.read_project_binding(binding_path)
            task = engagement.read_task_envelope(task_path, binding=binding)

            self.assertEqual("evidence:project-test", binding.evidence_logical_root)
            self.assertEqual(
                "authority:test-facts", binding.fact_authority["authority_id"]
            )
            self.assertEqual(
                "authority:test-review-board",
                binding.decision_authority["authority_id"],
            )
            self.assertEqual("task:test-001", task.task_id)
            self.assertRegex(binding.source_sha256, r"^[0-9a-f]{64}$")
            self.assertRegex(task.source_sha256, r"^[0-9a-f]{64}$")

            binding_dict = binding.as_dict()
            self.assertEqual("binding:test-package", binding_dict["binding_id"])
            self.assertEqual(
                {
                    "kind": "package",
                    "package_id": PACKAGE_ID,
                    "package_version": PACKAGE_VERSION,
                },
                binding_dict["semantic_target"],
            )
            self.assertEqual(
                binding.baseline_digest, binding_dict["baseline"]["digest"]
            )
            self.assertEqual(
                "authority:test-review-board",
                binding_dict["authority"]["decision"]["authority_id"],
            )
            task_dict = task.as_dict()
            self.assertEqual("task:test-001", task_dict["task_id"])
            self.assertEqual(["run", "verify"], task_dict["requested_actions"])
            self.assertEqual(
                binding_dict,
                json.loads(engagement.canonical_json(binding_dict)),
            )
            self.assertEqual(
                task_dict,
                json.loads(engagement.canonical_json(task_dict)),
            )
            native_task = engagement._native_task(task, "execute_candidate")
            self.assertEqual(["execute_candidate"], native_task["requested_actions"])

            lifecycle_task_value = task_envelope(
                ["open", "propose", "commit", "verify", "promote"]
            )
            lifecycle_task = engagement.read_task_envelope(
                write_json(root / "lifecycle-task.json", lifecycle_task_value),
                binding=engagement.read_project_binding(
                    write_json(
                        root / "lifecycle-binding.json",
                        native_workspace_binding("0" * 64),
                    )
                ),
            )
            self.assertEqual(
                ["proposed"],
                engagement._native_task(lifecycle_task, "proposed")[
                    "requested_actions"
                ],
            )
            self.assertEqual(
                ["committed"],
                engagement._native_task(lifecycle_task, "committed")[
                    "requested_actions"
                ],
            )

    def test_task_kind_is_open_provenance_text_not_an_authority_enum(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binding_path = write_json(
                root / "binding.json", package_binding(self.package_digest)
            )
            binding = engagement.read_project_binding(binding_path)
            value = task_envelope(["run"])
            value["task_kind"] = "supplier-specific-vibration-triage-v7"
            task = engagement.read_task_envelope(
                write_json(root / "task.json", value), binding=binding
            )
            self.assertEqual("supplier-specific-vibration-triage-v7", task.task_kind)
            self.assertEqual(["run"], list(task.requested_actions))

    def test_successor_binding_is_a_new_parseable_projection_not_an_in_place_edit(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original_path = write_json(
                root / "binding.json",
                workspace_binding("0" * 64, 0, NATIVE_PACKAGE_ID),
            )
            # The projection helper itself does not require the native API.
            original_bytes = original_path.read_bytes()
            binding = engagement.read_project_binding(original_path)
            descriptor = {
                "package_id": NATIVE_PACKAGE_ID,
                "version": "1.0.0",
                "package_sha256": "1" * 64,
                "manifest_sha256": "2" * 64,
                "promotion_record_sha256": "3" * 64,
                "promoted_at": RECORDED_AT,
            }
            successor = engagement._next_binding_projection(binding, descriptor)
            self.assertFalse(successor["auto_applied"])
            self.assertTrue(successor["requires_control_plane_approval"])
            self.assertEqual(
                binding.source_sha256, successor["predecessor_binding_sha256"]
            )
            self.assertEqual(original_bytes, original_path.read_bytes())
            successor_path = write_json(
                root / "binding-successor.json", successor["document"]
            )
            parsed = engagement.read_project_binding(successor_path)
            self.assertNotEqual(binding.binding_id, parsed.binding_id)
            self.assertEqual("1.0.0", parsed.baseline_version)
            self.assertEqual("1" * 64, parsed.baseline_digest)
            self.assertEqual(
                binding.source_sha256,
                parsed.predecessor["binding_sha256"] if parsed.predecessor else None,
            )
            self.assertEqual(
                "3" * 64,
                parsed.predecessor["promotion_record_sha256"]
                if parsed.predecessor
                else None,
            )

    def test_backend_fallback_unknown_fields_and_unbound_evidence_fail_closed(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            value = package_binding(self.package_digest)
            value["runtime_backend"] = "arbitrary"
            with self.assertRaises(engagement.BindingValidationError):
                engagement.read_project_binding(
                    write_json(root / "backend.json", value)
                )

            value = package_binding(self.package_digest)
            value["fallback"] = {"enabled": True}
            with self.assertRaises(engagement.BindingValidationError):
                engagement.read_project_binding(
                    write_json(root / "fallback.json", value)
                )

            binding_path = write_json(
                root / "binding.json", package_binding(self.package_digest)
            )
            binding = engagement.read_project_binding(binding_path)
            task = task_envelope(["run"])
            task["evidence"][0]["uri"] = "evidence:another-project/item"
            with self.assertRaises(engagement.BindingValidationError):
                engagement.read_task_envelope(
                    write_json(root / "task.json", task), binding=binding
                )

            value = package_binding(self.package_digest)
            value["promotion"]["target"] = "semantica.industry.not-a-channel"
            with self.assertRaises(engagement.BindingValidationError):
                engagement.read_project_binding(
                    write_json(root / "wrong-promotion-target.json", value)
                )

            writable_package = package_binding(
                self.package_digest, allowed_actions=["run", "propose"]
            )
            with self.assertRaisesRegex(engagement.BindingValidationError, "read-only"):
                engagement.read_project_binding(
                    write_json(root / "writable-package.json", writable_package)
                )

    def test_action_must_be_allowed_by_both_binding_and_task(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binding_path = write_json(
                root / "binding.json",
                package_binding(self.package_digest, allowed_actions=["verify"]),
            )
            task_path = write_json(root / "task.json", task_envelope(["run"]))
            output = io.StringIO()
            with redirect_stdout(output):
                exit_code = engagement.main(
                    ["run", "--binding", str(binding_path), "--task", str(task_path)]
                )
            response = json.loads(output.getvalue())
            self.assertEqual(0, exit_code)
            self.assertEqual("blocked", response["command_verdict"])
            self.assertEqual(
                "ActionNotAllowedError", response["execution"]["error_type"]
            )

    def test_open_requires_current_task_and_binds_its_hash_into_context(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binding_path = write_json(
                root / "binding.json", native_workspace_binding("0" * 64)
            )
            task_path = write_json(root / "task.json", task_envelope(["open"]))
            output = io.StringIO()
            with redirect_stdout(output):
                engagement.main(["open", "--binding", str(binding_path)])
            blocked = json.loads(output.getvalue())
            self.assertEqual("blocked", blocked["command_verdict"])
            self.assertIn("--task", blocked["execution"]["message"])

            _, task, _, response = engagement._load_context(
                "open", binding_path, task_path=task_path, require_task=True
            )
            self.assertIsNotNone(task)
            self.assertEqual(
                response["task"]["source_sha256"],
                task.source_sha256 if task else None,
            )

    def test_every_workspace_write_requires_a_current_task_for_that_action(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binding_path = write_json(
                root / "binding.json",
                native_workspace_binding("0" * 64),
            )
            for command, extra in (
                ("propose", ["--delta", "delta.json"]),
                ("commit", ["--candidate", "1" * 64]),
                ("promote", ["--candidate", "1" * 64]),
            ):
                output = io.StringIO()
                with redirect_stdout(output):
                    engagement.main(
                        [
                            command,
                            "--binding",
                            str(binding_path),
                            "--workspace",
                            str(root / "workspace"),
                            *extra,
                        ]
                    )
                response = json.loads(output.getvalue())
                self.assertEqual("blocked", response["command_verdict"])
                self.assertIn("--task", response["execution"]["message"])

            with self.assertRaisesRegex(
                engagement.BindingValidationError, "requires --task"
            ):
                engagement.verify(
                    binding_path,
                    workspace=root / "workspace",
                    candidate_sha256="1" * 64,
                )

            wrong_task = write_json(
                root / "wrong-task.json", task_envelope(["propose"])
            )
            with self.assertRaisesRegex(
                engagement.ActionNotAllowedError, "not requested"
            ):
                engagement.commit(
                    binding_path,
                    workspace=root / "workspace",
                    candidate_sha256="1" * 64,
                    authorization=root / "authorization.json",
                    task=wrong_task,
                )

    def test_native_adapter_accepts_plain_declared_authority_and_rejects_scope_drift(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binding = engagement.read_project_binding(
                write_json(
                    root / "binding.json",
                    native_workspace_binding("0" * 64),
                )
            )
            candidate_sha256 = "9" * 64
            authorization = native_authorization("commit", candidate_sha256)
            authorization["authority_scope"] = [
                "publication",
                "candidate-verdict",
                "promotion",
            ]
            authorization_path = write_json(root / "authorization.json", authorization)

            projected = engagement._native_authorization_input(
                authorization_path,
                binding=binding,
                expected_action="commit",
                candidate_sha256=candidate_sha256,
            )
            expected_token = engagement.canonical_json(
                {
                    "authority_id": "authority:test-review-board",
                    "scope": ["candidate-verdict", "promotion", "publication"],
                }
            )
            self.assertEqual(expected_token, projected["actor_id"])
            self.assertEqual(expected_token, projected["authority"])
            self.assertEqual(
                "authority:test-review-board",
                json.loads(authorization_path.read_text(encoding="utf-8"))["actor_id"],
            )

            authorization["authority_scope"] = ["candidate-verdict"]
            with self.assertRaises(engagement.ActionNotAllowedError):
                engagement._native_authorization_input(
                    write_json(root / "wrong-scope.json", authorization),
                    binding=binding,
                    expected_action="commit",
                    candidate_sha256=candidate_sha256,
                )

            authorization["authority_scope"] = [
                "candidate-verdict",
                "candidate-verdict",
                "promotion",
                "publication",
            ]
            with self.assertRaises(engagement.BindingValidationError):
                engagement._native_authorization_input(
                    write_json(root / "duplicate-scope.json", authorization),
                    binding=binding,
                    expected_action="commit",
                    candidate_sha256=candidate_sha256,
                )

    def test_native_delta_rejects_top_level_and_asset_shadow_semantics(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binding = engagement.read_project_binding(
                write_json(
                    root / "binding.json",
                    native_workspace_binding("0" * 64),
                )
            )
            delta = native_delta_schema_fixture("0" * 64)
            delta["shadow_semantics"] = {"allow": True}
            with self.assertRaises(engagement.BindingValidationError):
                engagement._native_delta_input(
                    write_json(root / "shadow-delta.json", delta), binding=binding
                )

            delta = native_delta_schema_fixture("0" * 64)
            delta["ontology"][0]["shadow_semantics"] = {"allow": True}
            with self.assertRaises(engagement.BindingValidationError):
                engagement._native_delta_input(
                    write_json(root / "shadow-asset.json", delta), binding=binding
                )


class SemanticEngagementPackageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.package_digest = runtime.run_package(PACKAGE_ID).package_digest

    def test_doctor_blocks_when_native_refinery_capability_is_missing(self) -> None:
        capabilities = {
            "native": {
                "contract": runtime.NATIVE_REFINERY_CONTRACT,
                "available": False,
                "status": "blocked",
                "operations": [],
                "missing_operations": ["open_engagement"],
            },
        }
        with mock.patch.object(
            runtime, "semantic_refinery_capabilities", return_value=capabilities
        ):
            response = engagement.doctor()
        self.assertEqual("blocked", response["command_verdict"])
        self.assertEqual("blocked", response["learning"]["status"])
        self.assertEqual("found", response["corpus_found"]["status"])

    def test_same_version_stale_installed_wheel_fails_source_identity(self) -> None:
        with mock.patch.object(
            runtime, "installed_runtime_artifact_sha256", return_value="0" * 64
        ):
            with self.assertRaises(engagement.CapabilityUnavailableError):
                engagement.doctor()

    def test_doctor_rejects_shadowed_or_record_tampered_import(self) -> None:
        with mock.patch.object(
            runtime,
            "verify_installed_runtime_record",
            side_effect=RuntimeError(
                "imported Semantica is outside the selected installed distribution"
            ),
        ):
            with self.assertRaisesRegex(
                engagement.CapabilityUnavailableError, "outside"
            ):
                engagement.doctor()

    def test_native_feature_detection_rejects_contract_surface_drift(self) -> None:
        module_operations = (
            "build_refinery_acceptance_delta",
            "open_engagement",
            "propose_candidate",
            "commit_candidate",
            "execute_candidate",
            "derive_gate_evidence",
            "verify_candidate",
            "promote_candidate",
            "history",
        )
        declaration = {
            "contract": runtime.NATIVE_REFINERY_CONTRACT,
            "schema_version": "1.0",
            "states": list(runtime.NATIVE_REFINERY_STATES[:-1]),
            "asset_categories": list(runtime.NATIVE_REFINERY_ASSET_CATEGORIES),
            "delta_categories": list(runtime.NATIVE_REFINERY_ASSET_CATEGORIES)
            + ["book_impact"],
            "book_impacts": list(runtime.NATIVE_REFINERY_BOOK_IMPACTS),
            "case_kinds": list(runtime.NATIVE_REFINERY_CASE_KINDS),
            "regression_check_ids": list(runtime.NATIVE_REFINERY_REGRESSION_CHECK_IDS),
            "release_check_ids": list(runtime.NATIVE_REFINERY_RELEASE_CHECK_IDS),
            "transition_context_actions": list(
                runtime.NATIVE_REFINERY_TRANSITION_CONTEXT_ACTIONS
            ),
            "transition_context_required_operations": list(
                runtime.NATIVE_REFINERY_TRANSITION_CONTEXT_REQUIRED_OPERATIONS
            ),
            "runner_contract": runtime.NATIVE_REFINERY_RUNNER_CONTRACT,
            "operations": list(runtime.NATIVE_REFINERY_OPERATIONS),
            "publication_owned_externally": True,
        }
        registry_type = type(
            "RegistrySurface",
            (),
            {
                "resolve_package": lambda self: None,
                "execution_manifest": lambda self: None,
            },
        )
        dto_type = type("RequiredNativeDTO", (), {})
        facade = SimpleNamespace(
            refinery_capabilities=lambda: declaration,
            EMPTY_PACKAGE_SHA256="0" * 64,
            IndustryOntologyRegistry=registry_type,
            IndustryPackageDescriptorDTO=dto_type,
            CandidateVerificationDTO=dto_type,
            EngagementPhaseDTO=dto_type,
            ExecutionReceiptReferenceDTO=dto_type,
            LearningResultDTO=dto_type,
            PackageDelta=dto_type,
            ProjectOntologyBinding=dto_type,
            REFINERY_CONTRACT=runtime.NATIVE_REFINERY_CONTRACT,
            REFINERY_SCHEMA_VERSION="1.0",
            RefineryAuthorizationDTO=dto_type,
            RefineryGateEvidenceDTO=dto_type,
            RefineryStateDTO=dto_type,
            RuntimeSourceIdentityDTO=dto_type,
            SemanticEngagementReceipt=dto_type,
            SemanticTaskEnvelope=dto_type,
            SourceEvidenceDTO=dto_type,
            SubjectExecutionSuiteDTO=dto_type,
            TransitionContextDTO=dto_type,
            **{name: (lambda: None) for name in module_operations},
        )
        with mock.patch.object(runtime, "_refinery", facade):
            detected = runtime.semantic_refinery_capabilities()["native"]
        self.assertFalse(detected["available"])
        self.assertTrue(detected["contract_matches"])
        self.assertFalse(detected["declaration_matches"])
        self.assertEqual(["states"], detected["declaration_mismatches"])
        self.assertEqual([], detected["missing_symbols"])

    def test_discover_returns_only_stable_descriptor_data(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = write_json(
                Path(temporary) / "binding.json", package_binding(self.package_digest)
            )
            response = engagement.discover(path)
            self.assertEqual("found", response["corpus_found"]["status"])
            self.assertEqual(
                PACKAGE_ID, response["corpus_found"]["selected"]["package_id"]
            )
            self.assertNotIn("manifest_path", response["corpus_found"]["selected"])
            self.assertRegex(
                response["corpus_found"]["selected"]["package_digest"],
                r"^[0-9a-f]{64}$",
            )
            self.assertIn(
                "semantic.package.load",
                response["corpus_found"]["selected"]["required_capabilities"],
            )
            self.assertGreaterEqual(len(response["corpus_found"]["packages"]), 30)

    def test_unbound_discover_supports_selection_before_package_id_is_known(
        self,
    ) -> None:
        response = engagement.discover()
        self.assertEqual("passed", response["command_verdict"])
        self.assertEqual("found", response["corpus_found"]["status"])
        self.assertIsNone(response["corpus_found"]["selected"])
        self.assertIsNone(response["binding"])
        first = response["corpus_found"]["packages"][0]
        self.assertRegex(first["package_digest"], r"^[0-9a-f]{64}$")
        self.assertTrue(first["available_capabilities"])
        bootstrap = response["corpus_found"]["native_workspace_bootstrap"]
        self.assertEqual("0", bootstrap["baseline_version"])
        self.assertEqual(runtime.NATIVE_REFINERY_CONTRACT, bootstrap["semantic_api"])
        self.assertGreaterEqual(len(response["corpus_found"]["packages"]), 30)

    def test_unbound_discover_rejects_runtime_selection_and_arbitrary_paths(
        self,
    ) -> None:
        for option, value in (
            ("--backend", "dynamic"),
            ("--fallback", "legacy"),
            ("--path", "/tmp/arbitrary-package"),
        ):
            output = io.StringIO()
            with redirect_stdout(output):
                exit_code = engagement.main(["discover", option, value])
            response = json.loads(output.getvalue())
            self.assertEqual(0, exit_code)
            self.assertEqual("blocked", response["command_verdict"])
            self.assertEqual(
                "BindingValidationError", response["execution"]["error_type"]
            )

    def test_run_injects_source_lock_and_keeps_six_verdicts_separate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binding_path = write_json(
                root / "binding.json", package_binding(self.package_digest)
            )
            task_path = write_json(root / "task.json", task_envelope(["run"]))
            response = engagement.run(binding_path, task=task_path)
            lock = runtime.read_runtime_source_lock()

            for field in (
                "corpus_found",
                "execution",
                "regression",
                "receipt",
                "release",
                "learning",
            ):
                self.assertIn(field, response)
            self.assertEqual(lock.commit, response["runtime_source"]["commit"])
            self.assertEqual(lock.version, response["runtime_source"]["version"])
            self.assertEqual(
                lock.artifact_sha256, response["runtime_source"]["wheel_sha256"]
            )
            self.assertEqual("verified", response["receipt"]["status"])
            self.assertEqual(
                lock.commit, response["receipt"]["receipt"]["runtime_commit"]
            )
            self.assertEqual("no_delta", response["learning"]["verdict"])
            self.assertEqual(
                [
                    "candidate",
                    "proposed",
                    "committed",
                    "regression_passed",
                    "release_complete",
                    "promoted",
                ],
                response["learning"]["state_model"],
            )
            # This deliberately partial chapter proves that transport success
            # and receipt integrity do not turn release status green.
            self.assertEqual("blocked", response["release"]["status"])
            self.assertEqual("blocked", response["command_verdict"])

    def test_cli_exit_zero_means_json_delivery_not_green_release(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = write_json(
                Path(temporary) / "binding.json", package_binding(self.package_digest)
            )
            output = io.StringIO()
            with redirect_stdout(output):
                exit_code = engagement.main(["run", "--binding", str(path)])
            response = json.loads(output.getvalue())
            self.assertEqual(0, exit_code)
            self.assertEqual("completed", response["transport"]["status"])
            self.assertEqual("blocked", response["command_verdict"])
            self.assertEqual("blocked", response["release"]["status"])

    def test_package_digest_mismatch_blocks_before_any_green_claim(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = write_json(
                Path(temporary) / "binding.json", package_binding("0" * 64)
            )
            output = io.StringIO()
            with redirect_stdout(output):
                engagement.main(["run", "--binding", str(path)])
            response = json.loads(output.getvalue())
            self.assertEqual("blocked", response["command_verdict"])
            self.assertEqual(
                "BaselineMismatchError", response["execution"]["error_type"]
            )
            self.assertEqual("not_checked", response["release"]["status"])

    def test_unknown_backend_cli_option_is_rejected_as_stable_json(self) -> None:
        output = io.StringIO()
        with redirect_stdout(output):
            exit_code = engagement.main(["doctor", "--backend", "dynamic"])
        response = json.loads(output.getvalue())
        self.assertEqual(0, exit_code)
        self.assertEqual("blocked", response["command_verdict"])
        self.assertEqual("BindingValidationError", response["execution"]["error_type"])


class SemanticEngagementSoleControlPlaneTests(unittest.TestCase):
    def test_legacy_governance_contract_cannot_be_selected_by_any_binding(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            value = workspace_binding(
                "0" * 64,
                0,
                NATIVE_PACKAGE_ID,
                semantic_api="semantica.ontology.governance/v1-compatibility",
            )
            with self.assertRaises(engagement.BindingValidationError):
                engagement.read_project_binding(
                    write_json(root / "legacy-binding.json", value)
                )

    def test_capability_output_has_no_parallel_lifecycle(self) -> None:
        capabilities = runtime.semantic_refinery_capabilities()
        self.assertEqual({"native"}, set(capabilities))


class SemanticEngagementNativeRefineryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.workspace = self.root / "native-registry"
        self.lock = runtime.read_runtime_source_lock()
        self.empty_digest = runtime.native_refinery_empty_package_sha256()
        self.binding_path = write_json(
            self.root / "binding.json",
            native_workspace_binding(self.empty_digest),
        )
        self.task_path = write_json(
            self.root / "task.json",
            task_envelope(
                [
                    "open",
                    "run",
                    "propose",
                    "commit",
                    "verify",
                    "history",
                    "promote",
                ]
            ),
        )
        self.delta_path = write_json(
            self.root / "delta.json", native_delta(self.empty_digest)
        )
        self.engagement_path = write_json(
            self.root / "engagement.json", native_engagement_evidence()
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _cli(self, *arguments: str) -> dict[str, object]:
        completed = subprocess.run(
            [
                sys.executable,
                str(
                    Path(__file__).resolve().parents[1]
                    / "scripts"
                    / "semantic_engagement.py"
                ),
                *arguments,
            ],
            cwd=Path(__file__).resolve().parents[1],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(0, completed.returncode, completed.stderr)
        return json.loads(completed.stdout)

    def test_read_only_missing_workspace_has_zero_creation_side_effect(self) -> None:
        missing = self.root / "missing-read-only-registry"
        for arguments in (
            (
                "discover",
                "--binding",
                str(self.binding_path),
                "--workspace",
                str(missing),
            ),
            (
                "run",
                "--binding",
                str(self.binding_path),
                "--workspace",
                str(missing),
                "--task",
                str(self.task_path),
            ),
            (
                "history",
                "--binding",
                str(self.binding_path),
                "--workspace",
                str(missing),
                "--candidate",
                "1" * 64,
            ),
        ):
            response = self._cli(*arguments)
            self.assertEqual("blocked", response["command_verdict"])
            self.assertFalse(missing.exists())

    def test_crash_after_regression_recovers_in_a_new_process(self) -> None:
        self._cli(
            "open",
            "--binding",
            str(self.binding_path),
            "--task",
            str(self.task_path),
            "--workspace",
            str(self.workspace),
        )
        proposed = self._cli(
            "propose",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--delta",
            str(self.delta_path),
            "--task",
            str(self.task_path),
            "--engagement",
            str(self.engagement_path),
            "--recorded-at",
            RECORDED_AT,
        )
        candidate_sha256 = proposed["learning"]["candidate"]["candidate_sha256"]
        commit_authorization = write_json(
            self.root / "recovery-commit-authorization.json",
            native_authorization("commit", candidate_sha256),
        )
        committed = self._cli(
            "commit",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
            "--authorization",
            str(commit_authorization),
            "--task",
            str(self.task_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("committed", committed["learning"]["current_state"])

        refinery_module = runtime._refinery
        self.assertIsNotNone(refinery_module)
        registry_type = refinery_module.IndustryOntologyRegistry
        original_gate_evidence = registry_type.gate_evidence

        def crash_before_release(
            registry: object, *args: object, **kwargs: object
        ) -> object:
            if kwargs.get("gate") == "release":
                raise RuntimeError("simulated crash after regression checkpoint")
            return original_gate_evidence(registry, *args, **kwargs)

        with mock.patch.object(
            registry_type, "gate_evidence", new=crash_before_release
        ):
            with self.assertRaisesRegex(RuntimeError, "simulated crash"):
                engagement.verify(
                    self.binding_path,
                    workspace=self.workspace,
                    candidate_sha256=candidate_sha256,
                    task=self.task_path,
                    recorded_at=RECORDED_AT,
                )

        checkpoint = self._cli(
            "history",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
        )
        self.assertEqual("regression_passed", checkpoint["learning"]["current_state"])

        recovered = self._cli(
            "verify",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
            "--task",
            str(self.task_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("passed", recovered["command_verdict"])
        self.assertTrue(recovered["execution"]["recovered_from_immutable_evidence"])
        self.assertIsNone(recovered["execution"]["subject_execution_suite"])
        self.assertEqual(
            {"derive_release_gate", "release_complete"},
            set(recovered["execution"]["transition_contexts"]),
        )
        self.assertEqual("complete", recovered["release"]["status"])

    def test_complete_new_process_cli_chain_and_successor_binding(self) -> None:
        original_binding_bytes = self.binding_path.read_bytes()
        discovered = self._cli("discover")
        bootstrap = discovered["corpus_found"]["native_workspace_bootstrap"]
        self.assertEqual("available", bootstrap["status"])
        self.assertEqual(self.empty_digest, bootstrap["baseline_digest"])

        opened = self._cli(
            "open",
            "--binding",
            str(self.binding_path),
            "--task",
            str(self.task_path),
            "--workspace",
            str(self.workspace),
        )
        self.assertEqual("blocked", opened["command_verdict"])
        self.assertTrue(self.workspace.is_dir())

        proposed = self._cli(
            "propose",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--delta",
            str(self.delta_path),
            "--task",
            str(self.task_path),
            "--engagement",
            str(self.engagement_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("passed", proposed["command_verdict"])
        candidate_sha256 = proposed["learning"]["candidate"]["candidate_sha256"]
        proposal_contexts = proposed["execution"]["transition_contexts"]
        self.assertEqual({"candidate", "proposed"}, set(proposal_contexts))
        for action in ("candidate", "proposed"):
            context = proposal_contexts[action]
            self.assertEqual(action, context["action"])
            self.assertEqual([action], context["envelope"]["requested_actions"])
            self.assertEqual(candidate_sha256, context["delta_sha256"])

        commit_authorization = write_json(
            self.root / "commit-authorization.json",
            native_authorization("commit", candidate_sha256),
        )
        committed = self._cli(
            "commit",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
            "--authorization",
            str(commit_authorization),
            "--task",
            str(self.task_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("committed", committed["learning"]["current_state"])
        committed_context = committed["execution"]["transition_context"]
        self.assertEqual("committed", committed_context["action"])
        self.assertEqual(
            ["committed"], committed_context["envelope"]["requested_actions"]
        )

        rejected_external_gate = self._cli(
            "verify",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
            "--task",
            str(self.task_path),
            "--regression-evidence",
            str(self.root / "caller-authored-gate.json"),
        )
        self.assertEqual("blocked", rejected_external_gate["command_verdict"])

        verified = self._cli(
            "verify",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
            "--task",
            str(self.task_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("passed", verified["command_verdict"])
        self.assertEqual("complete", verified["release"]["status"])
        suite = verified["execution"]["subject_execution_suite"]
        self.assertEqual("complete", suite["status"])
        self.assertEqual(
            committed["receipt"]["package_sha256"],
            suite["subject_package_sha256"],
        )
        verification_contexts = verified["execution"]["transition_contexts"]
        self.assertEqual(
            {
                "execute_candidate",
                "derive_regression_gate",
                "regression_passed",
                "derive_release_gate",
                "release_complete",
            },
            set(verification_contexts),
        )
        for action, context in verification_contexts.items():
            self.assertEqual(action, context["action"])
            self.assertEqual([action], context["envelope"]["requested_actions"])
        self.assertEqual(
            verification_contexts["execute_candidate"]["context_sha256"],
            suite["transition_context_sha256"],
        )
        regression_evidence = verified["regression"]["evidence"]
        release_evidence = verified["release"]["evidence"]
        self.assertEqual(
            verification_contexts["derive_regression_gate"]["context_sha256"],
            regression_evidence["transition_context_sha256"],
        )
        self.assertEqual(
            verification_contexts["derive_release_gate"]["context_sha256"],
            release_evidence["transition_context_sha256"],
        )
        native_verification = verified["execution"]["native_verification"]
        self.assertEqual(
            suite["suite_sha256"], native_verification["execution_suite_sha256"]
        )
        self.assertEqual(
            regression_evidence["evidence_sha256"],
            native_verification["regression_evidence"]["evidence_sha256"],
        )
        self.assertEqual(
            release_evidence["evidence_sha256"],
            native_verification["release_evidence"]["evidence_sha256"],
        )

        replayed = self._cli(
            "verify",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
            "--task",
            str(self.task_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("passed", replayed["command_verdict"])
        self.assertTrue(replayed["execution"]["recovered_from_immutable_evidence"])
        self.assertEqual(
            {
                action: verification_contexts[action]
                for action in ("derive_release_gate", "release_complete")
            },
            replayed["execution"]["transition_contexts"],
        )

        drifted_task = task_envelope(
            ["open", "run", "propose", "commit", "verify", "history", "promote"]
        )
        drifted_task["task_id"] = "task:test-drifted-replay"
        drifted_task["intent"] = "Attempt to replay release under a different task"
        drifted_task_path = write_json(self.root / "drifted-task.json", drifted_task)
        rejected_replay = self._cli(
            "verify",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
            "--task",
            str(drifted_task_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("blocked", rejected_replay["command_verdict"])
        self.assertEqual(
            "RefineryGateError", rejected_replay["execution"]["error_type"]
        )

        promote_authorization = write_json(
            self.root / "promote-authorization.json",
            native_authorization("promote", candidate_sha256),
        )
        promoted = self._cli(
            "promote",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
            "--authorization",
            str(promote_authorization),
            "--task",
            str(self.task_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("promoted", promoted["learning"]["current_state"])
        promoted_context = promoted["execution"]["transition_context"]
        self.assertEqual("promoted", promoted_context["action"])
        self.assertEqual(
            promoted_context["context_sha256"],
            promoted["execution"]["descriptor"]["transition_context_sha256"],
        )
        successor = promoted["learning"]["next_binding"]
        self.assertFalse(successor["auto_applied"])
        self.assertTrue(successor["requires_control_plane_approval"])
        self.assertEqual(original_binding_bytes, self.binding_path.read_bytes())
        successor_path = write_json(
            self.root / "approved-successor-binding.json", successor["document"]
        )
        successor_binding = engagement.read_project_binding(successor_path)

        found = self._cli(
            "discover",
            "--binding",
            str(successor_path),
            "--workspace",
            str(self.workspace),
        )
        subject = found["corpus_found"]["subject"]
        self.assertEqual(successor_binding.baseline_version, subject["version"])
        self.assertEqual(successor_binding.baseline_digest, subject["package_sha256"])

        executed = self._cli(
            "run",
            "--binding",
            str(successor_path),
            "--workspace",
            str(self.workspace),
            "--scenario",
            "scenario-current",
            "--task",
            str(self.task_path),
            "--recorded-at",
            RECORDED_AT,
        )
        self.assertEqual("passed", executed["command_verdict"])
        execution = executed["execution"]
        self.assertEqual(
            successor_binding.baseline_digest,
            execution["subject"]["package_sha256"],
        )
        self.assertEqual(
            execution["executor"]["digest"],
            execution["executor"]["receipt"]["package_digest"],
        )
        self.assertIn("execution_projection_sha256", execution["subject"])

        stale = self._cli(
            "discover",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
        )
        self.assertEqual("blocked", stale["command_verdict"])
        self.assertIn("subject", stale["execution"]["message"])

        historical = self._cli(
            "history",
            "--binding",
            str(self.binding_path),
            "--workspace",
            str(self.workspace),
            "--candidate",
            candidate_sha256,
        )
        self.assertEqual(
            list(engagement.STATE_MODEL), historical["learning"]["completed_states"]
        )
        self.assertEqual(6, len(historical["execution"]["events"]))
        events = historical["execution"]["events"]
        self.assertEqual(
            [
                proposal_contexts["candidate"]["context_sha256"],
                proposal_contexts["proposed"]["context_sha256"],
                committed_context["context_sha256"],
                verification_contexts["regression_passed"]["context_sha256"],
                verification_contexts["release_complete"]["context_sha256"],
                promoted_context["context_sha256"],
            ],
            [event["payload"]["transition_context_sha256"] for event in events],
        )


if __name__ == "__main__":
    unittest.main()
