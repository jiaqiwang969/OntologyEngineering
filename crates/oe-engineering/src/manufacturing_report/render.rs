use super::{
    array, cost_result, decimal::Decimal, index, optional_changes, strings, text, validate,
};
use anyhow::{Result, ensure};
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedReport {
    pub tex: String,
    pub map: Value,
}
/// Escape supplied text, never permitting it to become a TeX command.
pub fn tex(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '\\' => out.push_str(r"\textbackslash{}"),
            '{' => out.push_str(r"\{"),
            '}' => out.push_str(r"\}"),
            '$' => out.push_str(r"\$"),
            '&' => out.push_str(r"\&"),
            '#' => out.push_str(r"\#"),
            '%' => out.push_str(r"\%"),
            '_' => out.push_str(r"\_"),
            '~' => out.push_str(r"\textasciitilde{}"),
            '^' => out.push_str(r"\textasciicircum{}"),
            '\n' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}
/// Preserve the whole locator; only insert wrapping opportunities in long
/// ASCII tokens. Unicode and original source content remain in expected_text.
pub fn source_tex(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at].is_ascii_alphanumeric() {
            let mut end = at + 1;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || "./:_-".contains(chars[end]))
            {
                end += 1;
            }
            if end - at >= 32 {
                let chunks = chars[at..end]
                    .chunks(8)
                    .map(|c| tex(&c.iter().collect::<String>()))
                    .collect::<Vec<_>>();
                out.push_str(&chunks.join(r"\allowbreak{}"));
                at = end;
                continue;
            }
        }
        out.push_str(&tex(&chars[at].to_string()));
        at += 1;
    }
    out
}
fn table(headers: &[&str], rows: &[Vec<String>], widths: &[&str]) -> String {
    let columns = widths
        .iter()
        .map(|w| format!("P{{{w}\\linewidth}}"))
        .collect::<String>();
    let head = format!("{} \\\\", headers.join(" & "));
    format!(
        "{{\\small\\begin{{longtable}}{{{columns}}}\n\\toprule {head}\n\\midrule\\endfirsthead\n\\toprule {head}\n\\midrule\\endhead\n\\midrule\\multicolumn{{{}}}{{r}}{{续下页}}\\endfoot\n\\bottomrule\\endlastfoot\n{}\n\\end{{longtable}}}}\n",
        headers.len(),
        rows.iter()
            .map(|r| format!("{} \\\\", r.join(" & ")))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
fn citations(v: &Value, key: &str) -> Result<String> {
    Ok(strings(v, key)?
        .into_iter()
        .map(|id| format!("\\hyperref[src-{id}]{{{}}}", tex(id)))
        .collect::<Vec<_>>()
        .join("、"))
}
fn value_text(v: &Value) -> String {
    if v.is_null() {
        "未记录".into()
    } else if let Some(s) = v.as_str() {
        s.into()
    } else {
        v.to_string()
    }
}
fn unresolved_placeholder(s: &str) -> bool {
    s.match_indices("@@").any(|(start, _)| {
        s[start + 2..].find("@@").is_some_and(|end| {
            end > 0
                && s[start + 2..start + 2 + end]
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b == b'_')
        })
    })
}
/// Render exactly the explicitly selected views. No disk, subprocess, source
/// attachment inspection, PDF verification or formal semantic execution occurs.
pub fn render(s: &Value, preamble: &str) -> Result<RenderedReport> {
    validate(s)?;
    ensure!(
        preamble.len() <= super::MAX_INPUT_BYTES,
        "report_preamble_exceeds_4_MiB"
    );
    let objects = index(array(s, "objects")?, "objects")?;
    let records = index(array(s, "records")?, "records")?;
    let relations = index(array(s, "relations")?, "relations")?;
    let d = &s["document"];
    let mut body = preamble
        .replace("@@DOCUMENT_ID@@", &tex(text(d, "id")?))
        .replace("@@REVISION@@", &tex(text(d, "revision")?))
        .replace("@@FOOTER@@", &tex(text(d, "footer")?));
    body.push_str(&format!(
        "\\hypersetup{{pdftitle={{{}}},pdfauthor={{}}}}\n",
        tex(text(d, "title")?)
    ));
    body.push_str(&format!(
        "{{\\Huge\\bfseries\\color{{Ink}}{}\\par}}\\vspace{{4mm}}\n",
        tex(text(d, "title")?)
    ));
    body.push_str(&format!(
        "{}\\par\n",
        tex(&format!("{} · {}", text(d, "revision")?, text(d, "date")?))
    ));
    let metadata = [
        ("本轮决定", text(d, "decision")?),
        ("阅读对象", text(d, "audience")?),
        ("项目状态", text(d, "project_state")?),
        ("依据范围", text(d, "evidence_scope")?),
    ];
    body.push_str(&table(
        &["项目", "说明"],
        &metadata
            .iter()
            .map(|(a, b)| vec![tex(a), tex(b)])
            .collect::<Vec<_>>(),
        &["0.17", "0.76"],
    ));
    body.push_str("\\small\\color{Muted}本报告呈现所提供记录及计算。当前快照未进行工程语义核验；另行验证的合成案例不能替代本报告。\\color{black}\\normalsize\\par\n\\tableofcontents\\clearpage\n");
    let mut expectations = vec![
        text(d, "title")?.to_owned(),
        text(d, "revision")?.to_owned(),
    ];
    expectations.extend(metadata.iter().map(|(_, b)| (*b).to_owned()));
    let mut mapping = Vec::new();
    for view in array(s, "views")? {
        let vid = text(view, "id")?;
        let kind = text(view, "kind")?;
        let title = text(view, "title")?;
        let question = text(view, "question")?;
        body.push_str(&format!(
            "\\section{{{}}}\\label{{view-{vid}}}\n\\textbf{{本节回答：}}{}\n\n",
            tex(title),
            tex(question)
        ));
        expectations.extend([title.into(), question.into()]);
        let mut selected = Vec::new();
        match kind {
            "records" => {
                let mut rows = Vec::new();
                for rid in strings(view, "items")? {
                    let r = records[rid];
                    selected.push(rid.to_owned());
                    let obj = strings(r, "object_ids")?
                        .into_iter()
                        .map(|id| {
                            let o = objects[id];
                            Ok(format!(
                                "{} / {} / {}",
                                text(o, "name")?,
                                text(o, "revision")?,
                                text(o, "state")?
                            ))
                        })
                        .collect::<Result<Vec<_>>>()?
                        .join("；");
                    let identity = format!(
                        "{}\\newline {}",
                        tex(&format!("{rid} · {}", text(r, "title")?)),
                        tex(text(r, "kind")?)
                    );
                    let statement = format!(
                        "{}\\newline\\textbf{{性质与状态：}}{}\\newline\\textbf{{条件：}}{}",
                        tex(text(r, "statement")?),
                        tex(&format!("{}；{}", text(r, "nature")?, text(r, "status")?)),
                        tex(text(r, "conditions")?)
                    );
                    rows.push(vec![
                        identity,
                        statement,
                        format!("{}\\newline {}", tex(&obj), citations(r, "source_ids")?),
                    ]);
                    for field in ["statement", "nature", "status", "conditions"] {
                        expectations.push(text(r, field)?.into());
                    }
                }
                if rows.is_empty() {
                    body.push_str("本视图尚无记录；不能据此判断没有问题。\n");
                } else {
                    body.push_str(&table(
                        &["条目", "当前记录与条件", "对象及来源"],
                        &rows,
                        &["0.2", "0.47", "0.24"],
                    ));
                }
            }
            "relations" => {
                let mut rows = Vec::new();
                for rid in strings(view, "items")? {
                    let r = relations[rid];
                    selected.push(rid.to_owned());
                    rows.push(vec![
                        tex(&format!("{} → {}", text(r, "from")?, text(r, "to")?)),
                        tex(text(r, "relation")?),
                        format!(
                            "{}\\newline {}",
                            tex(text(r, "reason")?),
                            citations(r, "source_ids")?
                        ),
                    ]);
                    expectations.push(text(r, "reason")?.into());
                }
                if rows.is_empty() {
                    body.push_str("本视图尚无关系记录；影响范围仍需核对。\n");
                } else {
                    body.push_str(&table(
                        &["起点与终点", "关系", "理由与来源"],
                        &rows,
                        &["0.23", "0.16", "0.52"],
                    ));
                }
            }
            "cost" => {
                if let Some(cost) = s.get("cost").filter(|c| !c.is_null()) {
                    let result = cost_result(Some(cost))?.expect("present validated cost");
                    let context = format!(
                        "数量：{}；{}。币种：{}；{}。",
                        value_text(&cost["quantity"]),
                        text(cost, "quantity_basis")?,
                        text(cost, "currency")?,
                        text(cost, "tax_basis")?
                    );
                    body.push_str(&format!(
                        "{}\n\n{}\n\n{}\\par 口径依据：{}\n\n",
                        tex(&context),
                        tex(text(cost, "scope")?),
                        tex(text(cost, "assumptions")?),
                        citations(cost, "source_ids")?
                    ));
                    expectations.extend([
                        context,
                        text(cost, "scope")?.into(),
                        text(cost, "assumptions")?.into(),
                    ]);
                    let mut rows = Vec::new();
                    for (line, calc) in array(cost, "lines")?.iter().zip(array(&result, "rows")?) {
                        let id = text(line, "id")?;
                        selected.push(id.into());
                        let basis = text(line, "basis")?;
                        let label = match basis {
                            "per_unit" => "按件",
                            "per_batch" => "按批",
                            _ => "一次性",
                        };
                        let factor = if basis == "per_unit" {
                            &cost["quantity"]
                        } else {
                            &line["count"]
                        };
                        let value = Decimal::amount(&line["value"])?
                            .map_or_else(|| "未知".into(), |d| d.money());
                        let subtotal = calc["subtotal"].as_str().unwrap_or("未计入");
                        let obj = strings(line, "object_ids")?.join("、");
                        rows.push(vec![
                            format!(
                                "{}\\newline {}",
                                tex(&format!("{id} · {}", text(line, "name")?)),
                                tex(&format!("对象：{obj}"))
                            ),
                            tex(&format!("{label}；{value} × {}", value_text(factor))),
                            tex(subtotal),
                            format!(
                                "{}\\newline {}",
                                tex(text(line, "nature")?),
                                citations(line, "source_ids")?
                            ),
                        ]);
                        expectations.extend([
                            text(line, "name")?.into(),
                            text(line, "nature")?.into(),
                            subtotal.into(),
                        ]);
                    }
                    body.push_str(&table(
                        &["费用项", "计价依据", "本批小计", "性质与来源"],
                        &rows,
                        &["0.24", "0.25", "0.15", "0.25"],
                    ));
                    let total = format!(
                        "按上述条件计算的小计：{} {}。",
                        text(&result, "known_subtotal")?,
                        text(cost, "currency")?
                    );
                    body.push_str(&format!("\\textbf{{{}}}\n\n", tex(&total)));
                    expectations.push(total);
                    let missing = if array(&result, "omitted_ids")?.is_empty() {
                        "所列费用均已有数值；完整交付范围仍按上述条件判断。".into()
                    } else {
                        format!("未计入项：{}", strings(&result, "omitted_ids")?.join("、"))
                    };
                    body.push_str(&format!(
                        "{}\n\n所列小计不自动成为对外售价或已实现收益。\n",
                        tex(&missing)
                    ));
                    expectations.push(missing);
                } else {
                    body.push_str("尚未提供费用明细，当前不能形成核算小计。\n");
                }
            }
            "changes" => {
                let mut rows = Vec::new();
                for r in optional_changes(s)? {
                    selected.push(text(r, "id")?.into());
                    rows.push(vec![
                        tex(text(r, "before")?),
                        tex(text(r, "after")?),
                        format!(
                            "{}\\newline {}\\newline {}",
                            tex(text(r, "reason")?),
                            tex(&format!("涉及：{}", strings(r, "affected_ids")?.join("、"))),
                            citations(r, "source_ids")?
                        ),
                    ]);
                    for f in ["before", "after", "reason"] {
                        expectations.push(text(r, f)?.into());
                    }
                }
                if rows.is_empty() {
                    body.push_str("初次记录，尚无版本间变化。\n");
                } else {
                    body.push_str(&table(
                        &["此前记录", "本轮记录", "原因、涉及条目与来源"],
                        &rows,
                        &["0.25", "0.25", "0.4"],
                    ));
                }
            }
            _ => unreachable!("validated view kind"),
        }
        let triggers = text(view, "update_triggers")?;
        body.push_str(&format!(
            "\\par\\textbf{{重新核查条件：}}{}\n",
            tex(triggers)
        ));
        expectations.push(triggers.into());
        mapping.push(json!({"view_id":vid,"kind":kind,"selected_ids":selected,"question":question,"update_triggers":triggers}));
        body.push_str("\\clearpage\n");
    }
    body.push_str("\\section{对象与资料依据}\\label{source-index}\n");
    let object_rows = array(s, "objects")?
        .iter()
        .map(|o| {
            Ok(vec![
                tex(&format!("{} · {}", text(o, "id")?, text(o, "name")?)),
                tex(&format!("{}；{}", text(o, "revision")?, text(o, "state")?)),
                citations(o, "source_ids")?,
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    body.push_str(&table(
        &["对象", "版本与状态", "依据"],
        &object_rows,
        &["0.35", "0.39", "0.16"],
    ));
    for src in array(s, "sources")? {
        let id = text(src, "id")?;
        body.push_str(&format!(
            "\\subsection*{{{}}}\\phantomsection\\label{{src-{id}}}\n",
            tex(&format!("{id} · {}", text(src, "title")?))
        ));
        for field in ["basis", "locator", "excerpt"] {
            let value = text(src, field)?;
            body.push_str(&source_tex(value));
            body.push_str("\n\n");
            expectations.push(value.into());
        }
    }
    body.push_str("\\end{document}\n");
    ensure!(
        !unresolved_placeholder(&body),
        "unresolved_template_placeholders"
    );
    let mut bindings = Map::new();
    for r in array(s, "records")? {
        bindings.insert(
            text(r, "id")?.into(),
            json!({"object_ids":r["object_ids"],"source_ids":r["source_ids"]}),
        );
    }
    for r in array(s, "relations")? {
        bindings.insert(
            text(r, "id")?.into(),
            json!({"endpoints":[r["from"],r["to"]],"source_ids":r["source_ids"]}),
        );
    }
    if let Some(cost) = s.get("cost").filter(|c| !c.is_null()) {
        for r in array(cost, "lines")? {
            bindings.insert(
                text(r, "id")?.into(),
                json!({"object_ids":r["object_ids"],"source_ids":r["source_ids"]}),
            );
        }
    }
    for r in optional_changes(s)? {
        bindings.insert(
            text(r, "id")?.into(),
            json!({"affected_ids":r["affected_ids"],"source_ids":r["source_ids"]}),
        );
    }
    Ok(RenderedReport {
        tex: body,
        map: json!({"views":mapping,"bindings":bindings,"expected_text":expectations,"cost":cost_result(s.get("cost"))?,"semantic_status":s["semantic_status"]}),
    })
}
