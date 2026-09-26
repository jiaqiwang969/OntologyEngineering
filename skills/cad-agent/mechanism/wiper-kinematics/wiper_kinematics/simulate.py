"""Dual-crank simulation, invariants, and historical MATLAB regression."""

from __future__ import annotations

import platform
from dataclasses import dataclass
from typing import Any, Final, Iterable

import numpy as np
from numpy.typing import NDArray

from .cases import CASES, DualCrankCase, get_case
from .core import (
    angular_position,
    calibration_rotation_matrix,
    circular_path,
    matlab_colon,
    max_abs_distance_error,
    select_branch,
    sweep_degrees,
    transfer_intersections,
)


FloatArray = NDArray[np.float64]
SWEEP_TOLERANCE_DEG: Final[float] = 1e-4
LENGTH_TOLERANCE_MM: Final[float] = 1e-9


@dataclass(frozen=True)
class SimulationArrays:
    theta_rad: FloatArray
    motor_pin_s: FloatArray
    motor_pin_l: FloatArray
    output_pin_s: FloatArray
    output_pin_l: FloatArray
    theta_s_rad: FloatArray
    theta_l_rad: FloatArray
    height_sq_s_mm2: FloatArray
    height_sq_l_mm2: FloatArray


def _orthogonality_error(matrix: FloatArray) -> float:
    identity = np.eye(3)
    return float(np.max(np.abs(matrix @ matrix.T - identity)))


def _simulate_arrays(case: DualCrankCase) -> SimulationArrays:
    omega_rad_s = case.motor_rpm * 2.0 * np.pi / 60.0
    step_rad = omega_rad_s * case.sample_period_s
    theta_rad = matlab_colon(0.0, step_rad, case.motor_turns * 2.0 * np.pi)

    motor_pin_s = circular_path(
        case.motor_1, case.motor_2, case.motor_radius_s_mm, theta_rad
    )
    reflected_motor_axis = tuple(
        2.0 * np.asarray(case.motor_2) - np.asarray(case.motor_1)
    )
    motor_pin_l = circular_path(
        case.motor_2,
        reflected_motor_axis,
        case.motor_radius_l_mm,
        theta_rad,
    )

    branch_s_1, branch_s_2, height_sq_s = transfer_intersections(
        case.pivot_s,
        case.pivot_s_axis,
        case.arm_radius_s_mm,
        case.link_length_s_mm,
        motor_pin_s,
    )
    branch_l_1, branch_l_2, height_sq_l = transfer_intersections(
        case.pivot_l,
        case.pivot_l_axis,
        case.arm_radius_l_mm,
        case.link_length_l_mm,
        motor_pin_l,
    )
    output_pin_s = select_branch((branch_s_1, branch_s_2), case.branch_s)
    output_pin_l = select_branch((branch_l_1, branch_l_2), case.branch_l)

    theta_s_rad = angular_position(case.pivot_s, case.pivot_s_axis, output_pin_s)
    theta_l_rad = angular_position(case.pivot_l, case.pivot_l_axis, output_pin_l)
    return SimulationArrays(
        theta_rad=theta_rad,
        motor_pin_s=motor_pin_s,
        motor_pin_l=motor_pin_l,
        output_pin_s=output_pin_s,
        output_pin_l=output_pin_l,
        theta_s_rad=theta_s_rad,
        theta_l_rad=theta_l_rad,
        height_sq_s_mm2=height_sq_s,
        height_sq_l_mm2=height_sq_l,
    )


