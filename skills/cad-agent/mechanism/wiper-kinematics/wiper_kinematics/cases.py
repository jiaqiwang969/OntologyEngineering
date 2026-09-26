"""Archived D003 and J71E validation parameters.

Values are transcribed from the complete MATLAB sources embedded in the
corresponding MATLAB Publish HTML files inside the supplied RAR archive.
Ownership and redistribution rights for those source artifacts remain to be
confirmed; see the directory README before sharing this data.
"""

from __future__ import annotations

from dataclasses import dataclass


Vector3 = tuple[float, float, float]


@dataclass(frozen=True)
class DualCrankCase:
    name: str
    source_artifact: str
    source_runtime: str
    motor_1: Vector3
    motor_2: Vector3
    pivot_s: Vector3
    pivot_s_axis: Vector3
    pivot_l: Vector3
    pivot_l_axis: Vector3
    motor_radius_s_mm: float
    motor_radius_l_mm: float
    arm_radius_s_mm: float
    arm_radius_l_mm: float
    link_length_s_mm: float
    link_length_l_mm: float
    branch_s: int
    branch_l: int
    motor_rpm: float
    sample_period_s: float
    motor_turns: float
    historical_theta_s_deg: float
    historical_theta_l_deg: float


_ARCHIVE_PREFIX = "20171225_Matlab code update/汽车四连杆(约定统一)"


CASES: dict[str, DualCrankCase] = {
    "D003": DualCrankCase(
        name="D003",
        source_artifact=f"{_ARCHIVE_PREFIX}/RESULT/D003/D003.html",
        source_runtime="MATLAB 7.11",
        motor_1=(1198.023, -358.066, 1100.243),
        motor_2=(1202.727, -357.251, 1091.684),
        pivot_s=(1288.778, -566.888, 1103.337),
        pivot_s_axis=(1281.524, -569.015, 1115.160),
        pivot_l=(1223.686, -133.697, 1117.062),
        pivot_l_axis=(1216.430, -133.334, 1129.030),
        motor_radius_s_mm=50.0,
        motor_radius_l_mm=50.0,
        arm_radius_s_mm=74.0269,
        arm_radius_l_mm=74.6916,
        link_length_s_mm=220.579,
        link_length_l_mm=219.162,
        branch_s=1,
        branch_l=2,
        motor_rpm=48.0,
        sample_period_s=0.5e-3,
        motor_turns=5.0,
        historical_theta_s_deg=84.3934,
        historical_theta_l_deg=84.2716,
    ),
    "J71E": DualCrankCase(
        name="J71E",
        source_artifact=f"{_ARCHIVE_PREFIX}/RESULT/J71E/J71E.html",
        source_runtime="MATLAB 7.11",
        motor_1=(1944.065, -377.697, 1024.767),
        motor_2=(1948.029, -376.617, 1015.870),
        pivot_s=(1964.934, -157.598, 1040.507),
        pivot_s_axis=(1926.428, -159.729, 1124.034),
        pivot_l=(2031.312, -604.811, 1036.173),
        pivot_l_axis=(2001.482, -615.845, 1100.768),
        motor_radius_s_mm=50.0,
        motor_radius_l_mm=50.0,
        arm_radius_s_mm=73.8061,
        arm_radius_l_mm=73.7668,
        link_length_s_mm=215.0,
        link_length_l_mm=237.722,
        branch_s=2,
        branch_l=1,
        motor_rpm=48.0,
        sample_period_s=0.5e-3,
        motor_turns=5.0,
        historical_theta_s_deg=84.8999,
        historical_theta_l_deg=85.5999,
    ),
}


def get_case(name: str) -> DualCrankCase:
    """Return a case by case-insensitive name."""

    normalized = name.upper()
    try:
        return CASES[normalized]
    except KeyError as exc:
        available = ", ".join(CASES)
        raise KeyError(f"unknown case {name!r}; available cases: {available}") from exc

