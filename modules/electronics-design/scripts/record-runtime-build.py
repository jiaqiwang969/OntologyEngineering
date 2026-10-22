#!/usr/bin/env python3
"""Record build bytes and check offline action availability; no engineering approval."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import zipfile

root, patch = map(Path, sys.argv[1:])
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
version = "0.23.0-local.4"
assert (root / "logs/version.stdout").read_text().strip() == "easyeda-agent " + version
with zipfile.ZipFile(root / "connector.eext") as archive:
    meta = json.loads(archive.read("extension.json"))
    assert meta["version"] == version
    assert meta["activationEvents"]["onStartupFinished"] is False
    entry = archive.read("dist/index.js")
catalog = json.loads((root / "actions.json").read_text())
# The catalog shape is versioned by upstream; inspect each action definition.
actions = catalog["actions"] if isinstance(catalog, dict) else catalog
by_name = {x["name"]: x for x in actions}
for name in ("pcb.components.list", "pcb.export.netlist", "pcb.export.gerber", "project.export", "pcb.import_changes"):
    assert name in by_name, name
record = {
    "schema": "oev2.electronics-runtime-build/1",
    "status": "built-and-offline-tested",
    "version": version,
    "upstream_commit": subprocess.check_output(["git", "-C", str(root / "source"), "rev-parse", "HEAD"], text=True).strip(),
    "patch_sha256": sha(patch),
    "cli_sha256": sha(root / "bin/easyeda"),
    "connector_sha256": sha(root / "connector.eext"),
    "connector_entry_sha256": hashlib.sha256(entry).hexdigest(),
    "actions_sha256": sha(root / "actions.json"),
    "go": subprocess.check_output(["go", "version"], text=True).strip(),
    "node": subprocess.check_output(["node", "--version"], text=True).strip(),
    "npm": subprocess.check_output(["npm", "--version"], text=True).strip(),
    "native_installation": "not_run",
    "native_project_operations": "not_run",
    "engineering_acceptance": "not_granted",
}
(root / "BUILD.json").write_text(json.dumps(record, indent=2) + "\n")
