//! Read selected installed type libraries without registering or activating CAD.
use anyhow::{Context, Result, ensure};
use oe_contracts::{SolidWorksNativeFilePin, SolidWorksNativeInstallation};
use oe_windows_controller::windows::{PinnedFile, StableRoot};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use windows::{
    Win32::System::{Com::*, Ole::*},
    core::{BSTR, GUID, PCWSTR},
};

pub struct Member {
    pub key: &'static str,
    pub iid: GUID,
    pub name: &'static str,
    pub dispid: i32,
    pub kind: u16,
    pub arguments: &'static [&'static str],
    pub flags: &'static [u16],
    pub returns: &'static str,
}
include!("api_signature_table.rs");
pub struct Api {
    members: BTreeMap<&'static str, Member>,
    pub observation: Value,
}
impl Api {
    pub fn member(&self, key: &str) -> Result<&Member> {
        self.members
            .get(key)
            .with_context(|| format!("solidworks_unbound_api_member:{key}"))
    }
    #[cfg(test)]
    pub(crate) fn test(member: Member) -> Self {
        Self {
            members: std::iter::once((member.key, member)).collect(),
            observation: json!({"scope":"synthetic_COM_ABI_only_not_CAD"}),
        }
    }
}
pub fn pinned_file(pin: &SolidWorksNativeFilePin) -> Result<PinnedFile> {
    let path = Path::new(&pin.path);
    ensure!(
        path.is_absolute(),
        "solidworks_native_absolute_path_required"
    );
    let root = StableRoot::open(path.parent().context("solidworks_file_parent")?, None)?;
    let dir = root.as_directory()?;
    let mut file = dir
        .pin_file(
            path.file_name().context("solidworks_file_name")?,
            crate::MAX_MODEL_BYTES,
        )?
        .context("solidworks_source_file_missing")?;
    ensure!(file.size() == pin.bytes, "solidworks_source_size_changed");
    let raw = file.read_all(crate::MAX_MODEL_BYTES)?;
    ensure!(
        crate::sha256(&raw) == pin.sha256,
        "solidworks_source_sha_changed"
    );
    Ok(file)
}
pub fn observe_file(path: &Path, limit: u64) -> Result<SolidWorksNativeFilePin> {
    let root = StableRoot::open(path.parent().context("solidworks_file_parent")?, None)?;
    let dir = root.as_directory()?;
    let mut file = dir
        .pin_file(path.file_name().context("solidworks_file_name")?, limit)?
        .context("solidworks_file_missing")?;
    let raw = file.read_all(limit)?;
    file.verify()?;
    Ok(SolidWorksNativeFilePin {
        path: path.to_string_lossy().into_owned(),
        bytes: raw.len() as u64,
        sha256: crate::sha256(&raw),
    })
}
pub struct Installation {
    pub api: Api,
    pins: Vec<(SolidWorksNativeFilePin, PinnedFile)>,
}
impl Installation {
    pub fn load(spec: &SolidWorksNativeInstallation) -> Result<Self> {
        let mut pins = Vec::new();
        for pin in [
            &spec.executable,
            &spec.type_library,
            &spec.constants_library,
        ]
        .into_iter()
        .chain(spec.template.iter())
        {
            pins.push((pin.clone(), pinned_file(pin)?));
        }
        let mut wide: Vec<u16> = spec.type_library.path.encode_utf16().collect();
        wide.push(0);
        let library = unsafe { LoadTypeLibEx(PCWSTR(wide.as_ptr()), REGKIND_NONE)? };
        let lib = library_identity(&library)?;
        ensure!(
            lib["guid"] == "83a33d31-27c5-11ce-bfd4-00400513bb57"
                && lib["major"] == 33
                && lib["system_kind"] == 3,
            "solidworks_type_library_identity"
        );
        let mut rows = Vec::new();
        let mut members = BTreeMap::new();
        for spec in member_specs() {
            let info = unsafe { library.GetTypeInfoOfGuid(&spec.iid)? };
            let attr = unsafe { info.GetTypeAttr()? };
            let count = unsafe { (*attr).cFuncs };
            unsafe {
                info.ReleaseTypeAttr(attr);
            }
            ensure!(count <= 4096, "solidworks_type_member_budget");
            let mut found = None;
            for i in 0..u32::from(count) {
                let desc = unsafe { info.GetFuncDesc(i)? };
                let result = (|| -> Result<Option<Value>> {
                    let d = unsafe { &*desc };
                    if d.memid != spec.dispid || d.invkind.0 != i32::from(spec.kind) {
                        return Ok(None);
                    }
                    ensure!((0..=64).contains(&d.cParams), "solidworks_parameter_budget");
                    let mut name = BSTR::new();
                    unsafe {
                        info.GetDocumentation(
                            d.memid,
                            Some(&mut name),
                            None,
                            std::ptr::null_mut(),
                            None,
                        )?;
                    }
                    ensure!(
                        name.to_string() == spec.name,
                        "solidworks_member_name_changed"
                    );
                    let params = if d.cParams == 0 {
                        &[][..]
                    } else {
                        ensure!(
                            !d.lprgelemdescParam.is_null(),
                            "solidworks_parameter_description_null"
                        );
                        unsafe {
                            std::slice::from_raw_parts(d.lprgelemdescParam, d.cParams as usize)
                        }
                    };
                    let args = params
                        .iter()
                        .map(|p| type_name(&info, &p.tdesc, 0))
                        .collect::<Result<Vec<_>>>()?;
                    let flags: Vec<_> = params
                        .iter()
                        .map(|p| unsafe { p.Anonymous.paramdesc.wParamFlags.0 })
                        .collect();
                    let returns = type_name(&info, &d.elemdescFunc.tdesc, 0)?;
                    ensure!(
                        args == spec.arguments && flags == spec.flags && returns == spec.returns,
                        "solidworks_exact_signature_changed:{}",
                        spec.key
                    );
                    Ok(Some(
                        json!({"member":spec.key,"iid":format!("{:?}",spec.iid),"dispid":d.memid,"invoke_kind":d.invkind.0,"arguments":args,"parameter_flags":flags,"returns":returns,"calling_convention":d.callconv.0}),
                    ))
                })();
                unsafe {
                    info.ReleaseFuncDesc(desc);
                }
                if let Some(value) = result? {
                    ensure!(found.is_none(), "solidworks_signature_ambiguous");
                    found = Some(value);
                }
            }
            rows.push(found.with_context(|| format!("solidworks_member_missing:{}", spec.key))?);
            ensure!(
                members.insert(spec.key, spec).is_none(),
                "solidworks_duplicate_api_key"
            );
        }
        let mut wide: Vec<u16> = spec.constants_library.path.encode_utf16().collect();
        wide.push(0);
        let constants = unsafe { LoadTypeLibEx(PCWSTR(wide.as_ptr()), REGKIND_NONE)? };
        let constants_id = library_identity(&constants)?;
        ensure!(
            constants_id["guid"] == "4687f359-55d0-4cd3-b6cf-2eb42c11f989"
                && constants_id["major"] == 33
                && constants_id["system_kind"] == 3,
            "solidworks_constants_library_identity"
        );
        let expected: BTreeMap<&str, i32> = [
            ("swDocPART", 1),
            ("swOpenDocOptions_Silent", 1),
            ("swOpenDocOptions_ReadOnly", 2),
            ("swSaveAsOptions_Silent", 1),
            ("swSaveAsCurrentVersion", 0),
            ("swSolidBody", 0),
            ("swEndCondBlind", 0),
            ("swStartSketchPlane", 0),
        ]
        .into_iter()
        .collect();
        let mut found = BTreeMap::new();
        let count = unsafe { constants.GetTypeInfoCount() };
        ensure!(count <= 8192, "solidworks_constant_type_budget");
        for index in 0..count {
            let info = unsafe { constants.GetTypeInfo(index)? };
            let attr = unsafe { info.GetTypeAttr()? };
            let (kind, vars) = unsafe { ((*attr).typekind, (*attr).cVars) };
            unsafe {
                info.ReleaseTypeAttr(attr);
            }
            if kind != TKIND_ENUM {
                continue;
            }
            ensure!(vars <= 8192, "solidworks_constant_member_budget");
            for index in 0..u32::from(vars) {
                let desc = unsafe { info.GetVarDesc(index)? };
                let result = (|| -> Result<()> {
                    let desc = unsafe { &*desc };
                    let mut name = BSTR::new();
                    unsafe {
                        info.GetDocumentation(
                            desc.memid,
                            Some(&mut name),
                            None,
                            std::ptr::null_mut(),
                            None,
                        )?;
                    }
                    if let Some(wanted) = expected.get(name.to_string().as_str()) {
                        ensure!(desc.varkind == VAR_CONST, "solidworks_expected_constant");
                        let value = unsafe { desc.Anonymous.lpvarValue.as_ref() }
                            .context("solidworks_constant_null")?;
                        let actual = crate::com::integer(value)?;
                        ensure!(actual == *wanted, "solidworks_constant_value_changed");
                        ensure!(
                            found.insert(name.to_string(), actual).is_none(),
                            "solidworks_constant_ambiguous"
                        );
                    }
                    Ok(())
                })();
                unsafe {
                    info.ReleaseVarDesc(desc);
                }
                result?;
            }
        }
        ensure!(
            found.len() == expected.len(),
            "solidworks_required_constant_missing"
        );
        let api = Api {
            members,
            observation: json!({"status":"exact_selected_signatures_checked","type_library":lib,"constants_library":constants_id,"members":rows,"constants":found,"registration":"none","cad_activation":"not_run"}),
        };
        let mut this = Self { api, pins };
        this.verify()?;
        Ok(this)
    }
    pub fn verify(&mut self) -> Result<()> {
        for (wanted, file) in &mut self.pins {
            file.verify()?;
            ensure!(
                crate::sha256(&file.read_all(crate::MAX_MODEL_BYTES)?) == wanted.sha256,
                "solidworks_installation_drift"
            );
        }
        Ok(())
    }
}
fn library_identity(library: &ITypeLib) -> Result<Value> {
    let ptr = unsafe { library.GetLibAttr()? };
    let a = unsafe { &*ptr };
    let out = json!({"guid":format!("{:?}",a.guid).to_ascii_lowercase(),"major":a.wMajorVerNum,"minor":a.wMinorVerNum,"system_kind":a.syskind.0,"flags":a.wLibFlags});
    unsafe {
        library.ReleaseTLibAttr(ptr);
    }
    Ok(out)
}
fn type_name(info: &ITypeInfo, d: &TYPEDESC, depth: usize) -> Result<String> {
    ensure!(depth <= 8, "solidworks_type_description_depth");
    Ok(match d.vt.0 {
        3 => "i4".into(),
        5 => "r8".into(),
        8 => "bstr".into(),
        9 => "dispatch".into(),
        11 => "bool".into(),
        12 => "variant".into(),
        24 => "void".into(),
        26 => {
            let child =
                unsafe { d.Anonymous.lptdesc.as_ref() }.context("solidworks_type_pointer_null")?;
            format!("ptr:{}", type_name(info, child, depth + 1)?)
        }
        29 => {
            let referenced = unsafe { info.GetRefTypeInfo(d.Anonymous.hreftype)? };
            let mut name = BSTR::new();
            unsafe {
                referenced.GetDocumentation(
                    -1,
                    Some(&mut name),
                    None,
                    std::ptr::null_mut(),
                    None,
                )?;
            }
            name.to_string()
        }
        other => anyhow::bail!("solidworks_signature_type_not_supported:{other}"),
    })
}
