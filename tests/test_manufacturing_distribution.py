from __future__ import annotations

import copy
import hashlib
import io
import json
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from semantic_bundle_transport import BundleError, load_bundle, validate_archive
from package_skill import delivery_issues, document_links


class ManufacturingDistributionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.spec, cls.payload, cls.report = load_bundle("manufacturing-process-cost")

    def altered(self, change):
        spec = copy.deepcopy(self.spec)
        entries = [(name, value, stat.S_IFREG | 0o644) for name, value in self.payload.items()]
        entries = change(entries, spec)
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, value, mode in entries:
                info = zipfile.ZipInfo(name)
                info.external_attr = mode << 16
                archive.writestr(info, value)
        data = stream.getvalue()
        spec["sha256"] = hashlib.sha256(data).hexdigest()
        return data, spec

    def test_locked_snapshot_has_complete_declared_scope(self):
        self.assertEqual(self.report["assets"], 141)
        self.assertEqual(len(self.report["scenarios"]), 68)
        self.assertEqual(self.report["files"], 143)

    def test_changed_archive_rejected_before_materialization(self):
        data = (ROOT / self.spec["path"]).read_bytes()
        with self.assertRaisesRegex(BundleError, "bundle hash"):
            validate_archive(data + b"extra", self.spec)

    def test_archive_path_traversal_rejected_even_with_new_container_hash(self):
        data, spec = self.altered(lambda entries, _: entries + [("../escape.json", b"{}", stat.S_IFREG | 0o644)])
        with self.assertRaisesRegex(BundleError, "inside its package"):
            validate_archive(data, spec)

    def test_unlisted_and_missing_members_rejected(self):
        for change, message in [
            (lambda entries, _: entries + [("extra.json", b"{}", stat.S_IFREG | 0o644)], "unlisted"),
            (lambda entries, _: entries[1:], "incomplete"),
        ]:
            with self.subTest(message=message):
                data, spec = self.altered(change)
                with self.assertRaisesRegex(BundleError, message):
                    validate_archive(data, spec)

    def test_duplicate_and_symbolic_members_rejected(self):
        changes = [
            (lambda entries, _: entries + entries[:1], "duplicate archive"),
            (lambda entries, _: [(entries[0][0], entries[0][1], stat.S_IFLNK | 0o777)] + entries[1:], "regular files"),
        ]
        for change, message in changes:
            with self.subTest(message=message):
                data, spec = self.altered(change)
                with self.assertRaisesRegex(BundleError, message):
                    validate_archive(data, spec)

    def test_native_manifest_still_checks_assets_after_transport_relocking(self):
        def change(entries, spec):
            name = next(n for n, _, _ in entries if n.endswith(".ttl"))
            value = self.payload[name] + b"\n# altered input\n"
            for row in spec["files"]:
                if row["path"] == name:
                    row["sha256"] = hashlib.sha256(value).hexdigest()
            return [(n, value if n == name else b, mode) for n, b, mode in entries]
        data, spec = self.altered(change)
        with self.assertRaisesRegex(BundleError, "native asset hash"):
            validate_archive(data, spec)

    def test_transport_can_be_checked_after_relocation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "copied skill with spaces"
            for relative in ["runtime/semantic-bundles.json", "runtime/semantica-source-lock.json", self.spec["path"]]:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / relative, target)
            _, _, report = load_bundle("manufacturing-process-cost", root)
            self.assertEqual(report, self.report)
            lock = root / "runtime/semantica-source-lock.json"
            value = json.loads(lock.read_text())
            value["source"]["commit"] = "0" * 40
            lock.write_text(json.dumps(value))
            with self.assertRaisesRegex(BundleError, "different source-locked runtime"):
                load_bundle("manufacturing-process-cost", root)

    def test_cli_does_not_overwrite_existing_outputs_or_accept_unknown_case(self):
        with tempfile.TemporaryDirectory() as temporary:
            for extra in [[], ["--scenario", "unknown-case"]]:
                result = subprocess.run([sys.executable, str(ROOT / "scripts/run_manufacturing_cases.py"), "--run", "--output", temporary, *extra], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertFalse(list(Path(temporary).iterdir()))

    def test_link_extraction_includes_images_html_and_reference_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            file = Path(temporary) / "readme.md"
            file.write_text('[a](../a.md)\n![img](image.png)\n<img src="preview.png">\n[ref]: guide.md\n```text\n[example](missing.md)\n```\n')
            self.assertEqual(document_links(file), ["../a.md", "image.png", "preview.png", "guide.md"])

    def test_received_delivery_detects_edited_or_extra_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            file = root / "SKILL.md"
            file.write_text("frozen method")
            manifest = {"format": "ontology-engineering.portable-skill/v1", "files": [{"path": "SKILL.md", "sha256": hashlib.sha256(file.read_bytes()).hexdigest()}]}
            (root / "PORTABLE-MANIFEST.json").write_text(json.dumps(manifest))
            self.assertEqual(delivery_issues(root, [file]), [])
            file.write_text("different method")
            self.assertIn("byte mismatch", delivery_issues(root, [file])[0]["reason"])
            extra = root / "extra.txt"
            extra.write_text("extra")
            self.assertIn("inventory", delivery_issues(root, [file, extra])[0]["reason"])


if __name__ == "__main__":
    unittest.main()
