//! Native-only adapter for the separate /2 raw save path. Legacy caller stays intact.
use super::*;
use crate::geometry::motion_task::save_capture2::{self, RawSave2, SaveApi2, SaveCaptureSink2};

struct CapturedSave<'a> {
    native: &'a mut Native,
    namespace: &'a Path,
}
impl SaveApi2 for CapturedSave<'_> {
    fn loaded_part_count(&mut self) -> i32 {
        unsafe { (self.native.uf.num_parts)() }
    }
    fn owned_part_path(&mut self, index: i32) -> Result<String> {
        let part = unsafe { (self.native.uf.nth_part)(index) };
        ensure!(part != 0, "native_loaded_part_missing");
        let (path, _) = self.native.namespace_part(part, self.namespace)?;
        Ok(plain_path(&path)?
            .to_str()
            .context("native_path_not_unicode")?
            .to_owned())
    }
    fn call_save(&mut self) -> Result<RawSave2> {
        let (mut count, mut parts, mut codes) = (0, null_mut(), null_mut());
        let code = unsafe { (self.native.uf.save_all)(&mut count, &mut parts, &mut codes) };
        let _parts = Allocation {
            p: parts.cast(),
            free: self.native.uf.free,
        };
        let _codes = Allocation {
            p: codes.cast(),
            free: self.native.uf.free,
        };
        // Range/NULL checks live in the shared, mockable copying boundary. They
        // run before this closure. Ownership and ABI qualification remain native.
        Ok(save_capture2::copy_file_errors2(
            code,
            count,
            !parts.is_null(),
            !codes.is_null(),
            |index| Ok((unsafe { *parts.add(index) }, unsafe { *codes.add(index) })),
        ))
    }
}
impl Native {
    pub fn save_owned_assembly_capture2(
        &mut self,
        namespace: &Path,
        sink: &mut dyn SaveCaptureSink2,
    ) -> Result<Value> {
        let value = save_capture2::execute(
            &mut CapturedSave {
                native: self,
                namespace,
            },
            sink,
        )?;
        Ok(serde_json::to_value(value)?)
    }
}
