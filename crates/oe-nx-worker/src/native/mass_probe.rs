//! Existing UF ABI, exact result words, no model edit or alternate geometry checker.
use super::*;
use oe_contracts::NxGeometricalMassObservation1;

impl Native {
    pub(super) fn probe_geometrical_mass(
        &mut self,
        body: u64,
        owning_part: u64,
    ) -> Result<NxGeometricalMassObservation1> {
        let body = Tag::try_from(body)?;
        let part = Tag::try_from(owning_part)?;
        ensure!(body != 0 && part != 0, "native_mass_probe_identity_invalid");
        let (mut owner, mut kind, mut object_type, mut subtype) = (0, -1, -1, -1);
        self.check(
            unsafe { (self.uf.owning_part)(body, &mut owner) },
            "native_mass_probe_owner",
        )?;
        self.check(
            unsafe { (self.uf.object_type)(body, &mut object_type, &mut subtype) },
            "native_mass_probe_type",
        )?;
        self.check(
            unsafe { (self.uf.body_type)(body, &mut kind) },
            "native_mass_probe_body_type",
        )?;
        ensure!(
            owner == part
                && object_type == self.abi.solid_type
                && subtype == 0
                && kind == self.abi.solid_body
                && unsafe { (self.uf.is_occurrence)(body) } == 0,
            "native_mass_probe_not_current_prototype_solid"
        );
        let mut input = [0.; 11];
        input[0] = 0.999999;
        let requested = input;
        let mut raw = [0.; 47];
        let mut statistics = [0.; 13];
        let mut object = body;
        let return_code = unsafe {
            (self.uf.mass_props)(
                &mut object,
                1,
                1,
                4,
                0.,
                1,
                input.as_mut_ptr(),
                raw.as_mut_ptr(),
                statistics.as_mut_ptr(),
            )
        };
        // Keep the raw buffers even for a failed return; the caller records before
        // validating. Bit words preserve NaN/Inf as diagnostic data without JSON coercion.
        Ok(NxGeometricalMassObservation1 {
            schema: crate::geometry::mass_probe::OBSERVATION_SCHEMA.into(),
            body_tag: body as u64,
            owning_part_tag: owner as u64,
            analysis_type: 1,
            units: 4,
            density: 0.,
            accuracy_mode: 1,
            accuracy_values: requested.to_vec(),
            return_code: return_code.into(),
            raw47_binary64: crate::geometry::mass_probe::words(&raw),
            statistics13_binary64: crate::geometry::mass_probe::words(&statistics),
        })
    }
}
