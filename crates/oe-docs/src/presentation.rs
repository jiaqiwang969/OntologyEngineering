//! Offline presentation publication. Geometry, source binding and Project ownership
//! remain in their existing crates; receipts do not admit engineering facts.
use anyhow::{Result, ensure};
use oe_contracts::{
    ArtifactReference, PresentationReceipt1, PresentationRequest1, PresentationScene1,
};
use oe_core::{SkillRoot, sha256};
use oe_engineering::presentation::prepare;
use oe_geometry::presentation::{self as geometry, Evaluation, escape};
use oe_kernel::project::Project;
use std::{collections::BTreeMap, path::Path};

const LICENSE: &str = include_str!("../../../docs/third-party/ANATOMY-LICENSE.txt");
pub fn implementation_sha256() -> String {
    sha256(concat!(
        include_str!("presentation.rs"),
        include_str!("../../oe-engineering/src/presentation.rs"),
        include_str!("../../oe-geometry/src/presentation.rs"),
        include_str!("../../../generated/contracts.rs"),
        include_str!("../../../docs/third-party/ANATOMY-LICENSE.txt")
    ))
}
const CSS: &str = r#"
*{box-sizing:border-box}body{margin:0;background:#e8eeef;color:#183447;font:16px/1.6 system-ui,-apple-system,sans-serif}main{max-width:1280px;margin:40px auto;background:#fff;border-radius:22px;overflow:hidden;box-shadow:0 24px 80px #1c354515}header{padding:36px 42px;background:#142f43;color:#fff}.eyebrow{font-size:12px;letter-spacing:.16em;color:#a9cbd9}h1{font-size:32px;font-weight:600;line-height:1.25;margin:12px 0}header p{color:#ccdce4;margin:0}.flow{display:flex;gap:12px;flex-wrap:wrap;padding:20px 42px;background:#f1f5f6;font-size:14px}.flow span{border:1px solid #d4e0e5;border-radius:8px;padding:8px 15px;background:#fff}fieldset{border:0;padding:24px 36px;margin:0;min-width:0}legend{padding:20px 6px 0;font-weight:600}input[type=radio]{accent-color:#1c628a;margin:12px 5px 20px 6px;width:18px;height:18px;vertical-align:middle}label{margin-right:18px;cursor:pointer;font-size:15px}input:focus-visible{outline:3px solid #e69f40;outline-offset:4px}.frame{display:none;grid-template-columns:minmax(0,3fr) minmax(220px,1fr);gap:22px;align-items:start}.frame svg{width:100%;height:auto;display:block}.readout{border:1px solid #dce4e7;border-radius:14px;padding:24px;background:#f8fafb}.readout h2{margin:0 0 16px;font-size:20px}.metric{border-top:1px solid #dce4e7;padding:16px 0}.metric strong{display:block;font:36px/1.3 ui-monospace,monospace;color:#1b628b}.metric small{font-size:13px;color:#657986}.note{font-size:13px;color:#526c7a}a{color:#1b628b}.details{display:grid;grid-template-columns:1fr 1fr;gap:24px;padding:0 42px 30px}.card{border-top:2px solid #d8e2e6;padding-top:16px}.card h2{font-size:17px;margin:0 0 10px}.parts{list-style:none;padding:0;margin:0}.parts li{margin:8px 0}.tag{display:inline-block;margin-left:8px;border:1px solid #d2dfe5;border-radius:6px;padding:1px 7px;font-size:11px;color:#547082}.hash{font:12px/1.7 ui-monospace,monospace;overflow-wrap:anywhere}footer{padding:20px 42px;background:#f3f6f7;font-size:13px;color:#57707d}@media(max-width:780px){main{margin:0;border-radius:0}header,.details,footer{padding:24px}.flow{padding:16px 24px}.frame,.details{grid-template-columns:1fr}fieldset{padding:20px}.readout{padding:18px}h1{font-size:26px}}
"#;
fn html(
    request: &PresentationRequest1,
    scene: &PresentationScene1,
    evaluation: &Evaluation,
) -> String {
    let mut css = CSS.to_owned();
    for i in 0..scene.states.len() {
        css.push_str(&format!(
            "#choice-{i}:checked ~ .frames .frame-{i}{{display:grid}}"
        ));
    }
    let title = escape(&request.title);
    let mut out = format!(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'\"><title>{title}</title><style>{css}</style></head><body><main><header><div class=\"eyebrow\">ENGINEERING ONTOLOGY V2 · PRESENTATION</div><h1>{title}</h1><p>选择一个状态，查看同一组参数驱动的结构与读数。</p></header><div class=\"flow\"><span>01 来源与对象</span><span>02 离散状态</span><span>03 投影与读数</span><span>04 检查与追溯</span></div><fieldset><legend>状态选择</legend>"
    );
    for (i, s) in scene.states.iter().enumerate() {
        out.push_str(&format!("<input type=\"radio\" name=\"state\" id=\"choice-{i}\"{}><label for=\"choice-{i}\">{}</label>",if i==0 {" checked"} else {""},escape(&s.label)));
    }
    out.push_str("<div class=\"frames\">");
    for (i, s) in scene.states.iter().enumerate() {
        out.push_str(&format!("<article class=\"frame frame-{i}\" aria-label=\"{}\"><div>{}</div><aside class=\"readout\"><h2>{}</h2>",escape(&s.label),geometry::render(scene,evaluation,i),escape(&s.label)));
        for p in &scene.parameters {
            let v = s
                .values
                .iter()
                .find(|v| v.parameter_id == p.id)
                .expect("evaluated complete state");
            out.push_str(&format!("<div class=\"metric\">{}<strong>{}</strong><small>{} · 允许范围 {}—{}</small></div>",escape(&p.label),v.value,escape(geometry::unit_label(&p.unit)),p.minimum,p.maximum));
        }
        out.push_str(&format!("<p class=\"note\">这些状态没有时间轴，不表示速度或连续碰撞已验证。</p><a href=\"state-{i:02}.svg\" download>保存当前 SVG</a></aside></article>"));
    }
    out.push_str("</div></fieldset><section class=\"details\"><div class=\"card\"><h2>部件与表达性质</h2><ul class=\"parts\">");
    for p in &scene.parts {
        out.push_str(&format!(
            "<li>{}<span class=\"tag\">{}</span></li>",
            escape(&p.label),
            if p.representation == "illustration" {
                "示意"
            } else {
                "解析近似"
            }
        ));
    }
    out.push_str(&format!("</ul><p class=\"note\">部件列表只解释此图；原生 CAD、BOM 与工程采用仍以各自证据为准。</p></div><div class=\"card\"><h2>检查与来源</h2><p>{} 个状态 · {} 次部件对检查 · {} 处包围盒潜在重叠</p><p class=\"note\">包围盒检查覆盖全部已列状态；面排序采用显示启发式，未证明任意遮挡正确。</p><p>{} · 对象修订 {}</p><div class=\"hash\">来源：{}<br>SHA256：{}</div></div></section><footer>图示方法吸收自 Anatomy，Copyright © 2026 Ryan，<a href=\"ANATOMY-LICENSE.txt\">MIT License</a>。v2 负责来源绑定、Rust 状态计算与交付验证。<br>这是生成时的快照；来源更新后须重新验证和生成。</footer></main></body></html>",scene.states.len(),evaluation.pair_check_count,evaluation.possible_overlaps.len(),escape(&request.object.object_id),escape(&request.object.revision),escape(&request.source.path),escape(&request.source.sha256)));
    out
}
fn build(
    root: &SkillRoot,
    project: &Project,
    request: &PresentationRequest1,
) -> Result<(BTreeMap<String, Vec<u8>>, PresentationReceipt1)> {
    let scene = prepare(root, project, request)?;
    let evaluation = geometry::evaluate(&scene)?;
    let mut files = BTreeMap::new();
    files.insert(
        "index.html".into(),
        html(request, &scene, &evaluation).into_bytes(),
    );
    for i in 0..scene.states.len() {
        files.insert(
            format!("state-{i:02}.svg"),
            geometry::render(&scene, &evaluation, i).into_bytes(),
        );
    }
    files.insert("request.json".into(), serde_json::to_vec_pretty(request)?);
    files.insert("ANATOMY-LICENSE.txt".into(), LICENSE.as_bytes().to_vec());
    let receipt=PresentationReceipt1 {
        schema:"oev2.presentation-receipt/1".into(),request:request.clone(),implementation_sha256:implementation_sha256(),
        files:files.iter().map(|(path,bytes)|ArtifactReference{path:path.clone(),sha256:sha256(bytes)}).collect(),
        state_count:scene.states.len() as u64,pair_check_count:evaluation.pair_check_count,possible_overlaps:evaluation.possible_overlaps,
        coverage:vec!["exact source bytes and current Project/object/conditions".into(),"bounded typed primitives, complete finite states, parameter ranges and unit consistency".into(),"all declared states / all part pairs: strict positive AABB overlap candidates".into(),"deterministic HTML/SVG byte inventory; shared camera and state readouts".into()],
        limitations:vec!["no native CAD, physical, continuous-motion or manufacturing acceptance".into(),"AABB overlap is only a candidate; face-depth ordering is heuristic".into(),"browser interaction and visual acceptance require separate observations".into(),"implementation identity covers these three Rust modules, generated contracts and upstream license; CLI additionally checks its existing compiled-source binding".into(),"engineering adoption not established".into()],
    };
    Ok((files, receipt))
}
pub fn generate(
    root: &SkillRoot,
    project: &Project,
    request: &PresentationRequest1,
    output: &Path,
) -> Result<PresentationReceipt1> {
    let (files, receipt) = build(root, project, request)?;
    let directory = root.new_private_dir(output)?;
    for (name, bytes) in files {
        oe_core::write_new_bytes(&directory.join(name), &bytes)?;
    }
    // Detect source replacement during publication; incomplete directories retain no success receipt.
    prepare(root, project, request)?;
    oe_core::write_new(
        &directory.join("receipt.json"),
        &serde_json::to_value(&receipt)?,
    )?;
    Ok(receipt)
}
pub fn verify(
    root: &SkillRoot,
    project: &Project,
    directory: &Path,
) -> Result<PresentationReceipt1> {
    let directory = root.private_path(directory)?;
    let bytes = oe_core::read_regular_bytes(&directory.join("receipt.json"), 4 * 1024 * 1024)?;
    let recorded: PresentationReceipt1 =
        serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
    recorded
        .validate()
        .map_err(|e| anyhow::anyhow!("presentation_receipt_contract:{e:?}"))?;
    let (files, expected) = build(root, project, &recorded.request)?;
    ensure!(recorded == expected, "presentation_receipt_changed");
    let mut actual = Vec::new();
    for entry in std::fs::read_dir(&directory)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file(),
            "presentation_output_not_regular"
        );
        actual.push(
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("presentation_output_name"))?,
        );
    }
    actual.sort();
    let mut inventory: Vec<_> = files
        .keys()
        .cloned()
        .chain(std::iter::once("receipt.json".into()))
        .collect();
    inventory.sort();
    ensure!(actual == inventory, "presentation_output_inventory_changed");
    for (name, expected) in files {
        let bytes = oe_core::read_regular_bytes(&directory.join(name), 16 * 1024 * 1024)?;
        ensure!(bytes == expected, "presentation_output_bytes_changed");
    }
    prepare(root, project, &recorded.request)?;
    Ok(recorded)
}

/// Check an intent before saving its source-bound request.
pub fn validate_scene(scene: &PresentationScene1) -> Result<()> {
    geometry::evaluate(scene)?;
    Ok(())
}
