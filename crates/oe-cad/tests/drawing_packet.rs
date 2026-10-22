use oe_cad::drawing::{audit_packet, member};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn packet() -> (tempfile::TempDir, Value, Value, Vec<Vec<String>>) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("var/maintenance/rust-tests");
    fs::create_dir_all(&base).unwrap();
    let t = tempfile::tempdir_in(base).unwrap();
    fs::create_dir_all(t.path().join("验证记录")).unwrap();
    fs::create_dir_all(t.path().join("pdf")).unwrap();
    let header = [
        "序号",
        "机型",
        "零件",
        "图纸页",
        "总装引用",
        "图纸PDF",
        "审核状态",
        "重点核对原因",
        "刷新前对照PDF",
    ];
    let mut rows = vec![header.into_iter().map(str::to_owned).collect()];
    let specs = [
        ("A", "SHT1", "drawing_content_present", false, true),
        ("A", "SHT2", "blank_template", false, true),
        ("B", "SHT1", "drawing_content_present", true, false),
    ];
    let mut checks = Vec::new();
    let mut apdfs = Vec::new();
    let mut bpdfs = Vec::new();
    for (i, (part, sheet, status, layout, assembly)) in specs.into_iter().enumerate() {
        let name = format!("pdf/{i}.pdf");
        let bytes = format!("synthetic bytes, not a rendered PDF: {part}/{sheet}");
        fs::write(t.path().join(&name), bytes.as_bytes()).unwrap();
        let sha = oe_core::sha256(bytes);
        checks.push(json!({"file":name,"sha256":sha,"content_status":status,"layout_review_required":layout}));
        let p = json!({"sheet":sheet,"sha256":sha});
        if part == "A" {
            apdfs.push(p);
        } else {
            bpdfs.push(p);
        }
        let label = if status == "blank_template" {
            "空白图框，需确认或补图"
        } else if layout {
            "优先核对（排版）"
        } else {
            "已刷新，待工程审核"
        };
        rows.push(vec![
            (i + 1).to_string(),
            "set".into(),
            part.into(),
            sheet.into(),
            if assembly { "是" } else { "否" }.into(),
            name,
            label.into(),
            if layout {
                "图面排版：开发反例"
            } else {
                ""
            }
            .into(),
            "".into(),
        ]);
    }
    let part = |name: &str, pdfs: Vec<Value>| {
        json!({"set":"set","file":format!("{name}.prt"),"baseline_pdfs":[],"updated_pdfs":pdfs,
        "review_required":false,"errors":[],"after":{"stale_views":[],"retained_dimensions":[]}})
    };
    let native = json!({"finished":true,"export_complete":true,"errors":[],"sheet_count":3,"sets":[{
        "set":"set","inspected_part_files":["A.prt","B.prt"],"part_file_count":2,"load_errors":[],"assembly_part_files":["A.prt"],
        "drawing_inventory":{"A.prt":["SHT1","SHT2"],"B.prt":["SHT1"]}}],"parts":[part("A",apdfs),part("B",bpdfs)]});
    (t, native, Value::Array(checks), rows)
}
fn write(root: &Path, n: &Value, p: &Value, rows: &[Vec<String>]) {
    fs::write(
        root.join("验证记录/原生导出记录.json"),
        serde_json::to_vec(n).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("验证记录/PDF检查.json"),
        serde_json::to_vec(p).unwrap(),
    )
    .unwrap();
    let csv = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(",")
                + "\r\n"
        })
        .collect::<String>();
    fs::write(root.join("审核清单.csv"), csv).unwrap();
    let mut names = vec![
        "验证记录/原生导出记录.json".to_owned(),
        "验证记录/PDF检查.json".to_owned(),
        "审核清单.csv".to_owned(),
    ];
    names.extend(
        fs::read_dir(root.join("pdf"))
            .unwrap()
            .map(|e| format!("pdf/{}", e.unwrap().file_name().to_str().unwrap())),
    );
    names.sort();
    let sums = names
        .iter()
        .map(|n| {
            format!(
                "{}  {n}\n",
                oe_core::sha256(fs::read(root.join(n)).unwrap())
            )
        })
        .collect::<String>();
    fs::write(root.join("SHA256SUMS.txt"), sums).unwrap();
}
#[test]
fn full_packet_preserves_blank_layout_and_unreferenced_sheet_issues() {
    let (t, n, p, rows) = packet();
    write(t.path(), &n, &p, &rows);
    let r = audit_packet(t.path()).unwrap();
    assert_eq!(r["primary_sheets"], 3);
    assert_eq!(r["priority_review_sheets"], 2);
    assert_eq!(r["independent_sheets"], 1);
    assert_eq!(r["native_reexecution"], false);
    assert_eq!(r["visual_reinspection"], false);
    assert_eq!(r["engineering_acceptance"], "not_assessed");
}
#[test]
fn inventory_unknowns_and_issue_overrides_are_rejected_with_frozen_labels() {
    for case in 0..13 {
        let (t, mut n, mut p, mut rows) = packet();
        match case {
            0 => {
                rows.pop();
            }
            1 => rows[2][6] = "已刷新，待工程审核".into(),
            2 => rows[3][6] = "已刷新，待工程审核".into(),
            3 => n["parts"][0]["after"]["stale_views"] = json!(["stale"]),
            4 => p[0]["content_status"] = json!("unknown"),
            5 => rows[1][6] = "工程验收通过".into(),
            6 => {
                n["parts"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("review_required");
            }
            7 => rows[1][5] = rows[2][5].clone(),
            8 => rows[3][4] = "是".into(),
            9 => n["finished"] = json!(false),
            10 => {
                let duplicate = n["parts"][0].clone();
                n["parts"].as_array_mut().unwrap().push(duplicate);
            }
            11 => {
                n["parts"][0].as_object_mut().unwrap().remove("after");
            }
            12 => {
                p[2].as_object_mut()
                    .unwrap()
                    .remove("layout_review_required");
            }
            _ => unreachable!(),
        }
        write(t.path(), &n, &p, &rows);
        assert!(audit_packet(t.path()).is_err(), "case {case}");
    }
}
#[test]
fn hash_inventory_traversal_and_case_aliases_cannot_hide_missing_outputs() {
    let (t, n, p, rows) = packet();
    write(t.path(), &n, &p, &rows);
    fs::write(t.path().join("pdf/0.pdf"), "changed").unwrap();
    assert!(
        audit_packet(t.path())
            .unwrap_err()
            .to_string()
            .contains("hash_mismatch")
    );
    write(t.path(), &n, &p, &rows);
    fs::write(t.path().join("unlisted"), "extra").unwrap();
    assert!(audit_packet(t.path()).is_err());
    for name in ["../a", "/a", "c:\\a", "a:stream", "a//b", "a/./b"] {
        assert!(member(t.path(), name).is_err(), "{name}");
    }
    fs::remove_file(t.path().join("unlisted")).unwrap();
    fs::write(t.path().join("pdf/CASE.pdf"), "one").unwrap();
    fs::write(t.path().join("pdf/case.pdf"), "one").unwrap();
    // On a case insensitive host these address one file; an explicit duplicate
    // manifest still tests the transport alias rather than host behaviour.
    write(t.path(), &n, &p, &rows);
    let mut sums = fs::read_to_string(t.path().join("SHA256SUMS.txt")).unwrap();
    sums.push_str(&format!("{}  pdf/CaSe.pdf\n", oe_core::sha256("one")));
    fs::write(t.path().join("SHA256SUMS.txt"), sums).unwrap();
    assert!(
        audit_packet(t.path())
            .unwrap_err()
            .to_string()
            .contains("case_collision")
    );
}
