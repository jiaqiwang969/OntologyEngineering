# Conditional traction calculations

The existing `EngineeringCalculation` contract and `oe-kernel::calculation`
execute three explicit models. Existing Project model-boundary assertions check
their numerical inputs and required physical meanings. A computed value is a
model result; a declaration of its premises does not certify the physical setup.

| Method | Inputs and output | Meaning |
|---|---|---|
| `coulomb-single-contact-limit/1` | `friction_coefficient` in `1`, `normal_force` in N; `traction_limit` in N | Static Coulomb upper bound, μN, at one defined driven contact. It is neither actual traction nor a two-contact sum. |
| `coulomb-required-friction/1` | `tangential_force` in N, positive `normal_force` in N; `required_friction_coefficient` in `1` | F/N required to support the declared tangential demand. It does not look up or measure a material coefficient. |
| `steady-traction-power/1` | `tangential_force` in N, `linear_speed` in m/s; `mechanical_power` in W | Ideal contact power Fv for steady translation. Acceleration, transmission losses, motor sizing and thermal duty require further models and evidence. |

The input names and units are exact. Supplied values must be finite and
nonnegative; the divisor in F/N must be positive. Missing values stay missing.
Pressure, an added second contact, a safety factor, or speed in mm/s cannot be
silently substituted. Convert a source quantity explicitly before authoring the
calculation, retaining its original value, unit, source and interpretation.

Coulomb boundaries require `contact_model=single_driven_coulomb_contact`,
`normal_force_definition=resultant_normal_force_at_driven_contact` and
`friction_regime=static_limit`. The required-friction and power models also
require `tangential_force_definition=required_tangential_contact_force`.
Power requires `motion_regime=steady_translation` and
`power_boundary=ideal_contact_mechanical_power`. Unknown or inconsistent
Project conditions remain unknown or violated in the original kernel check.

For the exposed inCAD supplier-part development case, the supplier's 4 N and
“clamping pressure 10 N” do not yet establish these meanings. **If** 4 N is the
required contact force and 10 N is the resultant normal force at the driven
contact, the required coefficient is 0.4. **If** that force is maintained at
300 mm/s, the explicitly converted 0.3 m/s gives ideal contact power 1.2 W.
These conditional deductions establish neither sufficient grip nor an adequate
motor. Actual material, contamination, contact distribution, acceleration,
allowable film damage, manufacturing tolerances and measurement remain open.

Unknown friction remains unknown: the implementation never supplies a default
coefficient from a textbook, supplier label or a previously successful case.
