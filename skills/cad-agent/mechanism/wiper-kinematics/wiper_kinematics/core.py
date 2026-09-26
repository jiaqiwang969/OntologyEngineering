"""Core geometry translated from the archived MATLAB implementation.

The original ``CalcTransferCoor.m`` contains a symbolic expansion of a
circle/sphere intersection.  This module evaluates the equivalent compact
geometry and keeps the same first/second assembly-branch ordering.
"""

from __future__ import annotations

import math
from typing import Final

import numpy as np
from numpy.typing import ArrayLike, NDArray


FloatArray = NDArray[np.float64]
_EPS: Final[float] = np.finfo(float).eps


class GeometryDomainError(ValueError):
    """Raised when the requested linkage geometry has no real solution."""


def _vector3(value: ArrayLike, name: str) -> FloatArray:
    vector = np.asarray(value, dtype=float)
    if vector.shape != (3,):
        raise ValueError(f"{name} must have shape (3,), got {vector.shape}")
    if not np.isfinite(vector).all():
        raise ValueError(f"{name} must contain only finite values")
    return vector


def _points3(value: ArrayLike, name: str) -> FloatArray:
    points = np.asarray(value, dtype=float)
    if points.ndim == 1:
        points = points.reshape(3, 1)
    if points.ndim != 2 or points.shape[0] != 3:
        raise ValueError(f"{name} must have shape (3, n), got {points.shape}")
    if not np.isfinite(points).all():
        raise ValueError(f"{name} must contain only finite values")
    return points


def calibration_rotation_matrix(origin: ArrayLike, axis_point: ArrayLike) -> FloatArray:
    """Return the global-to-local rotation used by ``CalcRotMCalibration.m``.

    The third local basis vector follows ``axis_point - origin``.  For
    non-vertical axes the first two basis vectors exactly match the archived
    Z-then-X rotation construction.  A deterministic vertical-axis fallback
    removes the singularity present in the legacy formula.
    """

    p = _vector3(origin, "origin")
    pp = _vector3(axis_point, "axis_point")
    axis = pp - p
    axis_norm = float(np.linalg.norm(axis))
    if axis_norm <= 64.0 * _EPS:
        raise GeometryDomainError("origin and axis_point must be distinct")

    local_z = axis / axis_norm
    xy_norm = float(np.linalg.norm(axis[:2]))
    if xy_norm > 64.0 * _EPS * axis_norm:
        local_x = np.array([axis[1] / xy_norm, -axis[0] / xy_norm, 0.0])
    else:
        local_x = np.array([1.0, 0.0, 0.0])
    local_y = np.cross(local_z, local_x)
    local_y /= np.linalg.norm(local_y)
    return np.vstack((local_x, local_y, local_z))


def axis_rotation_matrix(axis: ArrayLike, angle_rad: float) -> FloatArray:
    """Rodrigues rotation matrix, corresponding to ``CalcRotMOnAxies.m``."""

    unit_axis = _vector3(axis, "axis")
    norm = float(np.linalg.norm(unit_axis))
    if norm <= 64.0 * _EPS:
        raise GeometryDomainError("rotation axis must be non-zero")
    unit_axis = unit_axis / norm
    x, y, z = unit_axis
    c = math.cos(float(angle_rad))
    s = math.sin(float(angle_rad))
    one_minus_c = 1.0 - c
    return np.array(
        [
            [x * x + (1.0 - x * x) * c, x * y * one_minus_c - z * s, x * z * one_minus_c + y * s],
            [x * y * one_minus_c + z * s, y * y + (1.0 - y * y) * c, y * z * one_minus_c - x * s],
            [x * z * one_minus_c - y * s, y * z * one_minus_c + x * s, z * z + (1.0 - z * z) * c],
        ],
        dtype=float,
    )


def matlab_colon(start: float, step: float, stop: float) -> FloatArray:
    """Create an inclusive MATLAB-style numeric range without endpoint drift."""

    start = float(start)
    step = float(step)
    stop = float(stop)
    if step == 0.0:
        raise ValueError("step must be non-zero")
    span = (stop - start) / step
    if span < 0.0:
        return np.empty(0, dtype=float)
    rounding_guard = 16.0 * _EPS * max(1.0, abs(span))
    count = int(math.floor(span + rounding_guard)) + 1
    return start + step * np.arange(count, dtype=float)


