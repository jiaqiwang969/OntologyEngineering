//! Current-session CopyComponents POD boundary. No old CAD scripts or fallback.
use super::*;
use oe_contracts::{NxAssemblyOccurrenceObservation, NxComponentCopyOutcome1};

#[repr(C)]
struct Request {
    version: u32,
    structure_bytes: u32,
    parent: Tag,
    source_component: Tag,
    name_bytes: u32,
    reserved: u32,
    name: *const c_char,
}
#[repr(C)]
struct Observation {
    version: u32,
    structure_bytes: u32,
    call_entered: u32,
    call_returned: u32,
    source_component: Tag,
    source_instance: Tag,
    source_parent: Tag,
    source_prototype: Tag,
    returned_count: u64,
    captured_count: u32,
    returned_components: [Tag; 64],
    new_instance: Tag,
    new_parent: Tag,
    new_prototype: Tag,
    rename_attempted: u32,
    rename_code: i32,
    complete: u32,
    exception_code: i32,
}
const _: () =
    assert!(std::mem::size_of::<Request>() == 32 && std::mem::offset_of!(Request, name) == 24);
const _: () = assert!(
    std::mem::size_of::<Observation>() == 328
        && std::mem::offset_of!(Observation, returned_count) == 32
        && std::mem::offset_of!(Observation, returned_components) == 44
);

impl Native {
    pub(super) fn component_copy(
        &mut self,
        parent: Tag,
        source: &NxAssemblyOccurrenceObservation,
        name: &str,
    ) -> Result<NxComponentCopyOutcome1> {
        let component = Tag::try_from(source.part_occurrence_tag)?;
        let instance = Tag::try_from(source.instance_tag)?;
        ensure!(
            parent != 0
                && component != 0
                && instance != 0
                && unsafe { (self.uf.is_occurrence)(component) } != 0
                && unsafe { (self.uf.instance)(component) } == instance
                && unsafe { (self.uf.parent_of_instance)(instance) } == parent
                && unsafe { (self.uf.work_part)() } == parent
                && unsafe { (self.uf.display_part)() } == parent,
            "component_copy_current_owned_context_differs"
        );
        ensure!(
            !name.is_empty()
                && name.len() <= 64
                && name.bytes().all(|x| x.is_ascii_alphanumeric() || x == b'_'),
            "component_copy_name_invalid"
        );
        // The maximum raw POD/error observation is <16KiB. Reserve its complete
        // diagnostic budget before a native side effect, not after a partial copy.
        ensure!(
            self.load_observation_bytes <= 512 * 1024 - 16 * 1024,
            "component_copy_diagnostic_capacity_before_call"
        );
        let name = text(name)?;
        let request = Request {
            version: 1,
            structure_bytes: std::mem::size_of::<Request>() as u32,
            parent,
            source_component: component,
            name_bytes: u32::try_from(name.as_bytes_with_nul().len())?,
            reserved: 0,
            name: name.as_ptr(),
        };
        type Copy = unsafe extern "C" fn(
            *const Request,
            u32,
            *mut Observation,
            u32,
            *mut c_char,
            u32,
        ) -> i32;
        let shim = self
            ._libraries
            .last()
            .context("component_copy_shim_missing")?;
        let function = unsafe {
            std::mem::transmute::<*mut c_void, Copy>(shim.symbol(b"oe_nx_component_copy_v1\0")?)
        };
        let mut observation: Box<Observation> = Box::new(unsafe { std::mem::zeroed() });
        let mut error = vec![0; 1024];
        let code = unsafe {
            function(
                &request,
                std::mem::size_of::<Request>() as u32,
                &mut *observation,
                std::mem::size_of::<Observation>() as u32,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        let o = &*observation;
        let record = json!({"operation":"NXOpen_ComponentAssembly_CopyComponents_single_v1","wrapper_code":code,
            "version":o.version,"structure_bytes":o.structure_bytes,"call_entered":o.call_entered,"call_returned":o.call_returned,
            "source_component":o.source_component,"source_instance":o.source_instance,"source_parent":o.source_parent,
            "source_prototype":o.source_prototype,"returned_count":o.returned_count,"captured_count":o.captured_count,
            "returned_component_storage":o.returned_components.to_vec(),"new_instance":o.new_instance,"new_parent":o.new_parent,
            "new_prototype":o.new_prototype,"rename_attempted":o.rename_attempted,"rename_code":o.rename_code,
            "complete":o.complete,"exception_code":o.exception_code,"error_utf8":buffer_text(&error).ok(),
            "error_storage_bytes":error.iter().map(|x|*x as u8).collect::<Vec<_>>()});
        ensure!(
            serde_json::to_vec(&record)?.len() <= 16 * 1024,
            "component_copy_raw_diagnostic_bound"
        );
        ensure!(
            queue_load_observation(
                &mut self.load_observations,
                &mut self.load_observation_bytes,
                record
            )?,
            "component_copy_raw_diagnostic_overflow"
        );
        ensure!(
            o.version == 1
                && o.structure_bytes == std::mem::size_of::<Observation>() as u32
                && o.captured_count <= 64
                && [
                    o.call_entered,
                    o.call_returned,
                    o.rename_attempted,
                    o.complete
                ]
                .iter()
                .all(|x| *x <= 1),
            "component_copy_observation_abi_invalid"
        );
        Ok(NxComponentCopyOutcome1 {
            schema: "oev2.nx-component-copy-outcome/1".into(),
            call_entered: o.call_entered == 1,
            call_returned: o.call_returned == 1,
            wrapper_code: code as i64,
            exception_code: o.exception_code as i64,
            source_component_tag: o.source_component as u64,
            source_instance_tag: o.source_instance as u64,
            source_parent_tag: o.source_parent as u64,
            source_prototype_tag: o.source_prototype as u64,
            returned_count: o.returned_count,
            returned_component_tags: o.returned_components[..o.captured_count as usize]
                .iter()
                .map(|x| *x as u64)
                .collect(),
            new_instance_tag: o.new_instance as u64,
            new_parent_tag: o.new_parent as u64,
            new_prototype_tag: o.new_prototype as u64,
            rename_attempted: o.rename_attempted == 1,
            rename_code: o.rename_code as i64,
            complete: o.complete == 1,
        })
    }
}
