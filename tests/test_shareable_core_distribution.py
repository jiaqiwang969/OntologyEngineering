"""Negative tests for the owner-approved public core allowlist."""

from __future__ import annotations

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import build_shareable_core as builder  # noqa: E402
from package_skill import check  # noqa: E402


class ShareableCoreDistributionTests(unittest.TestCase):
    def test_context_router_is_complete_and_runs_after_relocation(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary).resolve()
            stage = workspace / "installed skill"
            builder.stage(stage)
            self.assertEqual((stage / "VERSION").read_bytes(), (ROOT / "VERSION").read_bytes())
            self.assertEqual((stage / "VERSION").read_text().strip(),
                             json.loads(builder.ASSETS.read_text())["release_version"])
            for name in (
                "ontology_engineering/context_routing.py",
                "ontology_engineering/jev_transport.py",
                "references/context-routing-instructions.json",
                "references/context-capabilities.json",
                "references/context-routing.md",
                "scripts/route_engineering_task.py",
            ):
                self.assertEqual((stage / name).read_bytes(), (ROOT / name).read_bytes())
            context = workspace / "context.json"
            context.write_text(json.dumps({
                "schema": "ontology-engineering.context-input/v1",
                "task_id": "relocated-release-check",
                "context": "Synthetic engineering discussion; no customer data.",
                "request": "Identify the next evidence needed before changing the design.",
            }))
            output = workspace / "routing"
            result = subprocess.run([
                sys.executable, "-S", str(stage / "scripts/route_engineering_task.py"),
                "--input", str(context), "--output", str(output),
                "--credential-file", str(workspace / "intentionally-absent-credential"),
            ], cwd=workspace, capture_output=True, text=True)
            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads((output / "routing.json").read_text())
            self.assertEqual(report["status"], "unavailable")
            self.assertEqual(report["attempts"], 0)
            self.assertEqual(report["transport_errors"], ["credential_unavailable"])
            self.assertEqual(report["identity"]["instruction_sha256"], hashlib.sha256(
                (stage / "references/context-routing-instructions.json").read_bytes()).hexdigest())
            for route in report["routes"]:
                if route["capability_id"] != "external_specialist":
                    self.assertEqual(route["source"]["availability"], "source_present")
                    self.assertTrue(Path(route["source"]["path"]).is_relative_to(stage))

    def test_inert_entry_template_stages_to_approved_skill_name(self):
        ledger = deepcopy(json.loads(builder.ASSETS.read_text(encoding="utf-8")))
        entry = next(item for item in ledger["files"] if item["origin"] == "override" and item["path"] == "SKILL.md")
        data = b"synthetic approved entry template\n"
        entry["sha256"] = hashlib.sha256(data).hexdigest()
        ledger["files"] = [entry]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            overrides = root / "templates"; overrides.mkdir()
            (overrides / "SKILL.md.in").write_bytes(data)
            assets = root / "assets.json"; assets.write_text(json.dumps(ledger))
            target = root / "staged"
            with patch.object(builder, "ASSETS", assets), patch.object(builder, "OVERRIDES", overrides):
                result = builder.stage(target)
                self.assertEqual(result["asset_count"], 1)
                self.assertEqual((target / "SKILL.md").read_bytes(), data)
                self.assertFalse((target / "SKILL.md.in").exists())
                (overrides / "SKILL.md.in").write_bytes(b"changed unreviewed bytes")
                with self.assertRaisesRegex(ValueError, "changed since review"):
                    builder.stage(root / "changed")

    def modified_ledger(self, change) -> Path:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        ledger = deepcopy(json.loads(builder.ASSETS.read_text(encoding="utf-8")))
        change(ledger)
        path = Path(self.temporary.name) / "assets.json"
        path.write_text(json.dumps(ledger, ensure_ascii=False), encoding="utf-8")
        return path

    def test_curated_stage_is_closed_and_excludes_private_history(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            result = builder.stage(root)
            report, files = check(root)
            names = {file.relative_to(root).as_posix() for file in files}
            self.assertTrue(report["passed"], report["issues"])
            self.assertEqual(result["asset_count"], len(names))
            self.assertIn("skills/cad-agent/scripts/cad_process_handoff.py", names)
            self.assertFalse(any(name.startswith("skills/cad-agent/lessons-inbox/") for name in names))
            self.assertFalse(any(name.startswith("references/ontology-engineering-book/") for name in names))
            self.assertIn("skills/cad-agent/assembly/README.md", names)
            self.assertFalse(any(name.startswith("skills/cad-agent/assembly/cases/") for name in names))

    def test_retired_fusion_executor_cannot_reenter_current_core(self):
        ledger = self.modified_ledger(lambda document: document["files"].append({
            "path": "skills/cad-agent/dist/oe_cad_fusion_runtime-0.0.0-py3-none-any.whl"}))
        with patch.object(builder, "ASSETS", ledger), tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "retired Fusion execution assets"):
                builder.stage(Path(temporary))

    def test_changed_source_bytes_fail_closed(self):
        ledger = self.modified_ledger(lambda document: document["files"][0].update(sha256="0" * 64))
        with patch.object(builder, "ASSETS", ledger), tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "changed since review"):
                builder.stage(Path(temporary))

    def test_path_traversal_and_non_normal_paths_fail_closed(self):
        for bad in ("../escape", "a//b", "a\\b", "/absolute"):
            with self.subTest(path=bad):
                ledger = self.modified_ledger(lambda document: document["files"][0].update(path=bad))
                with patch.object(builder, "ASSETS", ledger), tempfile.TemporaryDirectory() as temporary:
                    with self.assertRaisesRegex(ValueError, "unsafe asset path"):
                        builder.stage(Path(temporary))

    def test_unreviewed_asset_fails_closed(self):
        ledger = self.modified_ledger(lambda document: document["files"][0].update(privacy_review="pending"))
        with patch.object(builder, "ASSETS", ledger), tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "review state"):
                builder.stage(Path(temporary))


if __name__ == "__main__":
    unittest.main()
