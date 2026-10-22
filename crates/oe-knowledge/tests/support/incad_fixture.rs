use oe_contracts::{ArtifactReference, IncadIntakeRequest};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub fn save(path: &Path, value: &Value) -> String {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let raw = serde_json::to_vec_pretty(value).unwrap();
    fs::write(path, &raw).unwrap();
    oe_core::sha256(&raw)
}
pub fn entry(root: &Path, name: &str) -> Value {
    let raw = fs::read(root.join(name)).unwrap();
    json!({"path":name,"bytes":raw.len(),"sha256":oe_core::sha256(raw)})
}
pub struct Fixture {
    pub request: IncadIntakeRequest,
    pub root: PathBuf,
    pub projection: PathBuf,
    pub task: Value,
}
impl Fixture {
    pub fn new(root: &Path) -> Self {
        fs::create_dir_all(root).unwrap();
        let canonical = root.canonicalize().unwrap();
        let root = canonical.as_path();
        let folder = root.join("cases/cn/000019/attempts/fixed");
        let projection = root.join("design-inputs/cn/000019/fixed");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(&projection).unwrap();
        let html = b"<html><body>SPEC: force 10 N\nREFERENCE: answer-secret</body></html>";
        fs::write(folder.join("source.html"), html).unwrap();
        save(
            &folder.join("case.json"),
            &json!({"number":"000019","site":"cn","source_sha256":oe_core::sha256(html)}),
        );
        let source_url = "https://www.misumi.com.cn/asia/incadlibrary/detail/000019.html";
        let case_manifest = json!({"schema":"incad.full-case-manifest/1","number":"000019","site":"cn","source_url":source_url,"files":[entry(&folder,"source.html"),entry(&folder,"case.json")],"errors":[{"reason":"image_not_archived"}],"offered_formats":2,"downloaded_formats":1,"native_cad_validation":"not_run","complete_for_requested_scope":false});
        let manifest_sha = save(&folder.join("manifest.json"), &case_manifest);
        let source = json!({"id":"supplier-html","url":source_url,"path":"cases/cn/000019/attempts/fixed/source.html","sha256":oe_core::sha256(html),"bytes":html.len(),"manifest_path":"cases/cn/000019/attempts/fixed/manifest.json","manifest_sha256":manifest_sha,"case_record_sha256":entry(&folder,"case.json")["sha256"],"projection_version":"fixed","offline_page":"cases/cn/000019/attempts/fixed/source-offline.html"});
        let input = json!({"schema":"incad.design-input/1","number":"000019","title":"fixture","purpose":"test","source":source,"scope":"Complete supplied specifications","sections":[{"id":"section-0","heading":"load","interpretation":"not_reviewed","locator":{"selector":"h4.right_ttl","ordinal":0},"kind":"precision_load","text":"force 10 N\n","text_sha256":oe_core::sha256(b"force 10 N\n"),"html":"<p>force 10 N</p>","source_ref":"supplier-html","status":"supplier_declared"}],"missing_sections":["environment"],"source_claim_status":"supplier_declared","unit_interpretation":"not_reviewed","cross_section_consistency":"not_reviewed","project_binding":"not_bound","engineering_adoption":"not_run"});
        save(&projection.join("input.json"), &input);
        for (name, schema) in [
            ("reference.json", "incad.design-reference/1"),
            ("study.json", "incad.design-study/1"),
            ("original.json", "incad.complete-original/1"),
        ] {
            save(
                &projection.join(name),
                &json!({"schema":schema,"number":"000019","source":source,"answer":"answer-secret","primary":{"verbatim_dom_text":"source full\n","text_sha256":oe_core::sha256(b"source full\n")},"whole_page":{"verbatim_dom_text":"full whole\n","text_sha256":oe_core::sha256(b"full whole\n")}}),
            );
        }
        for n in [
            "input.html",
            "reference.html",
            "study.html",
            "original.html",
            "original-page.html",
        ] {
            fs::write(projection.join(n), b"original fixture HTML\n").unwrap();
        }
        let names = [
            "input.json",
            "reference.json",
            "study.json",
            "original.json",
            "input.html",
            "reference.html",
            "study.html",
            "original.html",
            "original-page.html",
        ];
        let manifest = json!({"schema":"incad.design-projection-manifest/1","number":"000019","source":source,"files":names.iter().map(|n|entry(&projection,n)).collect::<Vec<_>>()});
        let sha = save(&projection.join("manifest.json"), &manifest);
        let task = json!({"schema":"incad.zero-design-task/1","case_number":"000019","title":"fixture","specifications":input,"current_request":"Keep original 10 N; investigate 20 N separately.\n","preparation_only":true,"project_binding":"not_bound","conditions_confirmation":"not_run","baseline_exposure":"development_replay","design_execution":"not_run","engineering_adoption":"not_run","next_step":"Current Project"});
        let task_sha = save(&root.join("task.json"), &task);
        let request = IncadIntakeRequest {
            schema: "oev2.incad-intake/1".into(),
            archive_root: root.display().to_string(),
            case_number: "000019".into(),
            projection_manifest: ArtifactReference {
                path: "design-inputs/cn/000019/fixed/manifest.json".into(),
                sha256: sha,
            },
            zero_design_task: ArtifactReference {
                path: root.join("task.json").display().to_string(),
                sha256: task_sha,
            },
        };
        Self {
            request,
            root: root.into(),
            projection,
            task,
        }
    }
    pub fn write_task(&mut self) {
        self.request.zero_design_task.sha256 = save(&self.root.join("task.json"), &self.task);
    }
}