def circular_path(
    center: ArrayLike,
    axis_point: ArrayLike,
    radius: float,
    theta_rad: ArrayLike,
) -> FloatArray:
    """Generate a crank-pin circle in global coordinates."""

    center_vector = _vector3(center, "center")
    theta = np.asarray(theta_rad, dtype=float).reshape(-1)
    if not np.isfinite(theta).all():
        raise ValueError("theta_rad must contain only finite values")
    rotation = calibration_rotation_matrix(center_vector, axis_point)
    local = float(radius) * np.vstack(
        (np.cos(theta), np.sin(theta), np.zeros_like(theta))
    )
    return rotation.T @ local + center_vector[:, None]


def transfer_intersections(
    pivot: ArrayLike,
    pivot_axis_point: ArrayLike,
    arm_radius: float,
    link_length: float,
    input_points: ArrayLike,
    *,
    feasibility_tolerance_mm2: float = 1e-9,
) -> tuple[FloatArray, FloatArray, FloatArray]:
    """Return both assembly branches and their squared-height margin.

    In the output-arm local plane, ``Q=(x,y,0)`` must satisfy ``|Q|=R`` and
    ``|Q-N|=L``.  Subtracting the squared equations yields a line.  Its two
    intersections with the radius-``R`` circle are returned as branch 1 and
    branch 2, in the same order as ``CalcTransferCoor.m``.
    """

    p = _vector3(pivot, "pivot")
    points = _points3(input_points, "input_points")
    radius = float(arm_radius)
    length = float(link_length)
    if radius <= 0.0 or length <= 0.0:
        raise ValueError("arm_radius and link_length must be positive")

    rotation = calibration_rotation_matrix(p, pivot_axis_point)
    local_n = rotation @ (points - p[:, None])
    nx, ny, nz = local_n
    rho_sq = nx * nx + ny * ny
    scale = max(1.0, float(np.max(rho_sq)))
    if np.any(rho_sq <= 64.0 * _EPS * scale):
        raise GeometryDomainError("input point projects onto the output axis")

    rhs = 0.5 * (radius * radius + rho_sq + nz * nz - length * length)
    height_sq = radius * radius - rhs * rhs / rho_sq
    minimum_height_sq = float(np.min(height_sq))
    if minimum_height_sq < -abs(float(feasibility_tolerance_mm2)):
        raise GeometryDomainError(
            "linkage has no real circle/sphere intersection; "
            f"minimum height^2={minimum_height_sq:.12g} mm^2"
        )
    height = np.sqrt(np.maximum(height_sq, 0.0))
    rho = np.sqrt(rho_sq)

    foot_x = rhs * nx / rho_sq
    foot_y = rhs * ny / rho_sq
    perpendicular_scale = height / rho

    local_branch_1 = np.vstack(
        (
            foot_x - ny * perpendicular_scale,
            foot_y + nx * perpendicular_scale,
            np.zeros_like(nx),
        )
    )
    local_branch_2 = np.vstack(
        (
            foot_x + ny * perpendicular_scale,
            foot_y - nx * perpendicular_scale,
            np.zeros_like(nx),
        )
    )
    branch_1 = rotation.T @ local_branch_1 + p[:, None]
    branch_2 = rotation.T @ local_branch_2 + p[:, None]
    return branch_1, branch_2, height_sq


def select_branch(branches: tuple[FloatArray, FloatArray], branch: int) -> FloatArray:
    """Select MATLAB branch number 1 or 2."""

    if branch not in (1, 2):
        raise ValueError(f"branch must be 1 or 2, got {branch}")
    return branches[branch - 1]


def angular_position(
    pivot: ArrayLike, pivot_axis_point: ArrayLike, points: ArrayLike
) -> FloatArray:
    """Return angular position in ``[0, 2*pi)`` around an output axis."""

    p = _vector3(pivot, "pivot")
    q = _points3(points, "points")
    rotation = calibration_rotation_matrix(p, pivot_axis_point)
    local = rotation @ (q - p[:, None])
    return np.mod(np.arctan2(local[1], local[0]), 2.0 * np.pi)


def sweep_degrees(angle_rad: ArrayLike) -> float:
    """Return ``max(angle)-min(angle)`` in degrees, matching the legacy runs."""

    angle = np.asarray(angle_rad, dtype=float)
    if angle.size == 0 or not np.isfinite(angle).all():
        raise ValueError("angle_rad must be a non-empty finite array")
    return float(np.rad2deg(np.max(angle) - np.min(angle)))


def max_abs_distance_error(
    left: ArrayLike, right: ArrayLike, expected_distance: float
) -> float:
    """Maximum absolute Euclidean distance residual for paired point arrays."""

    a = _points3(left, "left")
    b = _points3(right, "right")
    if a.shape != b.shape:
        raise ValueError(f"left and right must have equal shapes, got {a.shape} and {b.shape}")
    distance = np.linalg.norm(a - b, axis=0)
    return float(np.max(np.abs(distance - float(expected_distance))))

