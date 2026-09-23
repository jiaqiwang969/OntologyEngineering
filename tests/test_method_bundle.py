"""Round-trip real immutable method assets through the public CLI."""
import json
from pathlib import Path
import subprocess
import sys

from ontology_engineering.method_evidence import profiles

ROOT = Path(__file__).resolve().parents[1]


def test_native_rule_replay_keeps_stdout_machine_readable(tmp_path):
    spec, registry = profiles(ROOT)
    assert len(registry["profiles"]) == 19
    assert spec["scenario_count"] == 265
    output = tmp_path / "native-rule-replay"
    result = subprocess.run([sys.executable, str(ROOT / "scripts/run_methodology_cases.py"),
        "--run", "--scenario", "check-dependency-rules", "--output", str(output)],
        capture_output=True, text=True, check=True)
    summary = json.loads(result.stdout)
    assert summary["passed"] is True and summary["executed"] == 1
    receipt = json.loads((output / "check-dependency-rules.json").read_text())
    assert receipt["execution"]["status"] == "passed"
    assert receipt["verification"]["status"] == "complete"