def run_case(case_or_name: DualCrankCase | str) -> dict[str, Any]:
    """Run one case and return a JSON-serializable validation summary."""

    case = get_case(case_or_name) if isinstance(case_or_name, str) else case_or_name
    arrays = _simulate_arrays(case)
    sweep_s = sweep_degrees(arrays.theta_s_rad)
    sweep_l = sweep_degrees(arrays.theta_l_rad)

    motor_1 = np.asarray(case.motor_1, dtype=float)[:, None]
    motor_2 = np.asarray(case.motor_2, dtype=float)[:, None]
    pivot_s = np.asarray(case.pivot_s, dtype=float)[:, None]
    pivot_l = np.asarray(case.pivot_l, dtype=float)[:, None]

    crank_s_error = max_abs_distance_error(
        arrays.motor_pin_s,
        np.broadcast_to(motor_1, arrays.motor_pin_s.shape),
        case.motor_radius_s_mm,
    )
    crank_l_error = max_abs_distance_error(
        arrays.motor_pin_l,
        np.broadcast_to(motor_2, arrays.motor_pin_l.shape),
        case.motor_radius_l_mm,
    )
    arm_s_error = max_abs_distance_error(
        arrays.output_pin_s,
        np.broadcast_to(pivot_s, arrays.output_pin_s.shape),
        case.arm_radius_s_mm,
    )
    arm_l_error = max_abs_distance_error(
        arrays.output_pin_l,
        np.broadcast_to(pivot_l, arrays.output_pin_l.shape),
        case.arm_radius_l_mm,
    )
    link_s_error = max_abs_distance_error(
        arrays.output_pin_s, arrays.motor_pin_s, case.link_length_s_mm
    )
    link_l_error = max_abs_distance_error(
        arrays.output_pin_l, arrays.motor_pin_l, case.link_length_l_mm
    )
    maximum_length_error = max(
        crank_s_error,
        crank_l_error,
        arm_s_error,
        arm_l_error,
        link_s_error,
        link_l_error,
    )

    rotation_errors = [
        _orthogonality_error(calibration_rotation_matrix(case.motor_1, case.motor_2)),
        _orthogonality_error(
            calibration_rotation_matrix(
                case.motor_2,
                2.0 * np.asarray(case.motor_2) - np.asarray(case.motor_1),
            )
        ),
        _orthogonality_error(
            calibration_rotation_matrix(case.pivot_s, case.pivot_s_axis)
        ),
        _orthogonality_error(
            calibration_rotation_matrix(case.pivot_l, case.pivot_l_axis)
        ),
    ]

    error_s = sweep_s - case.historical_theta_s_deg
    error_l = sweep_l - case.historical_theta_l_deg
    all_finite = all(
        np.isfinite(value).all()
        for value in (
            arrays.theta_rad,
            arrays.motor_pin_s,
            arrays.motor_pin_l,
            arrays.output_pin_s,
            arrays.output_pin_l,
            arrays.theta_s_rad,
            arrays.theta_l_rad,
        )
    )
    passed = bool(
        all_finite
        and maximum_length_error <= LENGTH_TOLERANCE_MM
        and abs(error_s) <= SWEEP_TOLERANCE_DEG
        and abs(error_l) <= SWEEP_TOLERANCE_DEG
    )

    omega_rad_s = case.motor_rpm * 2.0 * np.pi / 60.0
    return {
        "case": case.name,
        "topology": "dual_crank",
        "source": {
            "artifact": case.source_artifact,
            "runtime_label": case.source_runtime,
            "historical_sweep_deg": {
                "theta_s": case.historical_theta_s_deg,
                "theta_l": case.historical_theta_l_deg,
            },
        },
        "sampling": {
            "sample_count": int(arrays.theta_rad.size),
            "motor_rpm": case.motor_rpm,
            "sample_period_s": case.sample_period_s,
            "motor_turns": case.motor_turns,
            "angular_step_rad": float(omega_rad_s * case.sample_period_s),
        },
        "computed_sweep_deg": {"theta_s": sweep_s, "theta_l": sweep_l},
        "historical_error_deg": {"theta_s": error_s, "theta_l": error_l},
        "invariants": {
            "all_finite_real": bool(all_finite),
            "max_abs_crank_radius_error_mm": max(crank_s_error, crank_l_error),
            "max_abs_output_arm_radius_error_mm": max(arm_s_error, arm_l_error),
            "max_abs_link_length_error_mm": max(link_s_error, link_l_error),
            "max_abs_any_length_error_mm": maximum_length_error,
            "max_rotation_orthogonality_error": max(rotation_errors),
            "min_intersection_height_sq_mm2": float(
                min(np.min(arrays.height_sq_s_mm2), np.min(arrays.height_sq_l_mm2))
            ),
        },
        "tolerances": {
            "historical_sweep_abs_deg": SWEEP_TOLERANCE_DEG,
            "length_closure_abs_mm": LENGTH_TOLERANCE_MM,
        },
        "passed": passed,
    }


def run_validation(case_names: Iterable[str] | None = None) -> dict[str, Any]:
    """Run selected cases (all by default) and return one JSON payload."""

    selected_names = list(CASES) if case_names is None else list(case_names)
    results = [run_case(name) for name in selected_names]
    return {
        "schema_version": 1,
        "implementation": {
            "language": "Python",
            "python_version": platform.python_version(),
            "numeric_backend": "NumPy",
            "numpy_version": np.__version__,
        },
        "results": results,
        "passed": bool(results and all(result["passed"] for result in results)),
    }

