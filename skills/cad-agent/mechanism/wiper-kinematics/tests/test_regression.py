from __future__ import annotations

import json
import unittest

import numpy as np

from wiper_kinematics.cases import CASES
from wiper_kinematics.core import calibration_rotation_matrix
from wiper_kinematics.simulate import (
    LENGTH_TOLERANCE_MM,
    SWEEP_TOLERANCE_DEG,
    run_case,
    run_validation,
)


class HistoricalRegressionTests(unittest.TestCase):
    def test_all_cases_pass_historical_and_closure_gates(self) -> None:
        for name in CASES:
            with self.subTest(case=name):
                result = run_case(name)
                self.assertTrue(result["passed"])
                self.assertEqual(result["sampling"]["sample_count"], 12_501)
                self.assertTrue(result["invariants"]["all_finite_real"])
                self.assertLessEqual(
                    result["invariants"]["max_abs_any_length_error_mm"],
                    LENGTH_TOLERANCE_MM,
                )
                self.assertLessEqual(
                    abs(result["historical_error_deg"]["theta_s"]),
                    SWEEP_TOLERANCE_DEG,
                )
                self.assertLessEqual(
                    abs(result["historical_error_deg"]["theta_l"]),
                    SWEEP_TOLERANCE_DEG,
                )

    def test_calibration_frames_are_proper_rotations(self) -> None:
        for case in CASES.values():
            for origin, axis_point in (
                (case.motor_1, case.motor_2),
                (case.pivot_s, case.pivot_s_axis),
                (case.pivot_l, case.pivot_l_axis),
            ):
                with self.subTest(case=case.name, origin=origin):
                    rotation = calibration_rotation_matrix(origin, axis_point)
                    np.testing.assert_allclose(rotation @ rotation.T, np.eye(3), atol=1e-14)
                    self.assertAlmostEqual(float(np.linalg.det(rotation)), 1.0, places=14)

    def test_validation_payload_is_machine_readable_json(self) -> None:
        payload = run_validation()
        encoded = json.dumps(payload, allow_nan=False, sort_keys=True)
        decoded = json.loads(encoded)
        self.assertTrue(decoded["passed"])
        self.assertEqual([item["case"] for item in decoded["results"]], list(CASES))


if __name__ == "__main__":
    unittest.main()

