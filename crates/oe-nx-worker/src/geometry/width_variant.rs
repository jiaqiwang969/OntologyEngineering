//! Explicit, task-owned imported-assembly width changes. No automatic part
//! classification, scaling, hole-pitch policy, or manufacturing approval.
use super::{NativeApi, Workspace, motion_task};
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, NxAssemblyBudget, NxAssemblySourceClosure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

pub const ACTION: &str = "maintain_width_variant";
pub const PARAMS_SCHEMA: &str = "oev2.nx-width-variant-params/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Face {
    pub tag: u32,
    pub kind: i32,
    pub point: [f64; 3],
    pub direction: [f64; 3],
    pub bounds: [f64; 6],
    pub radius: f64,
    pub normal: i32,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stretch {
    pub prototype: String,
    pub axis: usize,
    pub plane: f64,
    pub negative_mm: f64,
    pub positive_mm: f64,
    pub expected_body_count: usize,
    #[serde(default)]
    pub body_indices: Vec<usize>,
    #[serde(default)]
    pub protected_interval: Option<[f64; 2]>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyTranslation {
    prototype: String,
    body_index: usize,
    expected_bounds_mm: [f64; 6],
    delta_mm: [f64; 3],
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub occurrence: String,
    pub expected_origin: [f64; 3],
    pub origin: [f64; 3],
}
#[derive(Debug, Clone, Serialize)]
pub struct BodyPair {
    pub a_occurrence:String,
    pub a_body:usize,
    pub b_occurrence:String,
    pub b_body:usize,
    pub expected_touch:bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SupportPlacement {
    name:String,
    origin_mm:[f64;3],
    contacts:Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Support {
    prototype:String,
    size_mm:[f64;3],
    wall_mm:f64,
    hole_pattern_mm:[f64;2],
    hole_diameter_mm:f64,
    placements:Vec<SupportPlacement>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema: String,
    mode: String,
    source: NxAssemblySourceClosure,
    stretches: Vec<Stretch>,
    placements: Vec<Placement>,
    #[serde(default)]
    body_translations: Vec<BodyTranslation>,
    inspect_prototypes: Vec<String>,
    export_step: bool,
    #[serde(default)]
    support:Option<Support>,
    #[serde(default)]
    step_tolerance_mm:Option<f64>,
    source_refs: Vec<String>,
}

fn world_box(b:&oe_contracts::NxAssemblySolidObservation,m:&[f64])->Result<[f64;6]> {
    ensure!(m.len()==16,"width_world_transform_invalid");
    let mut out=[f64::INFINITY;6];out[3..].fill(f64::NEG_INFINITY);
    for p in b.exact_box_corners_native.chunks_exact(3) {for k in 0..3 {let x=m[k*4]*p[0]+m[k*4+1]*p[1]+m[k*4+2]*p[2]+1000.*m[k*4+3];out[k]=out[k].min(x);out[k+3]=out[k+3].max(x);}}
    Ok(out)
}

fn add_support(api:&mut dyn NativeApi,w:&mut dyn Workspace,root:u32,namespace:&std::path::Path,s:&Support)->Result<()> {
    let [x,y,z]=s.size_mm;let t=s.wall_mm;let [px,py]=s.hole_pattern_mm;let dia=s.hole_diameter_mm;
    ensure!(s.prototype.ends_with(".prt")&&s.prototype.len()<=128&&!s.prototype.contains(['/', '\\', ':'])&&s.prototype.is_ascii(),"width_support_filename_invalid");
    ensure!([x,y,z,t,px,py,dia].iter().all(|x|x.is_finite()&&*x>0.&&*x<1000.)&&x>2.*t&&z>2.*t&&px+dia<x-2.*t&&py+dia<y,"width_support_dimensions_invalid");
    ensure!(!s.placements.is_empty()&&s.placements.len()<=16,"width_support_placement_budget");
    let path=namespace.join(&s.prototype);ensure!(!path.exists(),"width_support_part_already_exists");
    let part=api.create_mm_part(&path)?;api.set_work_part(part)?;
    let mut features=vec![api.create_block([-x/2.,-y/2.,0.],s.size_mm.map(|v|v.to_string()))?];
    features.push(api.create_subtractive_block([-x/2.+t,-y/2.,t],[(x-2.*t).to_string(),y.to_string(),(z-2.*t).to_string()])?);
    for hx in [-px/2.,px/2.] {for hy in [-py/2.,py/2.] {features.push(api.create_subtractive_cylinder([hx,hy,0.],z,dia)?);}}
    api.save_work_part()?;
    w.record("width_support_created",json!({"prototype":s.prototype,"part_tag":part,"features":features,"size_mm":s.size_mm,"wall_mm":t,"hole_pattern_mm":s.hole_pattern_mm,"hole_diameter_mm":dia,"expected_volume_m3":(2.*x*y*t+2.*t*y*(z-2.*t)-4.*std::f64::consts::PI*(dia/2.).powi(2)*2.*t)*1e-9,"material_and_load_qualification":"not_assigned"}))?;
    api.prepare_owned_assembly_context(root,namespace)?;
    let mut names=BTreeSet::new();
    for p in &s.placements {
        ensure!(names.insert(&p.name)&&!p.name.is_empty()&&p.name.is_ascii()&&p.origin_mm.iter().all(|x|x.is_finite())&&!p.contacts.is_empty(),"width_support_placement_invalid");
        api.add_assembly_entire_instance_nxopen(root,part,&path,&p.name,p.origin_mm,&[1.,0.,0.,0.,1.,0.,0.,0.,1.])?;
        w.record("width_support_placed",json!({"name":p.name,"prototype":s.prototype,"origin_mm":p.origin_mm,"contacts":p.contacts}))?;
    }
    Ok(())
}

/// Select complete native faces in the declared end region. Faces crossing
/// the split plane, including a center hole, are not swept to either side.
pub fn end_faces(faces: &[Face], axis: usize, plane: f64, side: i32) -> Result<Vec<u32>> {
    ensure!(axis < 3 && plane.is_finite() && matches!(side, -1 | 1), "width_selection_invalid");
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for face in faces {
        ensure!(face.tag != 0 && seen.insert(face.tag), "width_face_identity_invalid");
        ensure!(face.bounds.iter().all(|v| v.is_finite()), "width_face_bounds_nonfinite");
        for a in 0..3 { ensure!(face.bounds[a] <= face.bounds[a+3], "width_face_bounds_reversed"); }
        if (side < 0 && face.bounds[axis+3] < plane - 1e-6)
            || (side > 0 && face.bounds[axis] > plane + 1e-6) {
            result.push(face.tag);
        }
    }
    ensure!(!result.is_empty(), "width_end_region_has_no_complete_faces");
    Ok(result)
}

fn bounds(corners: &[f64]) -> Result<[f64; 6]> {
    ensure!(corners.len() == 24 && corners.iter().all(|v| v.is_finite()), "width_box_invalid");
    let mut b = [f64::INFINITY;6];
    b[3..].fill(f64::NEG_INFINITY);
    for p in corners.chunks_exact(3) { for a in 0..3 { b[a]=b[a].min(p[a]); b[a+3]=b[a+3].max(p[a]); } }
    Ok(b)
}

fn selected_end(faces:&[Face],s:&Stretch,side:i32)->Result<Vec<u32>> {
    let selected=end_faces(faces,s.axis,s.plane,side)?;
    if let Some([lo,hi])=s.protected_interval {
        ensure!(lo.is_finite() && hi.is_finite() && lo<=s.plane && s.plane<=hi,"width_protected_interval_invalid");
        let filtered=selected.into_iter().filter(|tag| {
            let f=faces.iter().find(|f|f.tag==*tag).unwrap();
            !(f.bounds[s.axis]>=lo && f.bounds[s.axis+3]<=hi)
        }).collect::<Vec<_>>();
        ensure!(!filtered.is_empty(),"width_end_region_fully_protected");
        Ok(filtered)
    } else { Ok(selected) }
}

pub fn execute(api: &mut dyn NativeApi, w: &mut dyn Workspace, reference: &ArtifactReference, budget: &NxAssemblyBudget) -> Result<Value> {
    let plan: Plan = motion_task::read(w, reference, 16*1024*1024)?;
    ensure!(plan.schema == "oev2.nx-width-variant-intent/1" && matches!(plan.mode.as_str(), "inspect"|"apply"), "width_intent_profile_invalid");
    ensure!(!plan.source_refs.is_empty() && plan.stretches.len() <= 512 && plan.placements.len() <= 10000 && plan.inspect_prototypes.len() <= 2048, "width_intent_budget_or_provenance_invalid");
    ensure!(plan.body_translations.len()<=2000,"width_body_translation_budget");
    if plan.mode == "inspect" { ensure!(plan.stretches.is_empty() && plan.placements.is_empty() && plan.body_translations.is_empty() && plan.support.is_none() && !plan.export_step, "width_inspection_must_be_read_only"); }
    crate::motion_source::validate_source_closure(&plan.source)?;
    let (path, namespace) = w.prepare_motion_source(&plan.source)?;
    let root=api.open_part(&path)?;
    let before=api.full_assembly_info(root,&namespace,budget,"reference_observation")?;
    let loaded: Vec<_>=before.prototypes.iter().map(|p| p.prototype_id.clone()).collect();
    motion_task::check_loaded(&loaded,&plan.source)?;
    ensure!(before.prototypes.iter().all(|p| p.native_units=="mm" && p.fully_loaded), "width_requires_fully_loaded_mm_prototypes");
    w.record("width_before",serde_json::to_value(&before)?)?;
    let mut names=BTreeSet::new();
    for name in &plan.inspect_prototypes {
        ensure!(names.insert(name), "width_inspect_duplicate");
        let p=before.prototypes.iter().find(|p| &p.prototype_id==name).context("width_inspect_prototype_missing")?;
        for (i,b) in p.solids.iter().enumerate() {
            let faces=api.width_face_inventory(u32::try_from(b.body_tag)?)?;
            w.record("width_face_inventory",json!({"prototype":name,"body_index":i,"solid":b,"faces":faces}))?;
        }
    }
    let mut expected:BTreeMap<u64,([f64;6],bool)>=BTreeMap::new();
    for p in &before.prototypes {for b in &p.solids {ensure!(expected.insert(b.body_tag,(bounds(&b.exact_box_corners_native)?,false)).is_none(),"width_duplicate_native_body");}}
    let mut edited=BTreeSet::new();
    for t in &plan.body_translations {
        ensure!(t.delta_mm.iter().chain(&t.expected_bounds_mm).all(|v|v.is_finite()) && t.delta_mm.iter().all(|v|v.abs()<=10000.),"width_body_translation_invalid");
        let p=before.prototypes.iter().find(|p|p.prototype_id==t.prototype).context("width_body_translation_prototype_missing")?;
        let b=p.solids.get(t.body_index).context("width_body_translation_index_missing")?;
        ensure!(edited.insert(b.body_tag),"width_body_edited_twice");
        let prior=bounds(&b.exact_box_corners_native)?;
        ensure!(prior.iter().zip(t.expected_bounds_mm).all(|(a,b)|(a-b).abs()<1e-5),"width_body_translation_source_changed");
        w.record("width_body_translate_intent",json!({"prototype":t.prototype,"body_index":t.body_index,"body_tag":b.body_tag,"delta_mm":t.delta_mm,"before_bounds_mm":prior}))?;
        api.set_work_part(u32::try_from(p.native_part_tag)?)?;
        let result=api.width_translate_body(u32::try_from(p.native_part_tag)?,u32::try_from(b.body_tag)?,t.delta_mm)?;
        w.record("width_body_translate_returned",result)?;
        let mut next=prior;for a in 0..3 {next[a]+=t.delta_mm[a];next[a+3]+=t.delta_mm[a];}
        expected.insert(b.body_tag,(next,false));
    }
    let mut changed=BTreeSet::new();
    for s in &plan.stretches {
        ensure!(changed.insert(&s.prototype) && s.axis<3 && [s.plane,s.negative_mm,s.positive_mm].iter().all(|v|v.is_finite()) && s.negative_mm<=0. && s.positive_mm>=0. && s.positive_mm-s.negative_mm>0. && s.positive_mm-s.negative_mm<=10000., "width_stretch_invalid");
        let p=before.prototypes.iter().find(|p|p.prototype_id==s.prototype).context("width_stretch_prototype_missing")?;
        ensure!(p.solids.len()==s.expected_body_count && !p.solids.is_empty() && p.sheet_count==0,"width_stretch_body_scope_differs");
        api.set_work_part(u32::try_from(p.native_part_tag)?)?;
        let indices=if s.body_indices.is_empty(){(0..p.solids.len()).collect::<Vec<_>>()}else{s.body_indices.clone()};
        ensure!(indices.iter().all(|i|*i<p.solids.len()) && indices.iter().copied().collect::<BTreeSet<_>>().len()==indices.len(),"width_stretch_indices_invalid");
        for (i,b) in p.solids.iter().enumerate() {
            if !indices.contains(&i){continue;}
            ensure!(edited.insert(b.body_tag),"width_body_edited_twice");
            let prior=bounds(&b.exact_box_corners_native)?;
            ensure!(prior[s.axis]<s.plane && prior[s.axis+3]>s.plane,"width_split_outside_body");
            let baseline_faces=api.width_face_inventory(u32::try_from(b.body_tag)?)?;
            let union_box=|faces:&[Face]|{let mut bb=[f64::INFINITY;6];bb[3..].fill(f64::NEG_INFINITY);for f in faces{for a in 0..3{bb[a]=bb[a].min(f.bounds[a]);bb[a+3]=bb[a+3].max(f.bounds[a+3]);}}bb};
            let face_box_before=union_box(&baseline_faces);
            for (side,amount) in [(-1,s.negative_mm),(1,s.positive_mm)] {
                if amount==0. {continue;}
                let faces=api.width_face_inventory(u32::try_from(b.body_tag)?)?;
                let selected=selected_end(&faces,s,side)?;
                w.record("width_face_move_intent",json!({"prototype":s.prototype,"body_index":i,"axis":s.axis,"plane":s.plane,"amount_mm":amount,"selected":selected,"faces":faces}))?;
                let mut d=[0.;3];d[s.axis]=amount;
                let result=api.width_move_faces(u32::try_from(p.native_part_tag)?,&selected,d).map_err(|e|anyhow::anyhow!("width_move_failed:{}:body{i}:side{side}: {e:#}",s.prototype))?;
                w.record("width_face_move_returned",result)?;
            }
            let observed_faces=api.width_face_inventory(u32::try_from(b.body_tag)?)?;
            let face_box_after=union_box(&observed_faces);let mut face_box_expected=face_box_before;face_box_expected[s.axis]+=s.negative_mm;face_box_expected[s.axis+3]+=s.positive_mm;
            w.record("width_body_face_extent_readback",json!({"prototype":s.prototype,"body_index":i,"before_face_box":face_box_before,"after_face_box":face_box_after,"expected_face_box":face_box_expected,"exact_body_check":"required before save"}))?;
            let mut next=prior;next[s.axis]+=s.negative_mm;next[s.axis+3]+=s.positive_mm;
            let exact=api.width_exact_body_bounds(u32::try_from(b.body_tag)?)?;
            w.record("width_body_exact_extent_readback",json!({"prototype":s.prototype,"body_index":i,"expected_bounds_mm":next,"actual_bounds_mm":exact,"face_boxes":"diagnostic only; may be conservative, especially curved surfaces"}))?;
            ensure!(exact.iter().zip(next).all(|(a,b)|(a-b).abs()<1e-4),"width_commit_exact_extent_mismatch:{}:body{i}",s.prototype);
            expected.insert(b.body_tag,(next,true));
        }
    }
    let mut placements:BTreeMap<u64,([f64;3],Vec<f64>)>=BTreeMap::new();
    for p in &plan.placements {
        ensure!(p.expected_origin.iter().chain(&p.origin).all(|v|v.is_finite()),"width_placement_nonfinite");
        let n=before.occurrences.iter().find(|n|n.occurrence_id==p.occurrence).context("width_occurrence_missing")?;
        ensure!(n.raw_parent_origin.len()==3 && n.raw_parent_csys.len()==9,"width_placement_source_shape");
        ensure!(n.raw_parent_origin.iter().zip(p.expected_origin).all(|(a,b)|(a-b).abs()<1e-6),"width_placement_source_changed");
        if let Some((other,csys))=placements.get(&n.instance_tag) {
            ensure!(other.iter().zip(p.origin).all(|(a,b)|(a-b).abs()<1e-6) && csys==&n.raw_parent_csys,"width_shared_instance_needs_distinct_prototype");
        } else {placements.insert(n.instance_tag,(p.origin,n.raw_parent_csys.clone()));}
    }
    for (tag,(origin,csys)) in &placements {
        w.record("width_instance_place_intent",json!({"instance_tag":tag,"origin_mm":origin,"csys":csys}))?;
        let result=api.width_place_instance(u32::try_from(*tag)?,*origin,csys)?;
        w.record("width_instance_place_returned",result)?;
    }
    if let Some(s)=&plan.support {
        ensure!(before.prototypes.iter().all(|p|p.prototype_id!=s.prototype),"width_support_prototype_collision");
        add_support(api,w,root,&namespace,s)?;
    }
    let after=api.full_assembly_info(root,&namespace,budget,"reference_observation")?;
    if plan.support.is_some(){w.record("width_support_assembly_observed",serde_json::to_value(&after)?)?;}
    let added_prototypes=usize::from(plan.support.is_some());let added_instances=plan.support.as_ref().map_or(0,|s|s.placements.len());
    ensure!(before.prototypes.len()+added_prototypes==after.prototypes.len() && before.occurrences.len()+added_instances==after.occurrences.len(),"width_assembly_scope_changed");
    for a in &before.prototypes {
        let b=after.prototypes.iter().find(|p|p.prototype_id==a.prototype_id).context("width_changed_prototype_missing")?;
        ensure!(a.solids.len()==b.solids.len(),"width_solid_count_changed");
        for old in &a.solids {
            let new=b.solids.iter().find(|v|v.body_tag==old.body_tag).context("width_native_body_identity_changed")?;
            let got=bounds(&new.exact_box_corners_native)?;let (wanted,stretched)=expected[&old.body_tag];
            ensure!(got.iter().zip(wanted).all(|(g,e)|(g-e).abs()<1e-4),"width_native_extent_does_not_match_plan:{}:body{}",a.prototype_id,old.body_tag);
            if !stretched {let (old,new)=(old.volume_m3.context("width_missing_volume")?,new.volume_m3.context("width_missing_volume")?);ensure!((old-new).abs()<=old.abs()*1e-8+1e-15,"width_rigid_or_unchanged_body_volume_changed");}
        }
    }
    for (tag,(origin,_)) in &placements {
        for n in after.occurrences.iter().filter(|n|n.instance_tag==*tag) {
            ensure!(n.raw_parent_origin.iter().zip(origin).all(|(g,e)|(g-e).abs()<1e-6),"width_native_placement_does_not_match");
        }
    }
    if let Some(s)=&plan.support {
        let p=after.prototypes.iter().find(|p|p.prototype_id==s.prototype).context("width_support_missing_after_add")?;
        ensure!(p.solids.len()==1&&p.sheet_count==0&&p.fully_loaded&&p.native_units=="mm","width_support_native_scope");
        let b=&p.solids[0];let bb=bounds(&b.exact_box_corners_native)?;let [x,y,z]=s.size_mm;let t=s.wall_mm;let d=s.hole_diameter_mm;
        let expected=[-x/2.,-y/2.,0.,x/2.,y/2.,z];ensure!(bb.iter().zip(expected).all(|(a,b)|(a-b).abs()<1e-5),"width_support_bounds_differs");
        let volume=(2.*x*y*t+2.*t*y*(z-2.*t)-4.*std::f64::consts::PI*(d/2.).powi(2)*2.*t)*1e-9;
        ensure!((b.volume_m3.context("width_support_no_volume")?-volume).abs()<volume*1e-6,"width_support_volume_differs");
        let faces=api.width_face_inventory(u32::try_from(b.body_tag)?)?;w.record("width_support_faces",json!({"prototype":s.prototype,"body_index":0,"faces":faces,"solid":b}))?;
        let mut pairs=vec![];let mut contacts=0;
        for target in &s.placements {
            let n=after.occurrences.iter().find(|n|n.name==target.name&&n.prototype_id==s.prototype).context("width_support_placement_readback_missing")?;
            ensure!(n.raw_parent_origin.iter().zip(target.origin_mm).all(|(a,b)|(a-b).abs()<1e-7),"width_support_origin_differs");
            let mut expected_contacts=BTreeSet::new();
            for id in &target.contacts {
                let source=before.occurrences.iter().find(|o|&o.occurrence_id==id).context("width_support_source_contact_missing")?;
                let matches:Vec<_>=after.occurrences.iter().filter(|o|o.instance_tag==source.instance_tag&&o.prototype_id==source.prototype_id).collect();
                ensure!(matches.len()==1,"width_support_contact_instance_ambiguous");
                expected_contacts.insert(matches[0].occurrence_id.clone());
            }
            let a=world_box(b,&n.local_to_root_m)?;
            w.record("width_support_contact_binding",json!({"name":target.name,"source_contacts":target.contacts,"resolved_contacts":expected_contacts,"bounds_mm":a,"node":n}))?;
            for other in &after.occurrences {if other.occurrence_id==n.occurrence_id{continue;}let op=after.prototypes.iter().find(|p|p.prototype_id==other.prototype_id).unwrap();for (idx,ob)in op.solids.iter().enumerate(){let c=world_box(ob,&other.local_to_root_m)?;if (0..3).all(|k|(a[k]-c[k+3]).max(c[k]-a[k+3])<=0.02){let touch=expected_contacts.contains(&other.occurrence_id)&&idx==0;if touch{contacts+=1;}pairs.push(BodyPair{a_occurrence:n.occurrence_id.clone(),a_body:0,b_occurrence:other.occurrence_id.clone(),b_body:idx,expected_touch:touch});}}}
        }
        w.record("width_support_pair_candidates",json!({"pairs":pairs,"broad_phase":"conservative native box corners transformed to root; AABB nonoverlap excludes pair","required_contacts":contacts}))?;
        ensure!(contacts==s.placements.iter().map(|p|p.contacts.len()).sum::<usize>(),"width_support_contact_not_in_candidate_set");
        let checked=api.width_check_pairs(root,&after,&pairs,&mut |v|w.record("width_support_pair_check",v))?;
        w.record("width_support_clearance",checked.clone())?;
        ensure!(checked["interference_pairs"]==0&&checked["missing_contacts"]==0,"width_support_interference_or_missing_contact");
        let verify=api.full_assembly_info(root,&namespace,budget,"reference_observation")?;crate::assembly::same_geometry(&after,&verify)?;
        for old in &before.occurrences {let new=after.occurrences.iter().find(|n|n.instance_tag==old.instance_tag).context("width_support_original_instance_missing")?;ensure!(old.raw_parent_origin==new.raw_parent_origin&&old.raw_parent_csys==new.raw_parent_csys,"width_support_moved_original_instance");}
    }
    w.record("width_after",serde_json::to_value(&after)?)?;
    if plan.mode=="apply" {
        let saved=api.save_all_owned_assembly(&namespace)?;
        w.record("width_save_returned",saved)?;
    }
    w.verify_motion_source()?;
    if plan.export_step {
        let out=w.output("widened.step")?;
        if let Some(tol)=plan.step_tolerance_mm {w.record("width_step_export_precision",json!({"bspline_tolerance_mm":tol}))?;api.width_export_precise(&path,&out,tol)?;}else{api.export_ap214(&path,&out)?;}
        super::await_step_export(w,"widened.step",Duration::from_secs(1800),Duration::from_secs(1))?;
    }
    Ok(json!({"action":ACTION,"mode":plan.mode,"stretch_count":plan.stretches.len(),"translated_body_count":plan.body_translations.len(),"placed_definition_instances":placements.len(),"prototype_count":after.prototypes.len(),"occurrence_count":after.occurrences.len(),"solid_count":after.prototypes.iter().map(|p|p.solids.len()).sum::<usize>(),"observations":"complete width_before and width_after event records; face inventory records per body","independent_reopen":"required_separate_process","engineering_adoption":"not_assessed"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn face(tag:u32,lo:f64,hi:f64)->Face {Face{tag,kind:22,point:[0.;3],direction:[1.,0.,0.],bounds:[lo,-1.,-1.,hi,1.,1.],radius:0.,normal:1}}
    #[test] fn preserves_center_and_spanning_surfaces() {
        let f=vec![face(1,-30.,-20.),face(2,-50.,50.),face(3,-2.,2.),face(4,20.,30.)];
        assert_eq!(end_faces(&f,0,0.,-1).unwrap(),vec![1]);
        assert_eq!(end_faces(&f,0,0.,1).unwrap(),vec![4]);
    }
    #[test] fn protects_center_mounting_holes_while_extending_plate_ends() {
        let f=vec![face(1,-220.,-219.),face(2,-15.,-9.),face(3,9.,15.),face(4,219.,220.),face(5,-220.,220.)];
        let s=Stretch{prototype:"plate.prt".into(),axis:0,plane:0.,negative_mm:-104.,positive_mm:104.,expected_body_count:1,body_indices:vec![],protected_interval:Some([-30.,30.])};
        assert_eq!(selected_end(&f,&s,-1).unwrap(),vec![1]);
        assert_eq!(selected_end(&f,&s,1).unwrap(),vec![4]);
    }
    #[test] fn rejects_empty_or_invalid_selection() {
        assert!(end_faces(&[face(1,-10.,10.)],0,0.,1).is_err());
        assert!(end_faces(&[face(1,-10.,-1.),face(1,1.,10.)],0,0.,1).is_err());
        assert!(end_faces(&[face(1,2.,1.)],0,0.,1).is_err());
        assert!(end_faces(&[face(1,1.,2.)],3,0.,1).is_err());
    }
}
