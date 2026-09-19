#!/usr/bin/env python3
"""Render source-bound manufacturing report views with XeLaTeX; no inference."""
from __future__ import annotations

import argparse
from decimal import Decimal, InvalidOperation
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "skills/manufacturing-process-cost/assets/report-template"
FORMAT = "ontology-engineering.manufacturing-report/v1"
ID = re.compile(r"[A-Za-z][A-Za-z0-9_-]{0,63}\Z")
DIAGNOSTICS = ("Missing character", "Overfull \\hbox", "Overfull \\vbox", "Underfull \\hbox", "Underfull \\vbox", "Undefined control sequence", "LaTeX Error", "undefined references")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def dump(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n"


def text(value):
    return "未记录" if value is None else str(value)


def tex(value):
    replacements = {"\\": r"\textbackslash{}", "{": r"\{", "}": r"\}", "$": r"\$", "&": r"\&", "#": r"\#", "%": r"\%", "_": r"\_", "~": r"\textasciitilde{}", "^": r"\textasciicircum{}"}
    return "".join(replacements.get(c, c) for c in text(value)).replace("\n", " ")


def source_tex(value):
    """Preserve complete source locators while allowing long ASCII tokens to wrap."""
    tokens = re.split(r"([A-Za-z0-9][A-Za-z0-9./:_-]{31,})", text(value))
    return "".join(
        r"\allowbreak{}".join(tex(token[i:i + 8]) for i in range(0, len(token), 8))
        if index % 2 else tex(token)
        for index, token in enumerate(tokens)
    )


def require(condition, message):
    if not condition:
        raise ValueError(message)


def identifiers(rows, collection):
    require(isinstance(rows, list), collection + " must be a list")
    result = {}
    for row in rows:
        require(isinstance(row, dict) and isinstance(row.get("id"), str) and ID.fullmatch(row["id"]), collection + " has an invalid ID")
        require(row["id"] not in result, collection + " repeats " + row["id"])
        result[row["id"]] = row
    return result


def refs(values, index, owner, allow_empty=False):
    require(isinstance(values, list) and (values or allow_empty), owner + " needs explicit references")
    require(len(values) == len(set(values)), owner + " repeats references")
    require(all(v in index for v in values), owner + " has dangling references")


def amount(value):
    if value is None:
        return None
    require(isinstance(value, (str, int)) and not isinstance(value, bool), "amounts must be decimal strings or integers")
    try:
        result = Decimal(value)
    except InvalidOperation as exc:
        raise ValueError("invalid decimal amount") from exc
    require(result.is_finite(), "non-finite decimal amount")
    return result


def validate(s):
    """Check serialization and declared references, not engineering truth."""
    require(s.get("format") == FORMAT, "unsupported report input format")
    d = s["document"]
    for name in ("id", "revision", "title", "date", "audience", "project_state", "evidence_scope", "decision", "footer"):
        require(isinstance(d.get(name), str) and d[name].strip(), "document." + name + " is required")
    require(ID.fullmatch(d["id"]), "invalid document ID")
    require(s.get("semantic_status") == "not_executed_for_this_snapshot", "this renderer cannot attest semantic execution; use a separately verified Semantica engagement")
    sources = identifiers(s["sources"], "sources")
    records = identifiers(s["records"], "records")
    objects = identifiers(s["objects"], "objects")
    relations = identifiers(s["relations"], "relations")
    views = identifiers(s["views"], "views")
    require(views, "at least one decision view is required")
    for source in sources.values():
        for field in ("title", "locator", "excerpt", "basis"):
            require(isinstance(source.get(field), str) and source[field].strip(), "source " + source["id"] + " lacks " + field)
    for obj in objects.values():
        for field in ("name", "revision", "state"):
            require(isinstance(obj.get(field), str) and obj[field].strip(), "object lacks " + field)
        refs(obj["source_ids"], sources, obj["id"])
    for row in records.values():
        for field in ("title", "kind", "statement", "nature", "status", "conditions"):
            require(isinstance(row.get(field), str) and row[field].strip(), row["id"] + " lacks " + field)
        refs(row["source_ids"], sources, row["id"])
        refs(row["object_ids"], objects, row["id"])
    nodes = {**objects, **records}
    require(len(nodes) == len(objects) + len(records), "object and record IDs must not overlap")
    for row in relations.values():
        require(row["from"] in nodes and row["to"] in nodes, "relation endpoint is missing")
        require(row.get("relation") and row.get("reason"), "relation and reason are required")
        refs(row["source_ids"], sources, row["id"])
    change_ids = identifiers(s.get("changes", []), "changes")
    for row in change_ids.values():
        for field in ("before", "after", "reason"):
            require(isinstance(row.get(field), str) and row[field].strip(), "change lacks " + field)
        refs(row["source_ids"], sources, row["id"])
        refs(row["affected_ids"], nodes, row["id"])
    for row in views.values():
        require(row.get("title") and row.get("question"), "view title/question is required")
        require(isinstance(row.get("update_triggers"), str) and row["update_triggers"].strip(), "view update_triggers is required")
        kind = row["kind"]
        require(kind in ("records", "relations", "cost", "changes"), "unknown view kind")
        if kind in ("records", "relations"):
            refs(row["items"], records if kind == "records" else relations, row["id"], allow_empty=True)
    cost = s.get("cost")
    if cost is not None:
        for field in ("currency", "tax_basis", "quantity_basis", "scope", "assumptions"):
            require(isinstance(cost.get(field), str) and cost[field].strip(), "cost." + field + " is required")
        require(cost["quantity"] is None or (type(cost["quantity"]) is int and cost["quantity"] > 0), "quantity must be a positive integer or null")
        lines = identifiers(cost["lines"], "cost lines")
        refs(cost["source_ids"], sources, "cost")
        for row in lines.values():
            require(row["basis"] in ("per_unit", "per_batch", "one_time"), "unknown fee basis")
            require(row.get("name") and row.get("nature"), "cost line name/nature required")
            require(type(row["count"]) is int and row["count"] > 0, "fee count must be positive")
            require(row["basis"] != "per_unit" or row["count"] == 1, "per-unit count must be 1; quantity is declared once at scenario level")
            amount(row["value"])
            refs(row["source_ids"], sources, row["id"])
            refs(row["object_ids"], objects, row["id"])
    all_row_ids = list(objects) + list(records) + list(relations) + list(change_ids)
    if cost is not None:
        all_row_ids += [r["id"] for r in cost["lines"]]
    require(len(all_row_ids) == len(set(all_row_ids)), "object, record, relation, change and cost IDs must not overlap")
    return sources, objects, records, relations


def cost_result(cost):
    """Decimal arithmetic over supplied fee bases; does not judge quote scope."""
    if cost is None:
        return None
    total = Decimal(0)
    missing = []
    rows = []
    for line in cost["lines"]:
        value = amount(line["value"])
        factor = cost["quantity"] if line["basis"] == "per_unit" else line["count"]
        subtotal = None if value is None or factor is None else value * factor
        if subtotal is None:
            missing.append(line["id"])
        else:
            total += subtotal
        rows.append({"id": line["id"], "subtotal": None if subtotal is None else f"{subtotal:.2f}"})
    return {"known_subtotal": f"{total:.2f}", "omitted_ids": missing, "rows": rows,
            "scope": "Only the explicitly supplied fee lines; completeness and price acceptance are not inferred."}


def table(headers, rows, widths):
    columns = "".join("P{" + str(w) + r"\linewidth}" for w in widths)
    head = " & ".join(headers) + r" \\"
    return (r"{\small\begin{longtable}{" + columns + "}\n" + r"\toprule " + head + "\n" +
            r"\midrule\endfirsthead" + "\n" + r"\toprule " + head + "\n" + r"\midrule\endhead" + "\n" +
            r"\midrule\multicolumn{" + str(len(headers)) + r"}{r}{续下页}\endfoot" + "\n" +
            r"\bottomrule\endlastfoot" + "\n" + "\n".join(" & ".join(row) + r" \\" for row in rows) +
            "\n" + r"\end{longtable}}" + "\n")


def render(s, preamble):
    sources, objects, records, relations = validate(s)
    d = s["document"]
    body = preamble.replace("@@DOCUMENT_ID@@", tex(d["id"])).replace("@@REVISION@@", tex(d["revision"])).replace("@@FOOTER@@", tex(d["footer"]))
    body += r"\hypersetup{pdftitle={" + tex(d["title"]) + r"},pdfauthor={}}" + "\n"
    body += r"{\Huge\bfseries\color{Ink}" + tex(d["title"]) + r"\par}\vspace{4mm}" + "\n"
    body += tex(d["revision"] + " · " + d["date"]) + r"\par" + "\n"
    metadata = [("本轮决定", d["decision"]), ("阅读对象", d["audience"]), ("项目状态", d["project_state"]), ("依据范围", d["evidence_scope"])]
    body += table(["项目", "说明"], [[tex(a), tex(b)] for a, b in metadata], [.17, .76])
    body += r"\small\color{Muted}本报告呈现所提供记录及计算。当前快照未进行工程语义核验；另行验证的合成案例不能替代本报告。\color{black}\normalsize\par" + "\n"
    body += r"\tableofcontents\clearpage" + "\n"
    expectations = [d["title"], d["revision"], *[b for _, b in metadata]]
    mapping = []
    def citations(ids):
        return "、".join(r"\hyperref[src-" + i + "]{" + tex(i) + "}" for i in ids)
    for view in s["views"]:
        vid = view["id"]
        body += r"\section{" + tex(view["title"]) + r"}\label{view-" + vid + "}\n"
        body += r"\textbf{本节回答：}" + tex(view["question"]) + "\n\n"
        expectations += [view["title"], view["question"]]
        kind = view["kind"]
        selected = []
        if kind == "records":
            rows = []
            for rid in view["items"]:
                r = records[rid]
                selected.append(rid)
                obj = "；".join(objects[i]["name"] + " / " + objects[i]["revision"] + " / " + objects[i]["state"] for i in r["object_ids"])
                identity = tex(rid + " · " + r["title"]) + r"\newline " + tex(r["kind"])
                statement = tex(r["statement"]) + r"\newline\textbf{性质与状态：}" + tex(r["nature"] + "；" + r["status"]) + r"\newline\textbf{条件：}" + tex(r["conditions"])
                basis = tex(obj) + r"\newline " + citations(r["source_ids"])
                rows.append([identity, statement, basis])
                expectations += [r["statement"], r["nature"], r["status"], r["conditions"]]
            if rows:
                body += table(["条目", "当前记录与条件", "对象及来源"], rows, [.20, .47, .24])
            else:
                body += "本视图尚无记录；不能据此判断没有问题。\n"
        elif kind == "relations":
            rows = []
            for rid in view["items"]:
                r = relations[rid]
                selected.append(rid)
                rows.append([tex(r["from"] + " → " + r["to"]), tex(r["relation"]), tex(r["reason"]) + r"\newline " + citations(r["source_ids"])])
                expectations.append(r["reason"])
            body += table(["起点与终点", "关系", "理由与来源"], rows, [.23, .16, .52]) if rows else "本视图尚无关系记录；影响范围仍需核对。\n"
        elif kind == "cost":
            cost = s.get("cost")
            if cost is None:
                body += "尚未提供费用明细，当前不能形成核算小计。\n"
            else:
                result = cost_result(cost)
                context = f"数量：{text(cost['quantity'])}；{cost['quantity_basis']}。币种：{cost['currency']}；{cost['tax_basis']}。"
                body += tex(context) + "\n\n" + tex(cost["scope"]) + "\n\n" + tex(cost["assumptions"]) + r"\par 口径依据：" + citations(cost["source_ids"]) + "\n\n"
                expectations += [context, cost["scope"], cost["assumptions"]]
                rows = []
                for line, calc in zip(cost["lines"], result["rows"]):
                    selected.append(line["id"])
                    label = {"per_unit": "按件", "per_batch": "按批", "one_time": "一次性"}[line["basis"]]
                    factor = cost["quantity"] if line["basis"] == "per_unit" else line["count"]
                    value = "未知" if line["value"] is None else f"{amount(line['value']):.2f}"
                    subtotal = "未计入" if calc["subtotal"] is None else calc["subtotal"]
                    obj = "、".join(line["object_ids"])
                    rows.append([tex(line["id"] + " · " + line["name"]) + r"\newline " + tex("对象：" + obj), tex(f"{label}；{value} × {text(factor)}"), tex(subtotal), tex(line["nature"]) + r"\newline " + citations(line["source_ids"])])
                    expectations += [line["name"], line["nature"], subtotal]
                body += table(["费用项", "计价依据", "本批小计", "性质与来源"], rows, [.24, .25, .15, .25])
                total_line = "按上述条件计算的小计：" + result["known_subtotal"] + " " + cost["currency"] + "。"
                body += r"\textbf{" + tex(total_line) + "}\n\n"
                expectations.append(total_line)
                missing = "未计入项：" + "、".join(result["omitted_ids"]) if result["omitted_ids"] else "所列费用均已有数值；完整交付范围仍按上述条件判断。"
                body += tex(missing) + "\n\n所列小计不自动成为对外售价或已实现收益。\n"
                expectations.append(missing)
        elif kind == "changes":
            rows = []
            for r in s.get("changes", []):
                selected.append(r["id"])
                rows.append([tex(r["before"]), tex(r["after"]), tex(r["reason"]) + r"\newline " + tex("涉及：" + "、".join(r["affected_ids"])) + r"\newline " + citations(r["source_ids"])])
                expectations += [r["before"], r["after"], r["reason"]]
            body += table(["此前记录", "本轮记录", "原因、涉及条目与来源"], rows, [.25, .25, .40]) if rows else "初次记录，尚无版本间变化。\n"
        body += r"\par\textbf{重新核查条件：}" + tex(view["update_triggers"]) + "\n"
        expectations.append(view["update_triggers"])
        mapping.append({"view_id": vid, "kind": kind, "selected_ids": selected, "question": view["question"], "update_triggers": view["update_triggers"]})
        body += r"\clearpage" + "\n"
    body += r"\section{对象与资料依据}\label{source-index}" + "\n"
    body += table(["对象", "版本与状态", "依据"], [[tex(i + " · " + o["name"]), tex(o["revision"] + "；" + o["state"]), citations(o["source_ids"])] for i, o in objects.items()], [.35, .39, .16])
    for sid, src in sources.items():
        body += r"\subsection*{" + tex(sid + " · " + src["title"]) + r"}\phantomsection\label{src-" + sid + "}\n"
        for v in (src["basis"], src["locator"], src["excerpt"]):
            body += source_tex(v) + "\n\n"
            expectations.append(v)
    body += r"\end{document}" + "\n"
    require(not re.search(r"@@[A-Z_]+@@", body), "unresolved template placeholders")
    bindings = {r["id"]: {"object_ids": r["object_ids"], "source_ids": r["source_ids"]} for r in s["records"]}
    bindings.update({r["id"]: {"endpoints": [r["from"], r["to"]], "source_ids": r["source_ids"]} for r in s["relations"]})
    if s.get("cost"):
        bindings.update({r["id"]: {"object_ids": r["object_ids"], "source_ids": r["source_ids"]} for r in s["cost"]["lines"]})
    bindings.update({r["id"]: {"affected_ids": r["affected_ids"], "source_ids": r["source_ids"]} for r in s.get("changes", [])})
    return body, {"views": mapping, "bindings": bindings, "expected_text": expectations, "cost": cost_result(s.get("cost")), "semantic_status": s["semantic_status"]}


def command(argv, cwd):
    # Only document tools are callable here; keep executables statically visible.
    if argv[0] == "xelatex":
        result = subprocess.run(["xelatex", *argv[1:]], cwd=cwd, text=True, capture_output=True)
    elif argv[0] == "pdftotext":
        result = subprocess.run(["pdftotext", *argv[1:]], cwd=cwd, text=True, capture_output=True)
    elif argv[0] == "pdffonts":
        result = subprocess.run(["pdffonts", *argv[1:]], cwd=cwd, text=True, capture_output=True)
    elif argv[0] == "pdfinfo":
        result = subprocess.run(["pdfinfo", *argv[1:]], cwd=cwd, text=True, capture_output=True)
    else:
        raise ValueError("unsupported document tool")
    require(result.returncode == 0, Path(argv[0]).name + " failed:\n" + (result.stdout + result.stderr)[-3500:])
    return result.stdout


def generate(source, output, compile_pdf=False):
    require(not output.exists(), "output exists; create a successor directory")
    require(not output.resolve().is_relative_to(ROOT), "generated reports belong outside the distributed skill")
    source_bytes = source.read_bytes()
    require(len(source_bytes) <= 4 * 1024 * 1024, "report input exceeds 4 MiB")
    s = json.loads(source_bytes)
    preamble = (ASSETS / "preamble.tex").read_text()
    body, mapping = render(s, preamble)
    contract = (ASSETS / "contract.json").read_bytes()
    if compile_pdf:
        for tool in ("xelatex", "pdftotext", "pdfinfo", "pdffonts"):
            require(shutil.which(tool), "required tool missing: " + tool)
    output.mkdir(parents=True)
    (output / "snapshot.json").write_bytes(source_bytes)
    (output / "template.tex").write_text(preamble)
    (output / "contract.json").write_bytes(contract)
    (output / "report.tex").write_text(body)
    (output / "render-map.json").write_text(dump(mapping))
    files = ["snapshot.json", "template.tex", "contract.json", "report.tex", "render-map.json"]
    if compile_pdf:
        for _ in range(3):
            command(["xelatex", "-no-shell-escape", "-interaction=nonstopmode", "-halt-on-error", "-file-line-error", "report.tex"], output)
        command(["pdftotext", "-layout", "report.pdf", "report.txt"], output)
        (output / "fonts.txt").write_text(command(["pdffonts", "report.pdf"], output))
        (output / "pdfinfo.txt").write_text(command(["pdfinfo", "report.pdf"], output))
        files += ["report.pdf", "report.txt", "report.log", "fonts.txt", "pdfinfo.txt"]
    manifest = {"format": "ontology-engineering.manufacturing-report-delivery/v1", "generator_sha256": digest(Path(__file__).read_bytes()),
                "input_sha256": digest(source_bytes), "files": [{"path": name, "sha256": digest((output / name).read_bytes())} for name in files],
                "document_id": s["document"]["id"], "revision": s["document"]["revision"], "compiled": compile_pdf,
                "semantic_status": s["semantic_status"], "scope": "Frozen report artifact, not engineering approval or a semantic receipt."}
    (output / "manifest.json").write_text(dump(manifest))
    result = verify(output)
    (output / "verification.json").write_text(dump(result))
    return result


def norm(value):
    return re.sub(r"\s+", "", value)


def verify(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    require(manifest.get("format") == "ontology-engineering.manufacturing-report-delivery/v1", "unsupported delivery format")
    issues = []
    require(manifest["generator_sha256"] == digest(Path(__file__).read_bytes()), "use the archived generator version to verify this report")
    expected_files = {"snapshot.json", "template.tex", "contract.json", "report.tex", "render-map.json"}
    if manifest["compiled"]:
        expected_files |= {"report.pdf", "report.txt", "report.log", "fonts.txt", "pdfinfo.txt"}
    require({r["path"] for r in manifest["files"]} == expected_files and len(manifest["files"]) == len(expected_files), "delivery file inventory differs from required report artifacts")
    for row in manifest["files"]:
        name = row["path"]
        require(Path(name).name == name and name not in (".", ".."), "unsafe manifest member")
        p = directory / name
        if p.is_symlink() or not p.is_file() or digest(p.read_bytes()) != row["sha256"]:
            issues.append("file changed: " + name)
    if issues:
        return {"passed": False, "issues": issues, "scope": "Report artifact checks only."}
    source_bytes = (directory / "snapshot.json").read_bytes()
    require(digest(source_bytes) == manifest["input_sha256"], "snapshot identity mismatch")
    s = json.loads(source_bytes)
    body, mapping = render(s, (directory / "template.tex").read_text())
    if body != (directory / "report.tex").read_text():
        issues.append("generated TeX does not correspond to this snapshot")
    if mapping != json.loads((directory / "render-map.json").read_text()):
        issues.append("render map does not correspond to this snapshot")
    page_count = None
    if manifest["compiled"]:
        # Raw content order preserves each cell; -layout interleaves adjacent columns.
        extracted = command(["pdftotext", "-raw", "report.pdf", "-"], directory)
        haystack = norm(extracted)
        for item in mapping["expected_text"]:
            if norm(item) not in haystack:
                issues.append("expected text missing: " + item[:100])
        log = (directory / "report.log").read_text()
        for item in DIAGNOSTICS:
            if item in log:
                issues.append("XeLaTeX diagnostic: " + item)
        if "??" in extracted:
            issues.append("unresolved PDF cross reference")
        fonts = command(["pdffonts", "report.pdf"], directory).splitlines()[2:]
        if not fonts or any(row.split()[-5] != "yes" for row in fonts if row.strip()):
            issues.append("font not embedded")
        info = command(["pdfinfo", "report.pdf"], directory)
        found = re.search(r"^Pages:\s+(\d+)", info, re.M)
        page_count = int(found[1]) if found else None
    return {"passed": not issues, "issues": issues, "views": len(mapping["views"]), "text_expectations": len(mapping["expected_text"]),
            "pages": page_count, "compiled": manifest["compiled"], "input_sha256": manifest["input_sha256"],
            "pdf_sha256": digest((directory / "report.pdf").read_bytes()) if manifest["compiled"] else None,
            "semantic_status": s["semantic_status"], "visual_review": "not_performed_by_this_script", "scope": "Snapshot/render correspondence, arithmetic display and PDF technical checks. Engineering semantics, source truth and visual review are separate."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    gen = sub.add_parser("generate")
    gen.add_argument("--input", type=Path, required=True)
    gen.add_argument("--output", type=Path, required=True)
    gen.add_argument("--compile", action="store_true")
    chk = sub.add_parser("verify")
    chk.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        result = generate(args.input.resolve(), args.output.resolve(), args.compile) if args.action == "generate" else verify(args.directory.resolve())
        print(dump(result), end="")
        return 0 if result["passed"] else 1
    except (ValueError, KeyError, TypeError, OSError, json.JSONDecodeError) as exc:
        print(dump({"passed": False, "error": str(exc)}), end="")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
