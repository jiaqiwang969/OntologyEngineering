//! Fixed UF acquisition; no native tags or files are accepted from outside the
//! newly bound session and explicit task namespace.
use super::*;
use crate::assembly::{self, Rigid};
use crate::geometry::NativeApi;
use oe_contracts::{
    NxAssemblyBudget, NxAssemblyObservation, NxAssemblyOccurrenceObservation,
    NxAssemblyPrototypeObservation, NxAssemblySolidObservation,
};
use std::collections::{BTreeMap, BTreeSet};

impl Native {
    pub(super) fn observe_assembly_reference_sets(
        &mut self,
        observed: &NxAssemblyObservation,
    ) -> Result<oe_contracts::NxAssemblyReferenceSets2> {
        let mut members = Vec::new();
        for occurrence in &observed.occurrences {
            let part_occurrence = u32::try_from(occurrence.part_occurrence_tag)?;
            let instance = u32::try_from(occurrence.instance_tag)?;
            let prototype = observed
                .prototypes
                .iter()
                .find(|p| p.prototype_id == occurrence.prototype_id)
                .context("native_reference_set_prototype_missing")?;
            ensure!(
                part_occurrence != 0
                    && instance != 0
                    && unsafe { (self.uf.is_occurrence)(part_occurrence) } != 0
                    && unsafe { (self.uf.instance)(part_occurrence) } == instance
                    && unsafe { (self.uf.prototype)(part_occurrence) } as u64
                        == prototype.native_part_tag,
                "native_reference_set_live_inventory_differs"
            );
            let mut names = Vec::new();
            for tag in [instance, part_occurrence] {
                let mut path = vec![0; self.abi.max_path_bytes as usize];
                let mut reference = vec![0; self.abi.max_refset_bytes as usize];
                let mut name = vec![0; self.abi.max_instance_bytes as usize];
                let mut origin = [0.; 3];
                let mut csys = [0.; 9];
                let mut transform = [0.; 16];
                self.check(
                    unsafe {
                        (self.uf.component_data)(
                            tag,
                            path.as_mut_ptr(),
                            reference.as_mut_ptr(),
                            name.as_mut_ptr(),
                            origin.as_mut_ptr(),
                            csys.as_mut_ptr(),
                            transform.as_mut_ptr(),
                        )
                    },
                    "native_reference_set_component_data",
                )?;
                names.push(buffer_text(&reference)?);
            }
            members.push(oe_contracts::NxAssemblyReferenceSetMember2 {
                occurrence_id: occurrence.occurrence_id.clone(),
                part_occurrence_tag: occurrence.part_occurrence_tag,
                instance_tag: occurrence.instance_tag,
                instance_reference_set: names[0].clone(),
                part_occurrence_reference_set: names[1].clone(),
            });
        }
        let result = oe_contracts::NxAssemblyReferenceSets2 {
            schema: crate::assembly_reference_sets::SCHEMA.into(),
            api: "UF_ASSEM_ask_component_data".into(),
            root_prototype_id: observed.root_prototype_id.clone(),
            complete: true,
            members,
        };
        crate::assembly_reference_sets::validate(&result, observed)?;
        Ok(result)
    }
}

#[repr(C)]
struct UfEntireAdd {
    version: u32,
    structure_bytes: u32,
    parent: Tag,
    prototype_bytes: u32,
    name_bytes: u32,
    layer: i32,
    prototype: *const c_char,
    name: *const c_char,
    origin: [f64; 3],
    csys: [f64; 9],
}
#[repr(C)]
#[derive(Default)]
struct UfEntireAddCall {
    version: u32,
    structure_bytes: u32,
    status_bytes: u32,
    tag_bytes: u32,
    logical_bytes: u32,
    returned: u32,
    native_code: i32,
    instance: Tag,
    before: [u8; 24],
    after: [u8; 24],
}
#[repr(C)]
struct NxEntireLoadedAdd {
    version: u32,
    structure_bytes: u32,
    parent: Tag,
    child: Tag,
    prototype_bytes: u32,
    name_bytes: u32,
    layer: i32,
    reserved: u32,
    prototype: *const c_char,
    name: *const c_char,
    origin: [f64; 3],
    column_rotation: [f64; 9],
}
#[repr(C)]
struct NxEntireLoadedObservation {
    load: NxOpenAddObservation,
    vendor_default_refset: [c_char; 64],
    entire_refset: [c_char; 64],
    direct_owner_matches: u32,
    entire_set_returned: u32,
    builder_destroyed: u32,
    reserved: u32,
}

