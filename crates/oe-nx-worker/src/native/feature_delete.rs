//! NX 2412 reviewed UF feature/list ABI. One selected feature, no replacement
//! geometry, feature-name lookup, journal, retry or input-asset save.
use super::*;
use oe_contracts::{NxFeatureDeleteOutcome1, NxFeatureInventory1, NxFeatureObservation1};

impl Native {
    pub(super) fn feature_delete_context(&self, part: Tag, body: Tag) -> Result<()> {
        ensure!(
            part != 0 && body != 0 && unsafe { (self.uf.work_part)() } == part,
            "feature_delete_current_work_part_differs"
        );
        let (mut owner, mut kind, mut object_type, mut subtype, mut units) = (0, -1, -1, -1, -1);
        self.check(
            unsafe { (self.uf.owning_part)(body, &mut owner) },
            "feature_delete_body_owner",
        )?;
        self.check(
            unsafe { (self.uf.object_type)(body, &mut object_type, &mut subtype) },
            "feature_delete_body_type",
        )?;
        self.check(
            unsafe { (self.uf.body_type)(body, &mut kind) },
            "feature_delete_body_kind",
        )?;
        self.check(
            unsafe { (self.uf.units)(part, &mut units) },
            "feature_delete_part_units",
        )?;
        ensure!(
            owner == part
                && object_type == self.abi.solid_type
                && subtype == 0
                && kind == self.abi.solid_body
                && unsafe { (self.uf.is_occurrence)(body) } == 0,
            "feature_delete_not_current_prototype_solid"
        );
        // Unit enumeration is interpreted by the existing part_info/assembly
        // reader; this context check merely requires the native query succeed.
        Ok(())
    }
    pub(super) fn feature_delete_diagnostic(&mut self, value: Value) -> Result<()> {
        ensure!(
            queue_load_observation(
                &mut self.load_observations,
                &mut self.load_observation_bytes,
                value
            )?,
            "feature_delete_diagnostic_overflow"
        );
        Ok(())
    }
    pub(super) fn current_feature_inventory(
        &mut self,
        part: u64,
        body: u64,
    ) -> Result<NxFeatureInventory1> {
        let (part, body) = (Tag::try_from(part)?, Tag::try_from(body)?);
        self.feature_delete_context(part, body)?;
        let mut list = null_mut();
        let code = unsafe { (self.uf.body_features)(body, &mut list) };
        let tags = self.owned_list(list, code, "UF_MODL_ask_body_feats");
        self.feature_delete_diagnostic(
            json!({"operation":"UF_MODL_ask_body_feats", "part_tag":part,
            "body_tag":body,"return_code":code,"feature_tags":tags.as_ref().ok(),
            "list_error":tags.as_ref().err().map(|e|format!("{e:#}"))}),
        )?;
        let tags = tags?;
        ensure!(
            tags.len() <= crate::geometry::feature_delete::MAX_FEATURES,
            "feature_delete_inventory_budget"
        );
        let mut features = Vec::new();
        for feature in tags {
            let (mut owner, mut feature_body) = (0, 0);
            let owner_code = unsafe { (self.uf.owning_part)(feature, &mut owner) };
            let body_code = unsafe { (self.uf.feature_body)(feature, &mut feature_body) };
            let mut raw_type = null_mut();
            let type_code = unsafe { (self.uf.feature_type)(feature, &mut raw_type) };
            let _allocation = Allocation {
                p: raw_type.cast(),
                free: self.uf.free,
            };
            let text = if type_code == 0 {
                unsafe { allocated_text(raw_type, 1024) }
            } else {
                Err(anyhow::anyhow!("feature_type_native_failed"))
            };
            let mut location = [f64::NAN; 3];
            let location_code =
                unsafe { (self.uf.feature_location)(feature, location.as_mut_ptr()) };
            let mut face_list = null_mut();
            let faces_code = unsafe { (self.uf.feature_faces)(feature, &mut face_list) };
            let faces = self.owned_list(face_list, faces_code, "UF_MODL_ask_feat_faces");
            let occurrence = unsafe { (self.uf.is_occurrence)(feature) } != 0;
            let query_codes = vec![
                owner_code as i64,
                body_code as i64,
                type_code as i64,
                location_code as i64,
                faces_code as i64,
            ];
            let complete = query_codes == [0, 0, 0, 0, 0]
                && text.is_ok()
                && faces.is_ok()
                && location.iter().all(|n| n.is_finite());
            self.feature_delete_diagnostic(json!({"operation":"feature_delete_feature_queries","part_tag":part,"body_tag":body,
                "feature_tag":feature,"owning_part_tag":owner,"queried_body_tag":feature_body,"is_occurrence":occurrence,
                "query_codes":query_codes,"feature_type":text.as_ref().ok(),"type_error":text.as_ref().err().map(|e|format!("{e:#}")),
                "location_binary64":crate::geometry::mass_probe::words(&location),"face_tags":faces.as_ref().ok(),
                "face_list_error":faces.as_ref().err().map(|e|format!("{e:#}")),"complete":complete}))?;
            features.push(NxFeatureObservation1 {
                feature_tag: feature as u64,
                owning_part_tag: owner as u64,
                body_tag: feature_body as u64,
                is_occurrence: occurrence,
                feature_type: text.unwrap_or_default(),
                location_mm: if location.iter().all(|n| n.is_finite()) {
                    location.to_vec()
                } else {
                    vec![]
                },
                face_tags: faces
                    .unwrap_or_default()
                    .into_iter()
                    .map(u64::from)
                    .collect(),
                query_codes,
                complete,
            });
        }
        Ok(NxFeatureInventory1 {
            schema: crate::geometry::feature_delete::INVENTORY_SCHEMA.into(),
            part_tag: part as u64,
            body_tag: body as u64,
            complete: features.iter().all(|f| f.complete),
            features,
        })
    }
    pub(super) fn delete_current_feature(
        &mut self,
        part: u64,
        body: u64,
        feature: u64,
    ) -> Result<NxFeatureDeleteOutcome1> {
        let (p, b, f) = (
            Tag::try_from(part)?,
            Tag::try_from(body)?,
            Tag::try_from(feature)?,
        );
        self.feature_delete_context(p, b)?;
        ensure!(
            f != 0 && unsafe { (self.uf.is_occurrence)(f) } == 0,
            "feature_delete_not_prototype_feature"
        );
        let (mut owner, mut feature_body) = (0, 0);
        self.check(
            unsafe { (self.uf.owning_part)(f, &mut owner) },
            "feature_delete_current_owner",
        )?;
        self.check(
            unsafe { (self.uf.feature_body)(f, &mut feature_body) },
            "feature_delete_current_body",
        )?;
        ensure!(
            owner == p && feature_body == b,
            "feature_delete_current_identity_differs"
        );
        ensure!(
            self.load_observation_bytes <= 512 * 1024 - 16 * 1024,
            "feature_delete_diagnostic_capacity_before_call"
        );
        let mut o = NxFeatureDeleteOutcome1 {
            schema: crate::geometry::feature_delete::OUTCOME_SCHEMA.into(),
            part_tag: part,
            body_tag: body,
            feature_tag: feature,
            call_entered: false,
            call_returned: false,
            return_code: -1,
            list_create_code: -1,
            list_put_code: -1,
            list_count_code: -1,
            list_item_code: -1,
            list_delete_code: -1,
            list_count: 0,
            list_item_tag: 0,
            update_code: None,
            complete: false,
        };
        let mut list = null_mut();
        o.list_create_code = unsafe { (self.uf.list_create)(&mut list) } as i64;
        if o.list_create_code == 0 && !list.is_null() {
            o.list_put_code = unsafe { (self.uf.list_put)(list, f) } as i64;
            if o.list_put_code == 0 {
                let mut count = -1;
                o.list_count_code = unsafe { (self.uf.list_count)(list, &mut count) } as i64;
                o.list_count = u64::try_from(count).unwrap_or(0);
                if o.list_count_code == 0 && count == 1 {
                    let mut item = 0;
                    o.list_item_code = unsafe { (self.uf.list_item)(list, 0, &mut item) } as i64;
                    o.list_item_tag = item as u64;
                    if o.list_item_code == 0 && item == f {
                        o.call_entered = true;
                        o.return_code = unsafe { (self.uf.feature_delete)(list) } as i64;
                        o.call_returned = true;
                    }
                }
            }
        }
        // This releases the input list, not its objects. Never retry a failed
        // deletion or cleanup. No fallible Rust operation occurs between create
        // and cleanup, so every normal return frees its allocated list once.
        if !list.is_null() {
            o.list_delete_code = unsafe { (self.uf.list_delete)(&mut list) } as i64;
        }
        if o.call_returned && o.return_code == 0 {
            o.update_code = Some(unsafe { (self.uf.motion_update)() } as i64);
        }
        o.complete = o.call_returned && o.list_delete_code == 0 && o.update_code.is_some();
        self.feature_delete_diagnostic(json!({"operation":"UF_MODL_delete_feature","outcome":o}))?;
        Ok(o)
    }
}
