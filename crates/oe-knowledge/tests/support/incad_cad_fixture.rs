#![allow(dead_code)]
#[path = "incad_fixture.rs"]
mod source;
use serde_json::{Value, json};
pub use source::{Fixture, entry, save};
use std::{collections::BTreeMap, io::Write, path::Path};

pub struct CadFixture {
    pub source: Fixture,
    pub files: BTreeMap<String, Vec<u8>>,
}
impl CadFixture {
    pub fn new(root: &Path) -> Self {
        let source = Fixture::new(root);
        // Deliberately synthetic supplier reference bytes, never native models.
        let files = BTreeMap::from([
            (
                "model/assembly.step".into(),
                b"ISO-10303-21; SYNTHETIC ASSEMBLY SOURCE; END-ISO-10303-21;".to_vec(),
            ),
            (
                "model/part.step".into(),
                b"ISO-10303-21; SYNTHETIC PART SOURCE; END-ISO-10303-21;".to_vec(),
            ),
        ]);
        let mut result = Self { source, files };
        result.publish();
        result
    }
    pub fn folder(&self) -> std::path::PathBuf {
        self.source.root.join("cases/cn/000019/attempts/fixed")
    }
    pub fn zip_bytes(&self) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, raw) in &self.files {
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(raw).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    pub fn publish(&mut self) {
        let folder = self.folder();
        let original = "cad/ap203/original.zip";
        std::fs::create_dir_all(folder.join("cad/ap203/files/model")).unwrap();
        let zip = self.zip_bytes();
        std::fs::write(folder.join(original), &zip).unwrap();
        let mut members = Vec::new();
        for (name, raw) in &self.files {
            let path = format!("cad/ap203/files/{name}");
            let p = folder.join(&path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, raw).unwrap();
            members.push(json!({"archive_name":name,"path":path,"bytes":raw.len(),"sha256":oe_core::sha256(raw),
                "inspection":{"native_geometry_validation":"not_run","fixture":"synthetic source bytes"}}));
        }
        let mut case: Value =
            serde_json::from_slice(&std::fs::read(folder.join("case.json")).unwrap()).unwrap();
        case["cad_downloads"] = json!([{"code":"ap203","label":"STEP(AP214)","status":"downloaded",
            "original":original,"original_bytes":zip.len(),"original_sha256":oe_core::sha256(&zip),"members":members,
            "native_validation":"historical_not_run","zip_inventory_and_crc_checked":true}]);
        case["official_bom"] = json!([{"item":"synthetic reference only","quantity":1}]);
        case["visible_bom"] = case["official_bom"].clone();
        case["bom_product_urls"] = json!([]);
        save(&folder.join("case.json"), &case);
        self.rebind_sources();
    }
    pub fn rebind_sources(&mut self) {
        let folder = self.folder();
        let mp = folder.join("manifest.json");
        let mut manifest: Value = serde_json::from_slice(&std::fs::read(&mp).unwrap()).unwrap();
        let mut files = vec![
            entry(&folder, "source.html"),
            entry(&folder, "case.json"),
            entry(&folder, "cad/ap203/original.zip"),
        ];
        for name in self.files.keys() {
            files.push(entry(&folder, &format!("cad/ap203/files/{name}")));
        }
        manifest["files"] = json!(files);
        let manifest_sha = save(&mp, &manifest);
        let mut source = self.source.task["specifications"]["source"].clone();
        source["manifest_sha256"] = json!(manifest_sha);
        source["case_record_sha256"] = entry(&folder, "case.json")["sha256"].clone();
        for name in [
            "input.json",
            "reference.json",
            "study.json",
            "original.json",
        ] {
            let p = self.source.projection.join(name);
            let mut v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
            v["source"] = source.clone();
            save(&p, &v);
            if name == "input.json" {
                self.source.task["specifications"] = v;
            }
        }
        let p = self.source.projection.join("manifest.json");
        let mut m: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        m["source"] = source;
        m["files"] = json!(
            [
                "input.json",
                "reference.json",
                "study.json",
                "original.json",
                "input.html",
                "reference.html",
                "study.html",
                "original.html",
                "original-page.html"
            ]
            .iter()
            .map(|n| entry(&self.source.projection, n))
            .collect::<Vec<_>>()
        );
        self.source.request.projection_manifest.sha256 = save(&p, &m);
        self.source.write_task();
    }
}