impl Native {
    pub fn add_nxopen_entire_loaded(
        &mut self,
        parent: Tag,
        child: Tag,
        path: &Path,
        name: &str,
        origin: [f64; 3],
        rotation: &[f64],
    ) -> Result<()> {
        let actual = std::fs::canonicalize(self.part_path(child)?)?;
        ensure!(
            actual == std::fs::canonicalize(path)?,
            "nxopen_prototype_path_differs"
        );
        ensure!(
            origin.iter().all(|v| v.is_finite()),
            "nxopen_origin_nonfinite"
        );
        let columns = assembly::nxopen_column_rotation(rotation)?;
        let loaded = self.assembly_load_constant()?;
        let (mut parent_load, mut child_load, mut parent_units, mut child_units) = (-1, -1, -1, -1);
        let codes = [
            unsafe { (self.uf.load_state)(parent, &mut parent_load) },
            unsafe { (self.uf.load_state)(child, &mut child_load) },
            unsafe { (self.uf.units)(parent, &mut parent_units) },
            unsafe { (self.uf.units)(child, &mut child_units) },
        ];
        let display = unsafe { (self.uf.display_part)() };
        let work = unsafe { (self.uf.work_part)() };
        let pre = json!({"parent":parent,"child":child,"prototype_path":actual.to_string_lossy(),"codes":codes,
            "parent_load":parent_load,"child_load":child_load,"parent_units":parent_units,"child_units":child_units,
            "display":display,"work":work,"origin_parent_native":origin,"source_rotation_row_major":rotation,
            "sdk_rotation_column_order":columns});
        ensure!(
            queue_load_observation(
                &mut self.load_observations,
                &mut self.load_observation_bytes,
                json!({"operation":"nxopen_entire_loaded_preconditions","observation":pre})
            )?,
            "nxopen_precondition_diagnostic_budget"
        );
        for code in codes {
            self.check(code, "nxopen_loaded_part_precondition")?;
        }
        ensure!(
            parent_load == loaded && child_load == loaded && display == parent && work == parent,
            "nxopen_owned_loaded_context_differs"
        );
        ensure!(
            [self.abi.metric, self.abi.english].contains(&parent_units)
                && [self.abi.metric, self.abi.english].contains(&child_units),
            "nxopen_native_units_unresolved"
        );
        let path = path_text(path)?;
        let name = text(name)?;
        let request = NxEntireLoadedAdd {
            version: 1,
            structure_bytes: std::mem::size_of::<NxEntireLoadedAdd>() as u32,
            parent,
            child,
            prototype_bytes: u32::try_from(path.as_bytes_with_nul().len())?,
            name_bytes: u32::try_from(name.as_bytes_with_nul().len())?,
            layer: -1,
            reserved: 0,
            prototype: path.as_ptr(),
            name: name.as_ptr(),
            origin,
            column_rotation: columns,
        };
        type Add = unsafe extern "C" fn(
            *const NxEntireLoadedAdd,
            u32,
            *mut NxEntireLoadedObservation,
            u32,
            *mut c_char,
            u32,
        ) -> i32;
        let shim = self._libraries.last().context("assembly_shim_missing")?;
        let api = unsafe {
            std::mem::transmute::<*mut c_void, Add>(
                shim.symbol(b"oe_nxopen_entire_loaded_add_v1\0")?,
            )
        };
        let mut out: Box<NxEntireLoadedObservation> = Box::new(unsafe { std::mem::zeroed() });
        let mut error = vec![0; 1024];
        let code = unsafe {
            api(
                &request,
                std::mem::size_of::<NxEntireLoadedAdd>() as u32,
                &mut *out,
                std::mem::size_of::<NxEntireLoadedObservation>() as u32,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        let raw = &out.load;
        let instance = if raw.call_returned == 1 && raw.component != 0 {
            unsafe { (self.uf.instance)(raw.component) }
        } else {
            0
        };
        let actual_child = if instance != 0 {
            unsafe { (self.uf.child_of_instance)(instance) }
        } else {
            0
        };
        let actual_parent = if instance != 0 {
            unsafe { (self.uf.parent_of_instance)(instance) }
        } else {
            0
        };
        let outcome = oe_contracts::NxAssemblyAddOutcome {
            normal_return: raw.call_returned == 1,
            status_present: raw.status_present == 1,
            status_copied: raw.status_copied == 1,
            status_released: raw.status_released == 1,
            unloaded_count: i64::from(raw.unloaded_count),
            component_tag: u64::from(raw.component),
            definition_instance_tag: u64::from(instance),
            prototype_tag: u64::from(actual_child),
            definition_parent_tag: u64::from(actual_parent),
            reference_set: buffer_text(&raw.reference_set).unwrap_or_default(),
            entire_reference_set: buffer_text(&out.entire_refset).unwrap_or_default(),
            complete: raw.complete == 1,
        };
        let mut diagnostic = raw.observation(code, &error);
        diagnostic["schema"] = json!("cad-agent.nxopen-entire-loaded-add/v1");
        diagnostic["api"] = json!("ComponentAssembly::AddComponent(BasePart*)");
        diagnostic["preconditions"] = pre;
        diagnostic["outcome"] = serde_json::to_value(&outcome)?;
        diagnostic["vendor_default_refset"] = json!(buffer_text(&out.vendor_default_refset).ok());
        diagnostic["direct_owner_matches"] = json!(out.direct_owner_matches);
        diagnostic["entire_set_returned"] = json!(out.entire_set_returned);
        diagnostic["builder_destroyed"] = json!(out.builder_destroyed);
        ensure!(
            queue_load_observation(
                &mut self.load_observations,
                &mut self.load_observation_bytes,
                diagnostic
            )?,
            "nxopen_add_diagnostic_budget"
        );
        self.shim_check(code, &error, "nxopen_entire_loaded_add")?;
        ensure!(
            raw.version == 1
                && raw.structure_bytes == std::mem::size_of::<NxOpenAddObservation>() as u32
                && raw.call_entered == 1
                && raw.phase == 3
                && raw.exception_code == 0
                && out.direct_owner_matches == 1
                && out.entire_set_returned == 1
                && out.builder_destroyed == 1
                && out.reserved == 0,
            "nxopen_entire_vendor_observation_incomplete"
        );
        assembly::check_nxopen_add(&outcome, parent, child)
    }
    fn assembly_load_diagnostic(&self) -> Result<Value> {
        type Read = unsafe extern "C" fn(*mut i32, u32, *mut c_char, u32) -> i32;
        let shim = self._libraries.last().context("assembly_shim_missing")?;
        let api = unsafe {
            std::mem::transmute::<*mut c_void, Read>(
                shim.symbol(b"oe_nx_assembly_load_diagnostic_v1\0")?,
            )
        };
        let mut values = [-1; 6];
        let mut error = vec![0; 1024];
        let code = unsafe {
            api(
                values.as_mut_ptr(),
                6,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        Ok(
            json!({"return_code":code,"message":buffer_text(&error).ok(),"raw_values":values.to_vec(),
            "complete":code==0 && values[0]==1,"scope":"this owned session: getters only; no load option or license setter"}),
        )
    }
    pub fn owned_assembly_context(&mut self, part: Tag, namespace: &Path) -> Result<()> {
        let (path, _) = self.namespace_part(part, namespace)?;
        let before = unsafe { (self.uf.display_part)() };
        // Installed2412 UF documentation: set_work_part changes only a
        // descendant of the displayed assembly. Independent owned prototype
        // definitions require set_display_part before work-part selection.
        let display_code = unsafe { (self.uf.set_display_part)(part) };
        let after = unsafe { (self.uf.display_part)() };
        ensure!(
            queue_load_observation(
                &mut self.load_observations,
                &mut self.load_observation_bytes,
                json!({"operation":"owned_assembly_definition_context","part_tag":part,"part_path":plain_path(&path)?,
                "display_part_before":before,"display_part_after":after,"set_display_return_code":display_code,
                "namespace_verified":true,"context_scope":"this owned native worker process only; no OS focus activation"})
            )?,
            "native_assembly_context_diagnostic_budget"
        );
        self.check(display_code, "UF_PART_set_display_part_owned_assembly")?;
        ensure!(after == part, "native_owned_display_part_differs");
        self.set_work_part(part)
    }
    fn assembly_load_constant(&self) -> Result<i32> {
        type Abi = unsafe extern "C" fn(*mut i32, u32) -> i32;
        let shim = self._libraries.last().context("assembly_shim_missing")?;
        let api = unsafe {
            std::mem::transmute::<*mut c_void, Abi>(shim.symbol(b"oe_nx_assembly_abi_v1\0")?)
        };
        let mut values = [0; 3];
        self.check(
            unsafe { api(values.as_mut_ptr(), 3) },
            "assembly_compiled_abi",
        )?;
        ensure!(
            values[0] == 1 && values[1] == 4 && values[2] >= 0,
            "assembly_compiled_abi_differs"
        );
        Ok(values[2])
    }
    pub fn import_assembly(&mut self, input: &Path, output: &Path) -> Result<()> {
        type Import = unsafe extern "C" fn(*const StepImport, *mut u32, *mut c_char, u32) -> i32;
        let shim = self._libraries.last().context("assembly_shim_missing")?;
        let api = unsafe {
            std::mem::transmute::<*mut c_void, Import>(
                shim.symbol(b"oe_nx_step_assembly_import_v1\0")?,
            )
        };
        let (input, output) = (path_text(input)?, path_text(output)?);
        let options = StepImport {
            structure_bytes: std::mem::size_of::<StepImport>() as u32,
            import_to: self.abi.step_new_part,
            solids: 1,
            file_open: 0,
            process_hold: 1,
            input: input.as_ptr(),
            output: output.as_ptr(),
        };
        let mut flattened = u32::MAX;
        let mut error = vec![0; 1024];
        let code = unsafe {
            api(
                &options,
                &mut flattened,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        self.shim_check(code, &error, "native_preserved_assembly_import")?;
        ensure!(
            flattened == 0,
            "native_assembly_import_flatten_flag_differs"
        );
        Ok(())
    }
    pub fn create_units_part(&mut self, path: &Path, unit: &str) -> Result<Tag> {
        let raw = match unit {
            "mm" => self.abi.metric,
            "inch" => self.abi.english,
            _ => anyhow::bail!("fixture_unit_unsupported"),
        };
        let path = path_text(path)?;
        let mut part = 0;
        self.check(
            unsafe { (self.uf.new_part)(path.as_ptr(), raw, &mut part) },
            "UF_PART_new_explicit_fixture_units",
        )?;
        ensure!(part != 0, "fixture_new_part_null");
        Ok(part)
    }
    pub fn add_entire_instance(
        &mut self,
        parent: Tag,
        child: Tag,
        path: &Path,
        name: &str,
        origin_parent_native: [f64; 3],
        rotation: &[f64],
    ) -> Result<()> {
        let verified = std::fs::canonicalize(self.part_path(child)?)?;
        ensure!(
            verified == std::fs::canonicalize(path)?,
            "fixture_prototype_path_differs"
        );
        Rigid::from_rotation_origin(rotation, &[0., 0., 0.])?;
        let mut csys = [0.; 9];
        for r in 0..3 {
            for c in 0..3 {
                csys[r * 3 + c] = rotation[c * 3 + r];
            }
        }
        let origin = origin_parent_native;
        ensure!(
            origin.iter().all(|v| v.is_finite()),
            "fixture_origin_nonfinite"
        );
        // These are independent native reads, retained even when the following
        // single add operation fails. No file/load-option setter is used here.
        let (mut parent_units, mut child_units, mut parent_load, mut child_load) = (-1, -1, -1, -1);
        let parent_units_code = unsafe { (self.uf.units)(parent, &mut parent_units) };
        let child_units_code = unsafe { (self.uf.units)(child, &mut child_units) };
        let parent_load_code = unsafe { (self.uf.load_state)(parent, &mut parent_load) };
        let child_load_code = unsafe { (self.uf.load_state)(child, &mut child_load) };
        let display_before = unsafe { (self.uf.display_part)() };
        let work_before = unsafe { (self.uf.work_part)() };
        let load_options = self.assembly_load_diagnostic()?;
        let native_reads = json!({"parent_path":self.part_path(parent)?,
            "child_path":verified.to_string_lossy(),"parent_units_code":parent_units_code,"parent_units":parent_units,
            "child_units_code":child_units_code,"child_units":child_units,"parent_load_code":parent_load_code,
            "parent_load_state":parent_load,"child_load_code":child_load_code,"child_load_state":child_load,
            "display_part_before":display_before,"work_part_before":work_before,"load_options":load_options});
        ensure!(
            queue_load_observation(
                &mut self.load_observations,
                &mut self.load_observation_bytes,
                json!({"operation":"owned_entire_add_native_preconditions","native_reads":native_reads,
                "origin_parent_native":origin,"csys_native":csys,"reference_set_request":"entire_part_null","layer":-1})
            )?,
            "native_assembly_precondition_diagnostic_budget"
        );
        self.check(parent_units_code, "assembly_parent_units_before_add")?;
        self.check(child_units_code, "assembly_child_units_before_add")?;
        self.check(parent_load_code, "assembly_parent_load_before_add")?;
        self.check(child_load_code, "assembly_child_load_before_add")?;
        let loaded = self.assembly_load_constant()?;
        ensure!(
            parent_load == loaded && child_load == loaded,
            "assembly_add_prototypes_not_fully_loaded"
        );
        ensure!(
            display_before == parent,
            "assembly_add_display_parent_differs"
        );
        ensure!(work_before == parent, "assembly_add_work_parent_differs");
        let path = path_text(path)?;
        let name = text(name)?;
        let request = UfEntireAdd {
            version: 1,
            structure_bytes: std::mem::size_of::<UfEntireAdd>() as u32,
            parent,
            prototype_bytes: u32::try_from(path.as_bytes_with_nul().len())?,
            name_bytes: u32::try_from(name.as_bytes_with_nul().len())?,
            layer: -1,
            prototype: path.as_ptr(),
            name: name.as_ptr(),
            origin,
            csys,
        };
        type Add = unsafe extern "C" fn(
            *const UfEntireAdd,
            *mut c_void,
            u32,
            *mut UfEntireAddCall,
            u32,
            *mut c_char,
            u32,
        ) -> i32;
        let shim = self._libraries.last().context("assembly_shim_missing")?;
        let api = unsafe {
            std::mem::transmute::<*mut c_void, Add>(shim.symbol(b"oe_nx_uf_entire_add_v1\0")?)
        };
        let mut status = LoadStatus::default();
        let mut call = UfEntireAddCall::default();
        let mut error = vec![0; 1024];
        let wrapper_code = unsafe {
            api(
                &request,
                (&mut status as *mut LoadStatus).cast(),
                std::mem::size_of::<LoadStatus>() as u32,
                &mut call,
                std::mem::size_of::<UfEntireAddCall>() as u32,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        let child_after = if call.returned == 1 && call.instance != 0 {
            Some(unsafe { (self.uf.child_of_instance)(call.instance) })
        } else {
            None
        };
        let parent_after = if call.returned == 1 && call.instance != 0 {
            Some(unsafe { (self.uf.parent_of_instance)(call.instance) })
        } else {
            None
        };
        let arguments = json!({"parent":parent,"child":child,"name":name.to_str().ok(),"instance":call.instance,
            "reference_set_request":"entire_part_null","layer":-1,"origin_parent_native":origin,"csys_native":csys,
            "native_preconditions":native_reads,"vendor_call":{"wrapper_code":wrapper_code,
                "wrapper_message":buffer_text(&error).ok(),"version":call.version,"structure_bytes":call.structure_bytes,
                "status_bytes":call.status_bytes,"tag_bytes":call.tag_bytes,"logical_bytes":call.logical_bytes,
                "returned":call.returned,"native_code":call.native_code,"status_before_raw":call.before.to_vec(),
                "status_after_raw":call.after.to_vec(),"child_of_instance_after":child_after,
                "parent_of_instance_after":parent_after,"display_part_after":unsafe{(self.uf.display_part)()},
                "work_part_after":unsafe{(self.uf.work_part)()},
                "raw_storage_scope":"private diagnostic includes transient pointer bytes; no persistent identity"}});
        // Always copy/free vendor status once before deciding the operation.
        let status_result = self.load_status(
            call.native_code,
            &mut status,
            "UF_ASSEM_add_part_to_assembly2_entire",
            arguments,
        );
        self.shim_check(wrapper_code, &error, "entire_add_vendor_boundary")?;
        ensure!(
            call.version == 1
                && call.structure_bytes == std::mem::size_of::<UfEntireAddCall>() as u32
                && call.status_bytes == std::mem::size_of::<LoadStatus>() as u32
                && call.tag_bytes == 4
                && call.logical_bytes == 1
                && call.returned == 1
                && call.before.iter().all(|v| *v == 0),
            "entire_add_vendor_observation_invalid"
        );
        status_result?;
        ensure!(
            call.instance != 0 && child_after == Some(child) && parent_after == Some(parent),
            "fixture_created_instance_prototype_differs"
        );
        Ok(())
    }
    pub(super) fn namespace_part(&self, part: Tag, namespace: &Path) -> Result<(PathBuf, String)> {
        let path = std::fs::canonicalize(self.part_path(part)?)?;
        let root = std::fs::canonicalize(namespace)?;
        let relative = path
            .strip_prefix(&root)
            .context("native_part_outside_task_namespace")?;
        let name = relative
            .to_str()
            .context("native_part_path_encoding")?
            .replace('\\', "/");
        let rel = oe_nx_protocol::RelativePath::new(name.clone())?;
        ensure!(
            name.to_ascii_lowercase().ends_with(".prt"),
            "native_part_not_prt"
        );
        // Retained normal-file + ancestor verification, not a text prefix alone.
        let owner = StableRoot::open(
            plain_path(&root)?
                .parent()
                .context("namespace_parent_missing")?,
            None,
        )?;
        let dir = owner.directory(root.file_name().context("namespace_name_missing")?)?;
        let pin = adapter::pin_relative_file(&dir, &rel, oe_nx_protocol::MAX_PAYLOAD_BYTES)?;
        // Namespace membership uses the verified handle's size, not a content
        // digest. Source/copy/result binding still hashes bytes at its owners.
        // Rehashing every loaded PRT here made repeated inventories quadratic
        // in bytes read, without comparing or retaining any of those digests.
        ensure!(pin.size() > 0, "native_part_file_empty");
        pin.verify()?;
        dir.verify()?;
        owner.verify()?;
        Ok((path, name))
    }
    fn assembly_prototype(
        &mut self,
        part: Tag,
        namespace: &Path,
        fully_loaded: i32,
    ) -> Result<NxAssemblyPrototypeObservation> {
        let mut load = -1;
        self.check(
            unsafe { (self.uf.load_state)(part, &mut load) },
            "UF_ask_load_state_of_part",
        )?;
        ensure!(
            load == fully_loaded,
            "native_assembly_prototype_not_fully_loaded"
        );
        let (path, id) = self.namespace_part(part, namespace)?;
        self.set_work_part(part)?;
        let mut units = -1;
        self.check(
            unsafe { (self.uf.units)(part, &mut units) },
            "native_prototype_units",
        )?;
        let unit = if units == self.abi.metric {
            "mm"
        } else if units == self.abi.english {
            "inch"
        } else {
            anyhow::bail!("native_prototype_unit_unknown")
        };
        let scale = assembly::unit_to_m(unit)?;
        let mut solids = Vec::new();
        let mut sheets = 0;
        let mut body_tags = Vec::new();
        let mut body = 0;
        let mut seen = BTreeSet::new();
        loop {
            self.check(
                unsafe { (self.uf.cycle)(part, self.abi.solid_type, &mut body) },
                "UF_OBJ_cycle_objs_in_part_assembly",
            )?;
            if body == 0 {
                break;
            }
            ensure!(
                seen.insert(body) && body_tags.len() < 100_000,
                "native_body_cycle_or_budget"
            );
            body_tags.push(body);
        }
        for body in body_tags {
            let (mut object_type, mut subtype, mut owner) = (-1, -1, 0);
            let type_code = unsafe { (self.uf.object_type)(body, &mut object_type, &mut subtype) };
            let owner_code = unsafe { (self.uf.owning_part)(body, &mut owner) };
            let occurrence = unsafe { (self.uf.is_occurrence)(body) };
            let candidate = assembly::prototype_body_candidate(
                type_code,
                object_type,
                subtype,
                self.abi.solid_type,
                owner_code,
                owner,
                part,
                occurrence,
            );
            let sample = self.assembly_cycle_observation_count < 32;
            self.assembly_cycle_observation_count += 1;
            let mut diagnostic = json!({"operation":"assembly_prototype_cycle_object","part_tag":part,"object_tag":body,
                "type_query_code":type_code,"object_type":object_type,"subtype":subtype,"expected_solid_type":self.abi.solid_type,
                "installed_body_subtype":0,"owner_query_code":owner_code,"owner":owner,"occurrence_raw":occurrence,
                "candidate":candidate.as_ref().ok(),"classification_error":candidate.as_ref().err().map(|e|e.to_string()),
                "diagnostic_scope":"first32 cycle objects across task plus failures; all cycle objects still classified"});
            if sample || candidate.is_err() {
                ensure!(
                    queue_load_observation(
                        &mut self.load_observations,
                        &mut self.load_observation_bytes,
                        diagnostic.clone()
                    )?,
                    "assembly_cycle_diagnostic_budget"
                );
            }
            if !candidate? {
                continue;
            }
            let mut kind = -1;
            let body_code = unsafe { (self.uf.body_type)(body, &mut kind) };
            diagnostic["body_query_code"] = json!(body_code);
            diagnostic["body_kind"] = json!(kind);
            diagnostic["operation"] = json!("assembly_prototype_body_type_returned");
            if sample
                || body_code != 0
                || (kind != self.abi.sheet_body && kind != self.abi.solid_body)
            {
                ensure!(
                    queue_load_observation(
                        &mut self.load_observations,
                        &mut self.load_observation_bytes,
                        diagnostic
                    )?,
                    "assembly_body_type_diagnostic_budget"
                );
            }
            self.check(body_code, "native_assembly_body_type")?;
            if kind == self.abi.sheet_body {
                sheets += 1;
                continue;
            }
            ensure!(
                kind == self.abi.solid_body,
                "native_assembly_body_kind_unknown"
            );
            let (mut corner, mut axes, mut distances) = ([0.; 3], [[0.; 3]; 3], [0.; 3]);
            self.check(
                unsafe {
                    (self.uf.bbox_exact)(
                        body,
                        0,
                        corner.as_mut_ptr(),
                        axes.as_mut_ptr(),
                        distances.as_mut_ptr(),
                    )
                },
                "native_prototype_exact_bbox",
            )?;
            ExactBounds::new(corner, axes, distances)?;
            let mut native = Vec::new();
            for mask in 0..8 {
                let mut p = corner;
                for a in 0..3 {
                    if mask & (1 << a) != 0 {
                        for i in 0..3 {
                            p[i] += axes[a][i] * distances[a];
                        }
                    }
                }
                native.extend(p);
            }
            let mut object = body;
            let mut accuracy = [0.; 11];
            accuracy[0] = 0.999999;
            let mut props = [0.; 47];
            let mut statistics = [0.; 13];
            // SDK units4 = kg/metres; only geometrical volume is adopted here.
            // The SDK ignores density for solids; no material/mass assumption.
            self.check(
                unsafe {
                    (self.uf.mass_props)(
                        &mut object,
                        1,
                        1,
                        4,
                        0.,
                        1,
                        accuracy.as_mut_ptr(),
                        props.as_mut_ptr(),
                        statistics.as_mut_ptr(),
                    )
                },
                "native_prototype_geometrical_volume_m3",
            )?;
            ensure!(
                props[1].is_finite() && props[1] > 0.,
                "native_prototype_volume_invalid"
            );
            solids.push(NxAssemblySolidObservation {
                body_tag: body as u64,
                owning_part_tag: owner as u64,
                is_occurrence: false,
                exact_box_corners_m: native.iter().map(|x| x * scale).collect(),
                exact_box_corners_native: native,
                volume_m3: Some(props[1]),
            });
        }
        Ok(NxAssemblyPrototypeObservation {
            prototype_id: id,
            part_path: plain_path(&path)?.to_string_lossy().into_owned(),
            native_part_tag: part as u64,
            native_units: unit.into(),
            unit_to_m: scale,
            load_state_raw: load as i64,
            fully_loaded: true,
            solids,
            sheet_count: sheets,
        })
    }
    pub fn observe_assembly(
        &mut self,
        part: Tag,
        namespace: &Path,
        budget: &NxAssemblyBudget,
        role: &str,
    ) -> Result<NxAssemblyObservation> {
        let value = self.read_assembly_inventory(part, namespace, budget, role)?;
        assembly::validate_observation(&value, budget)?;
        Ok(value)
    }
    // Private raw acquisition only. Each public version applies its own checker.
    fn read_assembly_inventory(
        &mut self,
        part: Tag,
        namespace: &Path,
        budget: &NxAssemblyBudget,
        role: &str,
    ) -> Result<NxAssemblyObservation> {
        assembly::budget_valid(budget)?;
        self.owned_assembly_context(part, namespace)?;
        let loaded = self.assembly_load_constant()?;
        let root_proto = self.assembly_prototype(part, namespace, loaded)?;
        let root_id = root_proto.prototype_id.clone();
        let root_unit = root_proto.native_units.clone();
        let mut prototypes = BTreeMap::from([(part, root_proto)]);
        let mut occurrences = Vec::new();
        let mut visited = BTreeSet::new();
        let root = unsafe { (self.uf.root_occurrence)(part) };
        let mut pending = if root != 0 {
            vec![(root, None::<String>, Rigid::identity(), 1u64, vec![part])]
        } else {
            vec![]
        };
        while let Some((parent_occ, parent_id, parent_global, depth, ancestor_parts)) =
            pending.pop()
        {
            let mut tags = null_mut();
            let count = unsafe { (self.uf.all_children)(parent_occ, &mut tags) };
            let _guard = Allocation {
                p: tags.cast(),
                free: self.uf.free,
            };
            ensure!(
                count >= 0
                    && count as u64 <= budget.max_occurrences
                    && (count == 0 || !tags.is_null()),
                "native_all_children_count_invalid"
            );
            ensure!(
                count == 0 || depth <= budget.max_depth,
                "native_assembly_depth_budget"
            );
            for index in 0..count as usize {
                let occ = unsafe { *tags.add(index) };
                ensure!(
                    occ != 0 && visited.insert(occ),
                    "native_part_occurrence_cycle_or_duplicate"
                );
                ensure!(
                    (occurrences.len() as u64) < budget.max_occurrences,
                    "native_assembly_occurrence_budget"
                );
                let instance = unsafe { (self.uf.instance)(occ) };
                let proto = unsafe { (self.uf.prototype)(occ) };
                ensure!(
                    instance != 0
                        && proto != 0
                        && unsafe { (self.uf.child_of_instance)(instance) } == proto,
                    "native_instance_or_prototype_unresolved"
                );
                ensure!(
                    !ancestor_parts.contains(&proto),
                    "native_recursive_prototype_cycle"
                );
                let parent_part = unsafe { (self.uf.parent_of_instance)(instance) };
                ensure!(
                    parent_part == *ancestor_parts.last().unwrap(),
                    "native_definition_parent_differs"
                );
                if !prototypes.contains_key(&proto) {
                    ensure!(
                        (prototypes.len() as u64) < budget.max_prototypes,
                        "native_assembly_prototype_budget"
                    );
                    prototypes.insert(proto, self.assembly_prototype(proto, namespace, loaded)?);
                }
                let mut path = vec![0; self.abi.max_path_bytes as usize];
                let mut refset = vec![0; self.abi.max_refset_bytes as usize];
                let mut name = vec![0; self.abi.max_instance_bytes as usize];
                let mut origin = [0.; 3];
                let mut csys = [0.; 9];
                let mut transform = [0.; 16];
                self.check(
                    unsafe {
                        (self.uf.component_data)(
                            instance,
                            path.as_mut_ptr(),
                            refset.as_mut_ptr(),
                            name.as_mut_ptr(),
                            origin.as_mut_ptr(),
                            csys.as_mut_ptr(),
                            transform.as_mut_ptr(),
                        )
                    },
                    "native_immediate_parent_component_data",
                )?;
                let mut global_raw = [[0.; 4]; 4];
                self.check(
                    unsafe { (self.uf.root_transform)(0, occ, global_raw.as_mut_ptr()) },
                    "native_total_root_transform",
                )?;
                let raw = global_raw.into_iter().flatten().collect::<Vec<_>>();
                let global = Rigid::from_root_native(&raw, &root_unit)?;
                let local = parent_global.inverse()?.compose(global)?;
                let id = format!("{}/{index:06}", parent_id.as_deref().unwrap_or("root"));
                occurrences.push(NxAssemblyOccurrenceObservation {
                    occurrence_id: id.clone(),
                    parent_occurrence_id: parent_id.clone(),
                    prototype_id: prototypes[&proto].prototype_id.clone(),
                    part_occurrence_tag: occ as u64,
                    instance_tag: instance as u64,
                    name: buffer_text(&name)?,
                    reference_set: buffer_text(&refset)?,
                    raw_parent_origin: origin.to_vec(),
                    raw_parent_csys: csys.to_vec(),
                    raw_parent_transform: transform.to_vec(),
                    raw_root_transform: raw,
                    local_to_parent_m: local.0.to_vec(),
                    local_to_root_m: global.0.to_vec(),
                });
                let mut ancestors = ancestor_parts.clone();
                ancestors.push(proto);
                pending.push((occ, Some(id), global, depth + 1, ancestors));
            }
        }
        self.set_work_part(part)?;
        let value = NxAssemblyObservation {
            schema: assembly::OBSERVATION_SCHEMA.into(),
            root_prototype_id: root_id,
            prototypes: prototypes.into_values().collect(),
            occurrences,
            complete: true,
            source_role: role.into(),
            engineering_adoption: "not_assessed".into(),
        };
        Ok(value)
    }
    pub fn save_owned_assembly(&mut self, namespace: &Path) -> Result<Value> {
        let count = unsafe { (self.uf.num_parts)() };
        ensure!(
            (1..=4096).contains(&count),
            "native_loaded_part_count_invalid"
        );
        let mut paths = Vec::new();
        for i in 0..count {
            let part = unsafe { (self.uf.nth_part)(i) };
            ensure!(part != 0, "native_loaded_part_missing");
            let (path, _) = self.namespace_part(part, namespace)?;
            paths.push(plain_path(&path)?);
        }
        let mut errors = 0;
        let mut parts = null_mut();
        let mut codes = null_mut();
        let code = unsafe { (self.uf.save_all)(&mut errors, &mut parts, &mut codes) };
        let _parts = Allocation {
            p: parts.cast(),
            free: self.uf.free,
        };
        let _codes = Allocation {
            p: codes.cast(),
            free: self.uf.free,
        };
        ensure!(
            (0..=4096).contains(&errors) && (errors == 0 || (!parts.is_null() && !codes.is_null())),
            "native_save_error_inventory_invalid"
        );
        let error_records = (0..errors as usize)
            .map(|i| json!({"part_tag":unsafe{*parts.add(i)},"return_code":unsafe{*codes.add(i)}}))
            .collect::<Vec<_>>();
        let record = json!({"api":"UF_PART_save_all","return_code":code,"file_error_count":errors,"file_errors":error_records,"all_loaded_owned_part_paths":paths});
        self.check(code, "native_assembly_save_all")?;
        ensure!(errors == 0, "native_assembly_save_file_errors");
        Ok(record)
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn entire_add_copied_boundary_matches_reviewed_x64_layout() {
        assert_eq!(std::mem::size_of::<UfEntireAdd>(), 136);
        assert_eq!(std::mem::offset_of!(UfEntireAdd, prototype), 24);
        assert_eq!(std::mem::offset_of!(UfEntireAdd, origin), 40);
        assert_eq!(std::mem::offset_of!(UfEntireAdd, csys), 64);
        assert_eq!(std::mem::size_of::<UfEntireAddCall>(), 80);
        assert_eq!(std::mem::offset_of!(UfEntireAddCall, before), 32);
        assert_eq!(std::mem::offset_of!(UfEntireAddCall, after), 56);
        assert_eq!(std::mem::size_of::<NxEntireLoadedAdd>(), 144);
        assert_eq!(std::mem::offset_of!(NxEntireLoadedAdd, origin), 48);
        assert_eq!(std::mem::offset_of!(NxEntireLoadedAdd, column_rotation), 72);
        assert_eq!(std::mem::size_of::<NxEntireLoadedObservation>(), 65920);
        assert_eq!(
            std::mem::offset_of!(NxEntireLoadedObservation, vendor_default_refset),
            65776
        );
    }
}

impl Native {
    pub fn observe_assembly_positions(
        &mut self,
        part: Tag,
        namespace: &Path,
        budget: &NxAssemblyBudget,
        role: &str,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        let inventory = self.read_assembly_inventory(part, namespace, budget, role)?;
        let library = self._libraries.last().context("native_shim_missing")?;
        let query: unsafe extern "C" fn(Tag, *mut i32, *mut Tag, *mut c_char, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_position_override_v1\0")?) };
        let mut position_overrides = Vec::with_capacity(inventory.occurrences.len());
        for o in &inventory.occurrences {
            let tag = Tag::try_from(o.part_occurrence_tag)?;
            let mut raw_type = -1;
            let mut parent = 0;
            let mut error = vec![0; 1024];
            let code = unsafe {
                query(
                    tag,
                    &mut raw_type,
                    &mut parent,
                    error.as_mut_ptr(),
                    error.len() as u32,
                )
            };
            self.shim_check(code, &error, "native_position_override_query")?;
            position_overrides.push(oe_contracts::NxAssemblyPositionOverride {
                occurrence_id: o.occurrence_id.clone(),
                part_occurrence_tag: o.part_occurrence_tag,
                raw_type: raw_type as i64,
                parent_occurrence_tag: parent as u64,
            });
        }
        let value = oe_contracts::NxAssemblyPositionObservation {
            schema: crate::assembly_position::SCHEMA.into(),
            root_prototype_id: inventory.root_prototype_id,
            root_occurrence_tag: unsafe { (self.uf.root_occurrence)(part) } as u64,
            prototypes: inventory.prototypes,
            occurrences: inventory.occurrences,
            position_overrides,
            complete: inventory.complete,
            source_role: inventory.source_role,
            engineering_adoption: inventory.engineering_adoption,
        };
        crate::assembly_position::validate_position_observation(&value, budget)?;
        Ok(value)
    }
}

impl Native {
    /// Read-only whole-session inventory, including WAVE/non-occurrence parts.
    pub fn loaded_owned_assembly_files(&self, namespace: &Path) -> Result<Vec<String>> {
        let count = unsafe { (self.uf.num_parts)() };
        ensure!(
            (1..=4096).contains(&count),
            "native_loaded_part_count_invalid"
        );
        let mut tags = BTreeSet::new();
        let mut files = BTreeSet::new();
        for i in 0..count {
            let part = unsafe { (self.uf.nth_part)(i) };
            ensure!(
                part != 0 && tags.insert(part),
                "native_loaded_part_missing_or_duplicate"
            );
            let (_, name) = self.namespace_part(part, namespace)?;
            ensure!(files.insert(name), "native_loaded_part_file_duplicate");
        }
        ensure!(
            unsafe { (self.uf.num_parts)() } == count,
            "native_loaded_part_inventory_changed"
        );
        for i in 0..count {
            ensure!(
                tags.remove(&unsafe { (self.uf.nth_part)(i) }),
                "native_loaded_part_inventory_changed"
            );
        }
        ensure!(tags.is_empty(), "native_loaded_part_inventory_incomplete");
        Ok(files.into_iter().collect())
    }
}
