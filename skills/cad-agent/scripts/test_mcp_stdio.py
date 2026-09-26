"""Semantica transport and retired CAD rejection; no live model operation."""

import subprocess
import unittest
from unittest.mock import patch

import mcp_stdio as bridge


class MCPTransportTests(unittest.TestCase):
    def test_listing_does_not_start_cli(self):
        with patch.object(subprocess, "run", side_effect=AssertionError("no process")):
            self.assertEqual(len(bridge.tool_catalog("semantica")), 4)

    def test_retired_mode_cannot_list_or_dispatch_even_with_old_opt_in(self):
        with patch.dict(bridge.os.environ, {"CAD_AGENT_LEGACY_CAD": "explicit"}):
            with patch.object(subprocess, "run", side_effect=AssertionError("no process")):
                for action in (lambda: bridge.tool_catalog("fusion"),
                               lambda: bridge.create_server("fusion"),
                               lambda: bridge.invoke("fusion", "fusion_mcp_read", {"queryType":"projects"})):
                    with self.assertRaises(ValueError): action()

    def test_old_engine_and_lifecycle_mutations_not_exposed(self):
        for name in ("validate_graph", "validate_operation_plan", "semantic_promote", "semantic_commit"):
            with self.assertRaises(ValueError):
                bridge.build_command("semantica", name, {})

    def test_source_locked_cli_and_option_values(self):
        args = bridge.build_command("semantica", "semantic_discover", {"binding": "--workspace=/elsewhere"})
        self.assertEqual(args[0], str(bridge.SEMANTIC_PYTHON))
        self.assertEqual(args[-1], "--binding=--workspace=/elsewhere")

    def test_arbitrary_cli_options_rejected(self):
        with self.assertRaises(Exception):
            bridge.build_command("semantica", "semantic_doctor", {"backend": "legacy"})
        with self.assertRaises(Exception):
            bridge.build_command("semantica", "semantic_run", {"binding": "x"})

    def test_cad_result_cannot_masquerade_as_semantic_success(self):
        self.assertTrue(bridge.result_for("semantica", {"classification":"delivered_ok"}, 0).isError)
        with self.assertRaises(ValueError):
            bridge.result_for("fusion", {"classification":"delivered_ok"}, 0)

    def test_semantic_blocked_is_not_success(self):
        for verdict in ("blocked", "failed"):
            self.assertTrue(bridge.result_for("semantica", {"$schema": bridge.SEMANTIC_SCHEMA, "command_verdict": verdict}, 0).isError)
        self.assertTrue(bridge.result_for("semantica", {"command_verdict": "passed"}, 0).isError)

    def test_one_call_one_canonical_cli_and_environment(self):
        returned = subprocess.CompletedProcess([], 0, '{"$schema":"'+bridge.SEMANTIC_SCHEMA+'","command_verdict":"passed"}', '')
        with patch.dict(bridge.os.environ, {"CAD_AGENT_ROOT": "/stale", "PYTHONPATH": "/stale"}):
            with patch.object(subprocess, "run", return_value=returned) as run:
                result = bridge.invoke("semantica", "semantic_doctor", {})
        self.assertFalse(result.isError)
        run.assert_called_once()
        args, kwargs = run.call_args
        self.assertEqual(args[0][2], str(bridge.SEMANTIC_CLI))
        self.assertNotIn("timeout", kwargs)
        self.assertNotIn("shell", kwargs)
        self.assertNotIn("CAD_AGENT_ROOT", kwargs["env"])
        self.assertNotIn("PYTHONPATH", kwargs["env"])


if __name__ == "__main__":
    unittest.main()
