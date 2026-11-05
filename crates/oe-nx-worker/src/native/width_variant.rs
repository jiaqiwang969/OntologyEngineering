use super::*;
use crate::geometry::width_variant::Face;

impl Native {
    pub(super) fn width_export_precise_native(&mut self,input:&Path,output:&Path,tolerance_mm:f64)->Result<()> {
        ensure!(tolerance_mm.is_finite()&&(1e-7..=0.01).contains(&tolerance_mm),"width_export_tolerance_invalid");
        let input=path_text(input)?;let output=path_text(output)?;let layers=text("1-256")?;
        let options=StepExport{structure_bytes:std::mem::size_of::<StepExport>()as u32,application_protocol:self.abi.step_ap214,export_from:self.abi.step_existing_part,selection_scope:self.abi.step_entire_part,solids:1,file_save:0,process_hold:1,input:input.as_ptr(),output:output.as_ptr(),layers:layers.as_ptr()};
        let library=self._libraries.last().context("width_export_shim_missing")?;
        type Call=unsafe extern "C" fn(*const StepExport,f64,*mut c_char,u32)->i32;
        let call:Call=unsafe{std::mem::transmute(library.symbol(b"oe_nx_width_export_precise_v1\0")?)};
        let mut error=vec![0;2048];let code=unsafe{call(&options,tolerance_mm,error.as_mut_ptr(),error.len()as u32)};
        self.shim_check(code,&error,"width_export_precise")
    }
    pub(super) fn width_body_bounds_native(&mut self, body:u32)->Result<[f64;6]> {
        let (mut corner,mut axes,mut distances)=([0.;3],[[0.;3];3],[0.;3]);
        self.check(unsafe{(self.uf.bbox_exact)(body,0,corner.as_mut_ptr(),axes.as_mut_ptr(),distances.as_mut_ptr())},"width_exact_body_bounds")?;
        ExactBounds::new(corner,axes,distances)?;
        let mut bb=[f64::INFINITY;6];bb[3..].fill(f64::NEG_INFINITY);
        for mask in 0..8 {let mut p=corner;for a in 0..3 {if mask&(1<<a)!=0 {for i in 0..3 {p[i]+=axes[a][i]*distances[a];}}}for i in 0..3 {bb[i]=bb[i].min(p[i]);bb[i+3]=bb[i+3].max(p[i]);}}
        Ok(bb)
    }
    pub(super) fn width_translate_native(&mut self,part:u32,body:u32,delta:[f64;3])->Result<Value> {
        let library=self._libraries.last().context("width_shim_missing")?;
        type Call=unsafe extern "C" fn(u32,u32,*const f64,*mut c_char,u32)->i32;
        let call:Call=unsafe{std::mem::transmute(library.symbol(b"oe_nx_width_translate_body_v1\0")?)};
        let mut error=vec![0;2048];
        let code=unsafe{call(part,body,delta.as_ptr(),error.as_mut_ptr(),error.len() as u32)};
        self.shim_check(code,&error,"width_translate_body")?;
        Ok(json!({"part_tag":part,"body_tag":body,"delta_mm":delta,"return_code":code,"scale":1}))
    }
    pub(super) fn uf_text_utf8(&self, raw: &[c_char]) -> Result<String> {
        let end=raw.iter().position(|&v|v==0).context("vendor_string_not_terminated")?;
        ensure!(end<32768,"vendor_string_conversion_bound");
        let library=self._libraries.last().context("text_conversion_shim_missing")?;
        type Call=unsafe extern "C" fn(*const c_char,u32,*mut c_char,u32,*mut i32,*mut c_char,u32)->i32;
        let call:Call=unsafe{std::mem::transmute(library.symbol(b"oe_nx_uf_text_utf8_v1\0")?)};
        let mut output=vec![0;(end+1)*4];let mut codeset=-2;let mut error=vec![0;2048];
        let code=unsafe{call(raw.as_ptr(),(end+1) as u32,output.as_mut_ptr(),output.len() as u32,&mut codeset,error.as_mut_ptr(),error.len() as u32)};
        self.shim_check(code,&error,"uf_text_utf8")?;
        ensure!(codeset>=0,"vendor_codeset_uninitialized");
        buffer_text(&output)
    }
    pub(super) fn owned_body_tags_native(&mut self, part: u32) -> Result<Vec<u32>> {
        let library=self._libraries.last().context("body_inventory_shim_missing")?;
        type Call=unsafe extern "C" fn(u32,*mut u32,u32,*mut u32,*mut c_char,u32)->i32;
        let call:Call=unsafe{std::mem::transmute(library.symbol(b"oe_nx_owned_body_inventory_v1\0")?)};
        let mut tags=vec![0;100_000];let mut count=0;let mut error=vec![0;2048];
        let code=unsafe{call(part,tags.as_mut_ptr(),tags.len() as u32,&mut count,error.as_mut_ptr(),error.len() as u32)};
        self.shim_check(code,&error,"owned_body_inventory")?;
        ensure!(count as usize<=tags.len(),"owned_body_inventory_count_invalid");
        tags.truncate(count as usize);
        let mut seen=std::collections::BTreeSet::new();
        ensure!(tags.iter().all(|tag|*tag!=0&&seen.insert(*tag)),"owned_body_inventory_identity_invalid");
        Ok(tags)
    }
    pub(super) fn width_faces_native(&mut self, body: u32) -> Result<Vec<Face>> {
        let mut result=Vec::new();
        for tag in self.list(body,self.uf.body_faces)? {
            ensure!(result.len()<65536,"width_face_budget");
            let mut f=Face{tag,kind:0,point:[0.;3],direction:[0.;3],bounds:[0.;6],radius:0.,normal:0};
            let mut minor=0.;
            self.check(unsafe{(self.uf.face_data)(tag,&mut f.kind,f.point.as_mut_ptr(),f.direction.as_mut_ptr(),f.bounds.as_mut_ptr(),&mut f.radius,&mut minor,&mut f.normal)},"UF_MODL_ask_face_data")?;
            ensure!(f.point.iter().chain(&f.direction).chain(&f.bounds).chain([f.radius].iter()).all(|x|x.is_finite()),"width_face_nonfinite");
            result.push(f);
        }
        Ok(result)
    }
    pub(super) fn width_move_native(&mut self,part:u32,faces:&[u32],delta:[f64;3])->Result<Value> {
        ensure!(part!=0 && !faces.is_empty() && faces.len()<=65536 && delta.iter().all(|x|x.is_finite()),"width_move_input_invalid");
        let library=self._libraries.last().context("width_shim_missing")?;
        type Call=unsafe extern "C" fn(u32,*const u32,u32,*const f64,*mut u32,*mut c_char,u32)->i32;
        let call:Call=unsafe{std::mem::transmute(library.symbol(b"oe_nx_width_move_faces_v1\0")?)};
        let mut feature=0;let mut error=vec![0;2048];
        let code=unsafe{call(part,faces.as_ptr(),faces.len() as u32,delta.as_ptr(),&mut feature,error.as_mut_ptr(),error.len() as u32)};
        self.shim_check(code,&error,"width_move_faces")?;
        // Feature return availability is evidence, not proof of a geometric edit.
        Ok(json!({"part_tag":part,"feature_tag":feature,"feature_returned":feature!=0,"acceptance":"pending native geometry readback","faces":faces,"delta_mm":delta,"return_code":code}))
    }
    pub(super) fn width_place_native(&mut self,instance:u32,origin:[f64;3],csys:&[f64])->Result<Value> {
        ensure!(instance!=0 && csys.len()==9 && origin.iter().chain(csys).all(|x|x.is_finite()),"width_place_input_invalid");
        let library=self._libraries.last().context("width_shim_missing")?;
        type Call=unsafe extern "C" fn(u32,*const f64,*const f64,*mut c_char,u32)->i32;
        let call:Call=unsafe{std::mem::transmute(library.symbol(b"oe_nx_width_place_instance_v1\0")?)};
        let mut error=vec![0;2048];
        let code=unsafe{call(instance,origin.as_ptr(),csys.as_ptr(),error.as_mut_ptr(),error.len() as u32)};
        self.shim_check(code,&error,"width_place_instance")?;
        Ok(json!({"instance_tag":instance,"origin_mm":origin,"csys":csys,"return_code":code}))
    }
}
