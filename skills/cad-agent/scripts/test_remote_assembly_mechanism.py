#!/usr/bin/env python3
"""Offline protocol and bounded geometry regressions for public CAD tools."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from remote import mcp_bridge as bridge, autocad_server as autocad  # noqa: E402
from assembly import nominal_solver as assembly  # noqa: E402
from mechanism import fourbar as mechanism  # noqa: E402


class TestBridge(unittest.TestCase):
    def test_ssh_command_quotes_paths_and_rejects_host_injection(self):
        profile = {"transport": "ssh_windows", "host": "cad-host.example",
                   "python": "C:\\Program Files\\Python\\python.exe",
                   "entry": {"module": "nx_mcp.server"},
                   "environment": {"NX_MCP_WORKSPACE": "C:\\client's data"}}
        command = bridge.command_for(profile)
        self.assertEqual(command[0], "ssh")
        self.assertEqual(command[6], "cad-host.example")
        self.assertNotIn("client's data", " ".join(command))
        profile["host"] = "example;whoami"
        with self.assertRaises(ValueError):
            bridge.command_for(profile)

    def test_one_shot_mcp_protocol(self):
        source = '''import json,sys
for line in sys.stdin:
 m=json.loads(line)
 if "id" not in m: continue
 if m["method"]=="initialize": r={"serverInfo":{"name":"fake"}}
 elif m["method"]=="tools/list": r={"tools":[{"name":"echo"}]}
 else: r={"content":[{"type":"text","text":json.dumps(m["params"]["arguments"])}]}
 print(json.dumps({"jsonrpc":"2.0","id":m["id"],"result":r}),flush=True)
'''
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory) / "fake.py"
            fake.write_text(source)
            profile = {"transport": "local_stdio", "argv": [sys.executable, str(fake)]}
            self.assertEqual(bridge.run(profile, None, {}, 3)["tools"][0]["name"], "echo")
            result = bridge.run(profile, "echo", {"x": 7}, 3)
            self.assertEqual(json.loads(result["content"][0]["text"]), {"x": 7})

    def test_autocad_protocol_without_com(self):
        response = autocad.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize"})
        self.assertEqual(response["result"]["serverInfo"]["name"], "oe-autocad-com")
        tools = autocad.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        self.assertIn("drawing_info", [tool["name"] for tool in tools["result"]["tools"]])


class TestGeometry(unittest.TestCase):
    def test_assembly_direction_and_precedence(self):
        example = json.loads((ROOT / "assembly/synthetic-example.json").read_text())
        result = assembly.solve(example)
        self.assertEqual(result["status"], "FEASIBLE_IN_DECLARED_DOMAIN")
        self.assertEqual(result["sequence"][0]["instance_id"], "cover")
        self.assertEqual(result["sequence"][0]["approach_from_final"], [0.0, 0.0, 3.0])
        example["instances"].append({"id": "roof", "initial": True,
                                     "box": {"min": [0, 0, 3], "max": [2, 2, 4]}})
        result = assembly.solve(example)
        self.assertEqual(result["status"], "FEASIBLE_IN_DECLARED_DOMAIN")
        self.assertEqual(result["sequence"][0]["approach_from_final"], [3.0, 0.0, 0.0])
        example["instances"][1]["approaches"] = [[0, 0, 3]]
        self.assertEqual(assembly.solve(example)["status"], "NO_PATH_IN_DECLARED_DOMAIN")

    def test_fourbar_closure_and_nonclosure(self):
        example = json.loads((ROOT / "mechanism/synthetic-example.json").read_text())
        result = mechanism.solve(example)
        self.assertEqual(len(result["samples"]), 6)
        self.assertTrue(all(sample["status"] == "CLOSED" for sample in result["samples"]))
        for sample in result["samples"]:
            for branch in sample["branches"]:
                self.assertLess(branch["coupler_residual"], 1e-12)
                self.assertLess(branch["rocker_residual"], 1e-12)
        self.assertEqual(mechanism.position(10, 1, 2, 2, 0)["status"], "NO_REAL_CLOSURE")


if __name__ == "__main__":
    unittest.main()
