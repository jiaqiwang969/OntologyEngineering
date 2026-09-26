from __future__ import annotations

import json
import hashlib
from pathlib import Path
import shutil
import tempfile
import unittest

from scripts import check_semantica_backend_policy as gate


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
REPOSITORY_POLICY = REPOSITORY_ROOT / gate.DEFAULT_POLICY


class TemporaryRepository:
    def __init__(self) -> None:
        self._temporary = tempfile.TemporaryDirectory()
        self.root = Path(self._temporary.name)

    def close(self) -> None:
        self._temporary.cleanup()

    def write(self, relative: str, content: str, *, executable: bool = False) -> Path:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        if executable:
            path.chmod(path.stat().st_mode | 0o700)
        return path

    def policy(
        self, allowlist: list[dict[str, object]] | None = None, **extra: object
    ) -> Path:
        payload: dict[str, object] = {
            "schema_version": gate.SCHEMA_VERSION,
            "bootstrap": gate.REQUIRED_BOOTSTRAP,
            "literal_fixture_hosts": [],
            "allowlist": allowlist or [],
        }
        payload.update(extra)
        return self.write(gate.DEFAULT_POLICY, json.dumps(payload))

    def evaluate(self, mode: str = "audit") -> gate.GateReport:
        return gate.evaluate_repository(self.root, Path(gate.DEFAULT_POLICY), mode)


class SemanticaBackendRepositoryTests(unittest.TestCase):
    def test_bootstrap_location_is_a_fixed_repository_contract(self) -> None:
        self.assertEqual(
            "ontology_engineering/semantica_runtime.py",
            gate.REQUIRED_BOOTSTRAP,
        )

    def test_current_repository_audit_has_no_backend_debt(self) -> None:
        report = gate.evaluate_repository(REPOSITORY_ROOT, REPOSITORY_POLICY, "audit")
        self.assertTrue(report.passed)
        self.assertEqual([], report.findings)
        self.assertEqual([], report.unapproved_findings)
        self.assertEqual([], report.stale_allowances)
        self.assertEqual(0, report.debt_file_count)
        self.assertEqual([], report.allowlist)
        self.assertIn("scripts/check_semantica_backend_policy.py", report.fixture_hosts)
        self.assertIn("tests/test_semantica_backend_policy.py", report.fixture_hosts)
        self.assertGreaterEqual(report.scanned_by_kind["python"], 39)
        self.assertEqual(0, report.scanned_by_kind["java"])
        self.assertEqual(0, report.scanned_by_kind["semantic_asset"])

    def test_current_repository_strict_mode_is_green(self) -> None:
        report = gate.evaluate_repository(REPOSITORY_ROOT, REPOSITORY_POLICY, "strict")
        self.assertTrue(report.passed)
        self.assertEqual([], report.findings)
        self.assertEqual([], report.allowlist)
        self.assertEqual([], report.policy_errors)


class SemanticaBackendNegativeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.repo = TemporaryRepository()

    def tearDown(self) -> None:
        self.repo.close()

    def rules(self, report: gate.GateReport, path: str | None = None) -> set[str]:
        return {
            finding.rule
            for finding in report.findings
            if path is None or finding.path == path
        }

    def lock_cad_source(self, relative: str, content: str) -> None:
        self.repo.write("skills/cad-agent/SKILL.md", "# CAD execution module\n")
        fixture_dist = self.repo.root / "skills/cad-agent/dist"
        fixture_dist.mkdir(parents=True, exist_ok=True)
        # Synthetic inert legacy archive keeps historical rejection tests without
        # shipping or installing a retired CAD executor.
        from zipfile import ZipFile
        wheel = fixture_dist / "oe_cad_fusion_runtime-0.0.0-py3-none-any.whl"
        with ZipFile(wheel, "w") as archive:
            archive.writestr("fusion_mcp_proxy/__init__.py", "# inert fixture\n")
            archive.writestr("fixture.dist-info/entry_points.txt", "[console_scripts]\n")
            archive.writestr("fixture.dist-info/METADATA", "Name: fixture\nVersion: 0.0.0\n")
        (fixture_dist / "fusion-runtime-lock.json").write_text(json.dumps({
            "schema_version": gate.CAD_RUNTIME_LOCK_SCHEMA,
            "wheel_filename": wheel.name, "wheel_sha256": hashlib.sha256(wheel.read_bytes()).hexdigest()}))
        self.repo.write(relative, content)
        self.repo.write(
            gate.CAD_EXECUTION_LOCK,
            json.dumps({
                "schema_version": gate.CAD_EXECUTION_LOCK_SCHEMA,
                "scope": "Exact nonsemantic CAD subprocess only",
                "entries": [{
                    "path": relative,
                    "sha256": hashlib.sha256(content.encode("utf-8")).hexdigest(),
                    "rules": [gate.RULE_ALTERNATE_PROCESS],
                    "purpose": "Invoke the native CAD geometry process for evidence extraction.",
                }],
            }),
        )

    def test_public_cad_variant_requires_both_source_lock_and_export_approval(self) -> None:
        self.repo.write(gate.REQUIRED_BOOTSTRAP, "import semantica\n")
        relative = gate.CAD_PUBLIC_OVERRIDE_PREFIX + "scripts/native_call.py"
        content = "import subprocess\ndef run(command):\n    return subprocess.run(command)\n"
        self.lock_cad_source(relative, content)
        self.repo.policy()
        self.assertFalse(self.repo.evaluate("strict").passed)
        asset = {"path": "skills/cad-agent/scripts/native_call.py", "origin": "override",
                 "sha256": hashlib.sha256(content.encode()).hexdigest(),
                 "public_approval": "owner_approved", "has_personal_data": False}
        self.repo.write("distribution/shareable-core-assets.json", json.dumps({"files": [asset]}))
        self.assertTrue(self.repo.evaluate("strict").passed)
        self.repo.write(relative, content + "# changed after review\n")
        self.assertFalse(self.repo.evaluate("strict").passed)

    def test_public_cad_variant_cannot_hide_semantic_engine_even_if_relocked(self) -> None:
        self.repo.write(gate.REQUIRED_BOOTSTRAP, "import semantica\n")
        relative = gate.CAD_PUBLIC_OVERRIDE_PREFIX + "scripts/native_call.py"
        content = "import rdflib\nimport subprocess\ndef run(command):\n    return subprocess.run(command)\n"
        self.lock_cad_source(relative, content)
        self.repo.policy()
        self.repo.write("distribution/shareable-core-assets.json", json.dumps({"files": [{
            "path": "skills/cad-agent/scripts/native_call.py", "origin": "override",
            "sha256": hashlib.sha256(content.encode()).hexdigest(),
            "public_approval": "owner_approved", "has_personal_data": False}]}))
        report = self.repo.evaluate("strict")
        self.assertFalse(report.passed)
        self.assertIn(gate.RULE_DIRECT_BACKEND_IMPORT, self.rules(report, relative))

    def test_cad_execution_lock_is_exact_and_rejects_changed_source(self) -> None:
        self.repo.write(gate.REQUIRED_BOOTSTRAP, "import semantica\n")
        path = "skills/cad-agent/scripts/native_call.py"
        source = "import subprocess\n\ndef run(command):\n    return subprocess.run(command)\n"
        self.lock_cad_source(path, source)
        self.repo.policy()
        clean = self.repo.evaluate("strict")
        self.assertTrue(clean.passed, clean)
        self.assertEqual(1, len(clean.cad_locked_findings))
        self.repo.write(path, source + "import rdflib\n")
        changed = self.repo.evaluate("strict")
        self.assertFalse(changed.passed)
        self.assertIn(gate.RULE_DIRECT_BACKEND_IMPORT, self.rules(changed, path))
        self.assertTrue(any("source changed since review" in error for error in changed.policy_errors))

    def test_cad_execution_lock_cannot_hide_explicit_ontology_engine(self) -> None:
        self.repo.write(gate.REQUIRED_BOOTSTRAP, "import semantica\n")
        path = "skills/cad-agent/scripts/native_call.py"
        source = "import subprocess\nsubprocess.run(['python', '-m', 'rdflib'])\n"
        self.lock_cad_source(path, source)
        self.repo.policy()
        report = self.repo.evaluate("strict")
        self.assertFalse(report.passed)
        self.assertIn(gate.RULE_ALTERNATE_PROCESS, self.rules(report, path))

    def test_cad_runtime_wheel_hash_change_blocks_strict_gate(self) -> None:
        self.repo.write(gate.REQUIRED_BOOTSTRAP, "import semantica\n")
        self.lock_cad_source(
            "skills/cad-agent/scripts/native_call.py",
            "import subprocess\nsubprocess.run(['true'])\n",
        )
        self.repo.policy()
        lock = json.loads((self.repo.root / gate.CAD_RUNTIME_LOCK).read_text())
        wheel = self.repo.root / "skills/cad-agent/dist" / lock["wheel_filename"]
        wheel.write_bytes(wheel.read_bytes() + b"changed")
        report = self.repo.evaluate("strict")
        self.assertFalse(report.passed)
        self.assertTrue(any("wheel changed since review" in item for item in report.policy_errors))

    def test_cad_runtime_wheel_rejects_semantic_package_even_if_relocked(self) -> None:
        self.repo.write(gate.REQUIRED_BOOTSTRAP, "import semantica\n")
        self.lock_cad_source(
            "skills/cad-agent/scripts/native_call.py",
            "import subprocess\nsubprocess.run(['true'])\n",
        )
        self.repo.policy()
        lock_path = self.repo.root / gate.CAD_RUNTIME_LOCK
        lock = json.loads(lock_path.read_text())
        wheel = self.repo.root / "skills/cad-agent/dist" / lock["wheel_filename"]
        from zipfile import ZipFile
        with ZipFile(wheel, "a") as archive:
            archive.writestr("cad_geometry_mcp/server.py", "import rdflib\n")
        lock["wheel_sha256"] = hashlib.sha256(wheel.read_bytes()).hexdigest()
        lock_path.write_text(json.dumps(lock))
        report = self.repo.evaluate("strict")
        self.assertFalse(report.passed)
        self.assertTrue(any("non-Fusion package" in item for item in report.policy_errors))

    def direct_nx_fixture(self):
        self.repo.write(gate.REQUIRED_BOOTSTRAP, "import semantica\n")
        self.repo.write("skills/cad-agent/SKILL.md", "# NX direct\n")
        self.repo.write("skills/cad-agent/scripts/nx_direct.py", "# inert test source\n")
        self.repo.write(gate.CAD_EXECUTION_LOCK, json.dumps({
            "schema_version": gate.CAD_EXECUTION_LOCK_SCHEMA, "entries": []}))
        policy = {"schema": "cad-agent.operational-execution-policy/v1",
                  "default_cad": "NXDirect", "cad_mcp_enabled": False,
                  "removed_cad_systems": ["Fusion"], "legacy_cad_reactivation_allowed": False,
                  "automatic_legacy_fallback": False,
                  "semantic_authority": "parent-source-locked-Semantica"}
        self.repo.write(gate.CAD_DIRECT_POLICY, json.dumps(policy))
        self.repo.policy()
        return policy

    def test_direct_nx_needs_no_retired_wheel_and_still_scans_source(self):
        self.direct_nx_fixture()
        self.assertTrue(self.repo.evaluate("strict").passed)
        self.repo.write("skills/cad-agent/scripts/nx_direct.py", "import rdflib\n")
        report = self.repo.evaluate("strict")
        self.assertFalse(report.passed)
        self.assertIn(gate.RULE_DIRECT_BACKEND_IMPORT, self.rules(report))

    def test_direct_nx_rejects_reintroduced_retired_assets_and_policy(self):
        policy = self.direct_nx_fixture()
        self.repo.write("skills/cad-agent/scripts/fusion_call.py", "# retired\n")
        self.assertFalse(self.repo.evaluate("strict").passed)
        (self.repo.root/"skills/cad-agent/scripts/fusion_call.py").unlink()
        policy["automatic_legacy_fallback"] = True
        self.repo.write(gate.CAD_DIRECT_POLICY, json.dumps(policy))
        self.assertFalse(self.repo.evaluate("strict").passed)

    def test_applescript_control_is_not_a_forward_rule_but_predicates_are(self):
        script = 'if application "Chrome" is not running then error "closed"'
        self.assertIsNone(gate._semantic_payload_kind(script))
        self.assertEqual(gate._semantic_payload_kind(script + "\nif Equipment(?x) then Asset(?x)"),
                         "forward-rule program")

    def test_clean_strict_repository_and_single_bootstrap_pass(self) -> None:
        self.repo.write(
            gate.REQUIRED_BOOTSTRAP,
            "import semantica\n\ndef runtime_version():\n    return semantica.__version__\n",
        )
        self.repo.write(
            "consumer.py",
            "from ontology_engineering.semantica_runtime import runtime_version\n",
        )
        self.repo.policy()
        report = self.repo.evaluate("strict")
        self.assertTrue(report.passed, report)
        self.assertEqual([], report.findings)

    def test_every_alternate_python_backend_import_is_blocked(self) -> None:
        for module in sorted(gate.BACKEND_MODULES):
            with self.subTest(module=module):
                self.repo.write("app.py", f"import {module}\n")
                self.repo.policy()
                report = self.repo.evaluate()
                self.assertIn(
                    gate.RULE_DIRECT_BACKEND_IMPORT, self.rules(report, "app.py")
                )

    def test_semantica_import_is_legal_only_in_immutable_bootstrap(self) -> None:
        self.repo.write("application.py", "from semantica.reasoning import Reasoner\n")
        self.repo.write(
            gate.REQUIRED_BOOTSTRAP, "from semantica.reasoning import Reasoner\n"
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(
            gate.RULE_DIRECT_SEMANTICA_IMPORT, self.rules(report, "application.py")
        )
        self.assertNotIn(
            gate.RULE_DIRECT_SEMANTICA_IMPORT,
            self.rules(report, gate.REQUIRED_BOOTSTRAP),
        )

    def test_embedded_semantic_payloads_are_non_allowlistable(self) -> None:
        self.repo.write(
            "embedded.py",
            '''\
turtle = """@prefix ex: <urn:test:> .\nex:item ex:state ex:Ready ."""
query = "SELECT ?item WHERE { ?item ?p ?o }"
shape = "sh:ThingShape a sh:NodeShape ."
rule = "IF Equipment(?x) THEN Asset(?x)"
''',
        )
        self.repo.policy(
            allowlist=[
                {
                    "path": "embedded.py",
                    "rules": [gate.RULE_EMBEDDED_SEMANTIC_PAYLOAD],
                    "reason": "An embedded graph must never remain outside Semantica packages.",
                    "expires_when": "This negative fixture is removed after gate verification.",
                }
            ]
        )
        report = self.repo.evaluate()
        findings = [
            item
            for item in report.findings
            if item.rule == gate.RULE_EMBEDDED_SEMANTIC_PAYLOAD
        ]
        self.assertEqual(4, len(findings))
        self.assertTrue(
            any("non-allowlistable" in error for error in report.policy_errors)
        )

    def test_importlib_aliases_computed_names_getattr_and_dunder_import_are_blocked(
        self,
    ) -> None:
        self.repo.write(
            "dynamic.py",
            """\
import importlib as il
from importlib import import_module as load

module_name = "rd" + "flib"
il.import_module(module_name)
load(f"py{'shacl'}")
getattr(il, "import_" + "module")("owl" + "ready2")
loader = __import__
loader("pyoxi" + "graph")
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        dynamic = [
            finding
            for finding in report.findings
            if finding.path == "dynamic.py" and finding.rule == gate.RULE_DYNAMIC_IMPORT
        ]
        self.assertEqual(4, len(dynamic))

    def test_exec_eval_and_compile_are_fail_closed(self) -> None:
        self.repo.write(
            "evaluated.py",
            """\
payload = "import " + "rdflib"
exec(payload)
eval("__import__('pyshacl')")
compile(payload, "<memory>", "exec")
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        dynamic = [
            item for item in report.findings if item.rule == gate.RULE_DYNAMIC_IMPORT
        ]
        self.assertEqual(3, len(dynamic))

    def test_private_store_backend_access_variants_are_blocked(self) -> None:
        self.repo.write(
            "private.py",
            """\
first = engine._store_backend
second = getattr(engine, "_store" + "_backend")
third = engine.__dict__["_store_backend"]
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        private = [
            item for item in report.findings if item.rule == gate.RULE_PRIVATE_BACKEND
        ]
        self.assertEqual(3, len(private))

    def test_reflective_and_mapping_evasions_are_blocked(self) -> None:
        self.repo.write(
            "reflection.py",
            """\
import importlib
import operator
backend_name = "".join(["_store", "_backend"])
reader = operator.attrgetter(backend_name)
loader = importlib.__dict__["import_" + "module"]
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(gate.RULE_PRIVATE_BACKEND, self.rules(report, "reflection.py"))
        self.assertIn(gate.RULE_DYNAMIC_IMPORT, self.rules(report, "reflection.py"))

    def test_sparql_reasoner_import_and_computed_lookup_are_blocked(self) -> None:
        self.repo.write(
            "reasoner.py",
            """\
from semantica.reasoning import SPARQLReasoner
reasoner_type = getattr(api, "SPARQL" + "Reasoner")
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        reasoner = [
            item for item in report.findings if item.rule == gate.RULE_SPARQL_REASONER
        ]
        self.assertEqual(2, len(reasoner))

    def test_subprocess_alias_and_computed_engine_command_are_blocked(self) -> None:
        self.repo.write(
            "runner.py",
            """\
import subprocess as process
command = ["python", "-m", "rd" + "flib"]
launch = process.run
launch(command)
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(gate.RULE_ALTERNATE_PROCESS, self.rules(report, "runner.py"))

    def test_subprocess_cannot_hide_behind_backend_violating_helper(self) -> None:
        self.repo.write("legacy.py", "import rdflib\n")
        self.repo.write(
            "runner.py",
            """\
from pathlib import Path
import subprocess
tool = Path("legacy.py")
subprocess.run(["python", str(tool)])
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(gate.RULE_ALTERNATE_PROCESS, self.rules(report, "runner.py"))

    def test_scanned_clean_helper_through_current_python_is_not_an_alternate_engine(
        self,
    ) -> None:
        self.repo.write(
            "clean.py",
            "from ontology_engineering.semantica_runtime import create_runtime\n",
        )
        self.repo.write(
            "runner.py",
            """\
from pathlib import Path
import subprocess
import sys
tool = Path("clean.py")
subprocess.run([sys.executable, str(tool)])
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertNotIn(gate.RULE_ALTERNATE_PROCESS, self.rules(report, "runner.py"))

    def test_subprocess_cannot_hide_behind_backend_violating_shell_helper(self) -> None:
        self.repo.write("legacy.sh", "#!/usr/bin/env bash\npython -m rdflib\n")
        self.repo.write(
            "runner.py",
            """\
import subprocess
subprocess.run(["bash", "legacy.sh"])
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(gate.RULE_ALTERNATE_PROCESS, self.rules(report, "runner.py"))

    def test_unknown_process_command_fails_closed_but_static_git_is_safe(self) -> None:
        self.repo.write(
            "processes.py",
            """\
import subprocess

def unsafe(command):
    subprocess.run(command)

safe_command = ["git", "status", "--short"]
subprocess.run(safe_command)
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        process_findings = [
            item for item in report.findings if item.rule == gate.RULE_ALTERNATE_PROCESS
        ]
        self.assertEqual(1, len(process_findings))
        self.assertEqual(4, process_findings[0].line)

    def test_java_import_reflection_and_comment_handling(self) -> None:
        self.repo.write(
            "src/Client.java",
            """\
// import org.apache.jena.query.Query;
class Client {
  Object load() throws Exception {
    return Class.forName("org." + "apache.jena.query.Query");
  }
}
""",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(gate.RULE_DYNAMIC_IMPORT, self.rules(report, "src/Client.java"))
        self.assertIn(gate.RULE_JENA_CLIENT, self.rules(report, "src/Client.java"))
        self.assertTrue(all(finding.line != 1 for finding in report.findings))

    def test_shell_and_extensionless_shebang_surfaces_are_scanned(self) -> None:
        self.repo.write("run.sh", "#!/usr/bin/env bash\npython -m pyshacl graph.ttl\n")
        self.repo.write(
            "bin/ontology-runner",
            "#!/usr/bin/env python3\nimport pyoxigraph\n",
            executable=True,
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(gate.RULE_ALTERNATE_PROCESS, self.rules(report, "run.sh"))
        self.assertIn(
            gate.RULE_DIRECT_BACKEND_IMPORT, self.rules(report, "bin/ontology-runner")
        )

    def test_powershell_and_ci_workflow_commands_are_scanned(self) -> None:
        self.repo.write("tools/run.ps1", "python -m owlready2\n")
        self.repo.write(
            ".github/workflows/bypass.yml",
            "jobs:\n  bypass:\n    steps:\n      - run: python -m pyoxigraph\n",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(gate.RULE_ALTERNATE_PROCESS, self.rules(report, "tools/run.ps1"))
        self.assertIn(
            gate.RULE_ALTERNATE_PROCESS,
            self.rules(report, ".github/workflows/bypass.yml"),
        )

    def test_nested_vendor_and_reference_directories_are_not_omitted(self) -> None:
        self.repo.write(
            "vendor/reference/deep/backend.py", "from rdflib import Graph\n"
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(
            gate.RULE_DIRECT_BACKEND_IMPORT,
            self.rules(report, "vendor/reference/deep/backend.py"),
        )

    def test_dependency_manifests_are_execution_policy_surfaces(self) -> None:
        self.repo.write("requirements-prod.txt", "service-lib==1\npyshacl>=0.25\n")
        self.repo.write(
            "pom.xml",
            "<dependency><groupId>org.apache.jena</groupId><artifactId>jena-arq</artifactId></dependency>\n",
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertIn(
            gate.RULE_DIRECT_BACKEND_DEPENDENCY,
            self.rules(report, "requirements-prod.txt"),
        )
        self.assertIn(gate.RULE_JENA_CLIENT, self.rules(report, "pom.xml"))

    def test_duplicate_semantic_assets_are_migration_debt_and_strict_blockers(
        self,
    ) -> None:
        for relative in (
            "model/domain.ttl",
            "model/domain.owl",
            "queries/check.rq",
            "queries/check.sparql",
            "shapes/release.shacl",
        ):
            self.repo.write(relative, "# frozen executable semantic fixture\n")
        self.repo.policy()
        report = self.repo.evaluate("strict")
        self.assertFalse(report.passed)
        semantic_findings = [
            finding
            for finding in report.findings
            if finding.rule == gate.RULE_DUPLICATE_SEMANTIC_ASSET
        ]
        self.assertEqual(5, len(semantic_findings))

    def test_exact_allowance_passes_audit_but_never_strict(self) -> None:
        self.repo.write("legacy.py", "import rdflib\n")
        self.repo.policy(
            [
                {
                    "path": "legacy.py",
                    "rules": [gate.RULE_DIRECT_BACKEND_IMPORT],
                    "reason": "Legacy graph adapter has not yet reached its scheduled migration.",
                    "expires_when": "Remove once legacy.py imports only ontology_engineering.semantica_runtime.",
                }
            ]
        )
        audit = self.repo.evaluate("audit")
        strict = self.repo.evaluate("strict")
        self.assertTrue(audit.passed)
        self.assertEqual(1, len(audit.allowed_findings))
        self.assertFalse(strict.passed)
        self.assertEqual(1, len(strict.unapproved_findings))

    def test_stale_allowance_is_rejected(self) -> None:
        self.repo.write(
            "migrated.py", "from ontology_engineering.semantica_runtime import graph\n"
        )
        self.repo.policy(
            [
                {
                    "path": "migrated.py",
                    "rules": [gate.RULE_DIRECT_BACKEND_IMPORT],
                    "reason": "This deliberately stale entry verifies automatic exception retirement.",
                    "expires_when": "Remove immediately after the direct backend import disappears.",
                }
            ]
        )
        report = self.repo.evaluate()
        self.assertFalse(report.passed)
        self.assertEqual(
            ["migrated.py: direct_backend_import"], report.stale_allowances
        )

    def test_globs_and_directory_exclusion_configuration_are_rejected(self) -> None:
        self.repo.write("legacy.py", "import rdflib\n")
        self.repo.policy(
            [
                {
                    "path": "vendor/*.py",
                    "rules": [gate.RULE_DIRECT_BACKEND_IMPORT],
                    "reason": "Wildcard exception must be rejected even when its explanation looks valid.",
                    "expires_when": "Wildcard exception must never be accepted by this backend gate.",
                }
            ],
            excluded_directories=["vendor"],
        )
        report = self.repo.evaluate()
        self.assertFalse(report.passed)
        self.assertTrue(
            any("exclusions are forbidden" in error for error in report.policy_errors)
        )
        self.assertTrue(any("without globs" in error for error in report.policy_errors))

    def test_literal_fixture_classification_cannot_hide_application_code(self) -> None:
        self.repo.write("application.py", "name = '_store_backend'\n")
        self.repo.policy(
            literal_fixture_hosts=[
                {
                    "path": "application.py",
                    "scope": "string_literals_only",
                    "reason": "An application is not permitted to claim negative-test fixture status.",
                }
            ]
        )
        report = self.repo.evaluate()
        self.assertFalse(report.passed)
        self.assertIn(gate.RULE_PRIVATE_BACKEND, self.rules(report, "application.py"))
        self.assertTrue(
            any("not gate infrastructure" in error for error in report.policy_errors)
        )

    def test_active_source_directory_symlink_fails_closed(self) -> None:
        outside = tempfile.TemporaryDirectory()
        self.addCleanup(outside.cleanup)
        outside_path = Path(outside.name)
        (outside_path / "hidden.py").write_text("import rdflib\n", encoding="utf-8")
        (self.repo.root / "linked-source").symlink_to(
            outside_path, target_is_directory=True
        )
        self.repo.policy()
        report = self.repo.evaluate()
        self.assertFalse(report.passed)
        self.assertIn(gate.RULE_UNSAFE_SYMLINK, self.rules(report, "linked-source"))

    def test_python_parse_failure_cannot_be_allowlisted(self) -> None:
        self.repo.write("broken.py", "def broken(:\n")
        self.repo.policy(
            [
                {
                    "path": "broken.py",
                    "rules": [gate.RULE_PARSE_FAILURE],
                    "reason": "A syntax failure cannot be used to conceal an alternate backend import.",
                    "expires_when": "The source must parse before the backend gate can ever pass.",
                }
            ]
        )
        report = self.repo.evaluate()
        self.assertFalse(report.passed)
        self.assertIn(gate.RULE_PARSE_FAILURE, self.rules(report, "broken.py"))
        self.assertTrue(
            any("non-allowlistable" in error for error in report.policy_errors)
        )


if __name__ == "__main__":
    unittest.main()
