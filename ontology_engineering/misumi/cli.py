#!/usr/bin/env python3
"""One knowledge workflow over registered textbook and supplier sources."""
from __future__ import annotations

import argparse
import collections
from contextlib import ExitStack
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import sys
import time
import uuid

from .catalog import (CATALOG_CODES, LABELS, VERSION, build_index, chapters, connect, digest,
                     metadata, now, retrieve, source_result, write_json)
from .judger import Judge, JevFailure
from .knowledge import Knowledge
from .paths import runtime_config, readiness, prepare_state, default_data_root
from . import paths
from ..supplier_knowledge import load_context, make_packet
from ..knowledge_context import project_context, verify_context_sources

RESULT_SCHEMA = "misumi.search-result/v1"
LIMITATIONS = [
    "结果是工程资料候选；方法适用性、尺寸、公差和具体配置尚未逐项核验。",
    "2022/2023年目录不能证明当前库存、价格或交货承诺。",
    "原生正文与派生导航分开；扫描页、原理图、公式及复杂表格尚未全面OCR或结构化。",
    "Jev只筛选已召回的候选页；未返回不代表全目录不存在，模型概率不是合格率。",
]


def _providers(config, args, stack):
    """Open independent source indexes; never alter the frozen supplier pack."""
    selection = getattr(args, "sources", "misumi")
    local_root = Path(getattr(args, "pdf_root", None) or default_data_root("local-pdf"))
    use_misumi = selection != "local-pdf"
    use_local = selection in {"all", "local-pdf"} or (selection == "auto" and
        (getattr(args, "pdf_root", None) or (not getattr(args, "data_root", None)
         and args.book == "all" and local_root.exists())))
    providers, statuses = [], []
    if use_misumi:
        ready = readiness(config)
        statuses.append(ready)
        if ready["status"] == "ready":
            db = connect(config["index_path"])
            stack.callback(db.close)
            catalog_filter = {"all": None, "standard": CATALOG_CODES[0], "economy": CATALOG_CODES[1]}[args.book]
            providers.append({"id": "misumi_archive", "meta": metadata(db),
                "lookup": lambda identifier: db.execute("SELECT * FROM pages WHERE id=?", (identifier,)).fetchone(),
                "chapters": chapters(db, catalog_filter),
                "retrieve": lambda knowledge, route, limit: knowledge.retrieve(db, args.query, route, limit, catalog_filter)
                            if knowledge else retrieve(db, args.query, limit, catalog=catalog_filter),
                "source_result": lambda page: source_result(db, page, config["archive_root"], args.query)})
    if use_local:
        from ..local_pdf_sources import LocalPDFSources
        if Path(config["state_root"]).is_relative_to(local_root.resolve()):
            raise ValueError("knowledge_state_root_must_be_outside_source_data")
        local = LocalPDFSources(local_root)
        stack.callback(local.close)
        if getattr(args, "_enforce_skill_paths", False):
            for source in local.sources.values():
                original = Path(source["path"])
                paths.skill_path(original if original.is_absolute() else local.root / original,
                                 "knowledge_pdf_original", root=paths.SKILL_ROOT)
        statuses.append(local.status())
        if local.status()["status"] == "ready":
            providers.append({"id": "local_pdf", "meta": local.meta, "chapters": local.chapters(),
                "lookup": lambda identifier: local.db.execute("SELECT * FROM pages WHERE id=?", (identifier,)).fetchone(),
                "retrieve": lambda knowledge, route, limit: local.retrieve(args.query, route, limit),
                "source_result": lambda page: local.source_result(page, args.query)})
    return providers, statuses


def registered_page_source(identifier, args):
    """Resolve one exact registered page without retrieval or model judgment."""
    view_args = argparse.Namespace(**vars(args))
    is_local = identifier.startswith("local_pdf:")
    view_args.sources, view_args.query = "local-pdf" if is_local else "misumi", ""
    config = runtime_config(args.data_root, args.state_root, include_misumi=not is_local)
    for name in ("data_root", "index_path", "archive_root"):
        paths.skill_path(config[name], "knowledge_" + name, root=paths.SKILL_ROOT)
    with ExitStack() as stack:
        providers, _ = _providers(config, view_args, stack)
        for provider in providers:
            row = provider["lookup"](identifier)
            if row is None:
                continue
            return provider["source_result"](dict(row))
    raise ValueError("source_view_page_unavailable")


def relocated_record_source(result, args):
    """Resolve a historical citation through current registrations, without rewriting it."""
    try:
        current = registered_page_source(result["id"], args)
    except ValueError as exc:
        if str(exc) == "source_view_page_unavailable":
            raise ValueError("source_view_record_source_unavailable") from None
        raise
    required = ("source_sha256", "pdf_page")
    checked = required + tuple(k for k in ("source_locator", "text_sha256") if k in result)
    if any(result.get(k) != current.get(k) for k in checked):
        raise ValueError("source_view_record_identity_changed")
    return current


def _not_ready(args, statuses):
    missing = [{"code": item.get("kind", item.get("code", "source_not_ready"))}
               for status in statuses for item in status.get("missing", status.get("errors", []))]
    ready = statuses[0] if len(statuses) == 1 and "setup" in statuses[0] else {
        "status": "not_ready", "sources": statuses,
        "setup": {"instruction": "Import the Drive downloads with scripts/source_library.py, register books there, and install sources/misumi with runtime/misumi/setup.py. See docs/PORTABLE-DISTRIBUTION.md."}}
    return {"schema": RESULT_SCHEMA, "status": "not_ready", "query": args.query,
            "provider": "registered_sources", "readiness": ready, "results": [], "errors": missing,
            "parameter_verification": "not_run", "usage": {"network_requests": 0, "cache_hits": 0}}


def search(config, args, state_root=None):
    started = time.monotonic()
    if state_root is not None:
        config = runtime_config(config["data_root"], state_root)
    with ExitStack() as stack:
        providers, statuses = _providers(config, args, stack)
        if not providers:
            return _not_ready(args, statuses)
        root = prepare_state(config["state_root"])
        combined = len(providers) > 1 or providers[0]["id"] != "misumi_archive"
        indexes = [{"provider": p["id"], **{k: p["meta"][k] for k in ("fingerprint", "built_at", "counts")}}
                   for p in providers]
        meta = providers[0]["meta"] if not combined else {
            "fingerprint": digest(indexes), "built_at": max(p["meta"]["built_at"] for p in providers),
            "counts": dict(sum((collections.Counter(p["meta"]["counts"]) for p in providers), collections.Counter()))}
        report = {"schema": "ontology-engineering.knowledge-search-result/v1" if combined else RESULT_SCHEMA,
                  "version": VERSION, "query": args.query, "created_at": now(), "status": "running",
                  "mode": "local" if args.local else "jev", "model": None if args.local else config["model"],
                  "index": {k: meta[k] for k in ("fingerprint", "built_at", "counts")}, "source_indexes": indexes,
                  "source_readiness": statuses,
                  "retrieval": {"method": "context_topics_body_fts_plus_jev_chapters", "book": args.book,
                                "candidate_limit": args.candidates, "full_corpus_semantic_scan": False},
                  "parameter_verification": "not_run", "live_information": "not_available_from_archive",
                  "limitations": LIMITATIONS, "results": [], "errors": []}
        if getattr(args, "sources", None) in {"all", "local-pdf"}:
            report["errors"] = [{"provider": s["provider"], "code": item.get("kind", item.get("code"))}
                                for s in statuses for item in s.get("missing", s.get("errors", []))]
        judge = route = knowledge = None
        try:
            if not args.local:
                judge = Judge(root, config, meta["fingerprint"], args.no_cache)
            navigation = [chapter for provider in providers for chapter in provider["chapters"]]
            context_input = getattr(args, "context", None)
            if context_input or getattr(args, "knowledge", False):
                context_input = context_input or {"schema": "ontology-engineering.supplier-knowledge-context/v1",
                    "task_id": "ad-hoc-" + digest(args.query)[:16], "goal": args.query}
                knowledge = Knowledge(config, context_input, judge)
                report.update(context_sha256=knowledge.sha, context_snapshot=knowledge.context,
                              knowledge_policy_sha256=knowledge.policy_sha,
                              method_bundle=knowledge.method_bundle, method_bundle_sha256=knowledge.policy_sha)
                freshness = verify_context_sources(knowledge.context, paths.SKILL_ROOT)
                report["project_source_freshness"] = freshness
                if freshness["status"] == "stale":
                    report.update(status="stale_context", errors=[{"code": "knowledge_context_sources_stale"}])
                    return finish(report, judge, started, root, knowledge)
                route = knowledge.route(args.query, navigation)
                report["knowledge_gaps"] = knowledge.gaps(route)
            elif judge:
                route = judge.route(args.query, navigation)
            if route:
                report["routing"] = route
                report["errors"].extend(route.get("errors", []))
                if route["scope"] == "live_only":
                    report.update(status="unsupported_live_query", message="离线资料没有当前库存、价格或交货证据。")
                    report["retrieval"]["candidates_retrieved"] = 0
                    return finish(report, judge, started, root, knowledge, route)
                if knowledge and route["scope"] == "outside":
                    report["status"] = "no_match_in_candidates"
                    report["retrieval"]["candidates_retrieved"] = 0
                    return finish(report, judge, started, root, knowledge, route)
                if knowledge and route["scope"] == "unclear" and not route["needed_topics"] and not route["chapters"]:
                    report.update(status="insufficient_context", message="当前对象和资料用途尚不明确；请续接项目背景。")
                    report["retrieval"]["candidates_retrieved"] = 0
                    return finish(report, judge, started, root, knowledge, route)
            elif args.local:
                report["retrieval"]["method"] = "unicode_ngrams_fts5_bm25"
            pools = []
            quotas = {}
            for i, provider in enumerate(providers):
                quota = args.candidates // len(providers) + int(i < args.candidates % len(providers))
                quotas[provider["id"]] = quota
                try:
                    pool = provider["retrieve"](knowledge, route, quota)
                    pools.append([{**page, "provider_id": provider["id"]} for page in pool])
                except (ValueError, OSError, sqlite3.Error) as exc:
                    report["errors"].append({"provider": provider["id"], "code": str(exc).split(":", 1)[0]})
            pages = [pool[i] for i in range(args.candidates) for pool in pools if i < len(pool)]
            report["retrieval"].update(candidates_retrieved=len(pages), source_quotas=quotas,
                                       cross_source_order="round_robin_not_comparable_bm25")
            report["retrieval"]["routed_chapter_candidate_counts"] = {
                chapter["id"]: sum(page["catalog"] == chapter["catalog"]
                    and chapter["start_page"] <= page["pdf_page"] <= chapter["end_page"] for page in pages)
                for chapter in (route or {}).get("chapters", [])}
            if not pages:
                report["status"] = "partial" if report["errors"] else "no_match_in_candidates"
                return finish(report, judge, started, root, knowledge, route)
            readable = [p for p in pages if not p.get("navigation_only")]
            if knowledge:
                evaluations = knowledge.screen(args.query, readable, route)
            elif args.local:
                evaluations = [{"id": p["id"], "relation": "lexical_only", "evaluation": "not_run",
                                "document_type": "unknown", "model_answer": None, "errors": []} for p in readable]
            else:
                evaluations = judge.screen(args.query, readable)
            evaluations.extend({"id": p["id"], "relation": "navigation_only", "evaluation": "not_run",
                "document_type": "navigation", "knowledge_roles": [], "model_answer": None, "chunks": [], "errors": []}
                for p in pages if p.get("navigation_only"))
            report["evaluations"] = evaluations
            report["retrieval"]["relation_counts"] = dict(collections.Counter(e["relation"] for e in evaluations))
            for e in evaluations:
                report["errors"].extend({"page_id": e["id"], "code": c} for c in e["errors"])
            by_id = {p["id"]: p for p in pages}
            ranks = {"direct": 0, "related": 1, "lexical_only": 2, "navigation_only": 3}
            accepted = [e for e in evaluations if e["relation"] in ranks]
            def order(e):
                p = by_id[e["id"]]
                return knowledge.rank(e, route, p) if knowledge else (
                    ranks[e["relation"]], 0 if e["document_type"] == "specification" else 1,
                    -(e["model_answer"] or {}).get("probabilities", {}).get("direct", 0),
                    p["lexical_score"] if p["lexical_score"] is not None else 0, e["id"])
            ordered = []
            for relation in ranks:
                pools = [sorted((e for e in accepted if e["relation"] == relation and
                          by_id[e["id"]]["provider_id"] == provider["id"]), key=order) for provider in providers]
                ordered.extend(pool[i] for i in range(len(accepted)) for pool in pools if i < len(pool))
            resolvers = {p["id"]: p["source_result"] for p in providers}
            for e in ordered:
                p = by_id[e["id"]]
                try:
                    candidate = resolvers[p["provider_id"]](p)
                except (ValueError, OSError) as exc:
                    report["errors"].append({"page_id": p["id"], "code": str(exc).split(":", 1)[0]})
                    continue
                candidate["provider"] = p["provider_id"]
                candidate["judgment"] = {k: v for k, v in e.items() if k not in ("id", "chunks")}
                if knowledge:
                    candidate["knowledge_units"] = knowledge.units(p, e, route)
                    candidate["retrieval_pool"] = p["retrieval_pool"]
                if args.full_text:
                    candidate["native_text"] = p["body"]
                report["results"].append(candidate)
                if len(report["results"]) >= args.top:
                    break
            report["status"] = "partial" if report["errors"] else "completed" if report["results"] else "no_match_in_candidates"
        except JevFailure as exc:
            report["status"] = "failed"
            report["errors"].append({"code": exc.code})
        return finish(report, judge, started, root, knowledge, route)


def finish(report, judge, started, root, knowledge=None, route=None):
    if knowledge:
        freshness = verify_context_sources(knowledge.context, paths.SKILL_ROOT)
        if freshness["status"] != "stale":
            report["review_plan"] = knowledge.review(report["query"], report["results"], route)
            if report["review_plan"]["status"] == "failed":
                report["errors"].extend({"stage": "next_inquiry_review", **error}
                                        for error in report["review_plan"]["errors"])
                if report["status"] != "failed":
                    report["status"] = "partial"
            freshness = verify_context_sources(knowledge.context, paths.SKILL_ROOT)
        report["project_source_freshness"] = freshness
        if freshness["status"] == "stale":
            report["status"] = "stale_context"
            if not any(e.get("code") == "knowledge_context_sources_stale" for e in report["errors"]):
                report["errors"].append({"code": "knowledge_context_sources_stale"})
    report["elapsed_seconds"] = round(time.monotonic() - started, 3)
    report["usage"] = dict(judge.stats) if judge else {"network_requests": 0, "cache_hits": 0, "input_tokens": 0, "output_tokens": 0, "usage_unreported_attempts": 0}
    if judge:
        report["calls"] = list(judge.calls)
    report["run_id"] = uuid.uuid4().hex
    history = root / "history" / (report["run_id"] + ".json")
    report["record_file"] = str(history)
    write_json(history, report)
    return report


def print_report(report):
    print("查询：" + report["query"])
    if report["status"] == "not_ready":
        print("资料尚未就绪：" + ", ".join(e["code"] for e in report["errors"]))
        print(report["readiness"]["setup"]["instruction"])
        return
    if report.get("context_sha256"):
        context = report.get("context_snapshot", {})
        if (report.get("project_source_freshness", {}).get("status") == "not_bound"
                and (context.get("project", {}).get("id") or context.get("object", {}).get("id")
                     or context.get("decision_id"))):
            print("项目来源：未绑定原记录；仅核对传入快照，尚未核验原项目当前状态。")
        labels = {"mechanism": "功能与机理", "selection": "选型比较", "calculation": "计算与边界",
                  "interfaces": "接口装配", "failure": "失效维护", "examples": "应用经验", "specification": "型号规格"}
        route = report.get("routing", {})
        topics = route.get("needed_topics", [])
        print("当前工程主题：" + ("、".join(labels[t] for t in topics) if topics else "未作出确定判断"))
    if report["status"] in ("unsupported_live_query", "insufficient_context"):
        print(report["message"])
    if report["status"] == "stale_context":
        print("项目来源记录或映射已经变化；结果只保留为历史候选，请从当前记录重新查询。")
    for i, r in enumerate(report["results"], 1):
        relation = r["judgment"]["relation"]
        label = {"direct": "Jev优先候选", "related": "关联候选", "lexical_only": "本地关键词候选",
                 "navigation_only": "导航候选，需查看原图"}[relation]
        title = " / ".join(r["headings"][:2]) or r["chapter"]
        print(f"\n{i}. [{label}] {title}")
        printed = r['printed_page'] if r['printed_page'] is not None else "未核"
        print(f"   {r['catalog_title']} · 印刷页 {printed} · PDF 第 {r['pdf_page']} 页")
        if "knowledge_units" in r:
            roles = r["judgment"].get("knowledge_roles", [])
            print("   资料内容：" + ("、".join(labels[t] for t in roles) or "尚未分类") + "；适用性待审查")
        seen = list(dict.fromkeys(s["keyword"] for s in r["series_references"]))
        if seen:
            print("   页面型号/系列：" + "、".join(seen[:10]) + (f" 等 {len(seen)} 项" if len(seen) > 10 else ""))
        # This is copied text, not a generated answer or a verified parameter table.
        text = re.sub(r"\s+", " ", r["excerpt"]["text"]).strip()
        print("   原文节选：" + text[:210])
        print("   原页文件：" + r["source_pdf"])
    if not report["results"] and report["status"] not in ("unsupported_live_query", "insufficient_context"):
        print("本次候选中没有可返回的相关资料。" if report["status"] != "failed" else "Jev 查询失败，未将失败解释为没有匹配。")
    review = report.get("review_plan")
    if review:
        action_labels = {action["id"]: action["label"] for action in review["actions"]}
        selected = "、".join(action_labels[aid] for aid in review["selected_actions"])
        if review["status"] == "completed":
            print("下一核查候选：" + (selected or "尚无确定候选") + "；待 agent 采用，均未执行。")
            if review["uncertain_actions"]:
                print("仍待判断：" + "、".join(action_labels[aid] for aid in review["uncertain_actions"]))
        elif review["status"] == "not_run":
            print("下一核查候选：未作模型判断，均未执行。")
        else:
            print("下一核查候选：判断失败，均未执行；保留已返回的资料。")
    if report["errors"]:
        print("查询未完整完成：" + ", ".join(sorted({e["code"] for e in report["errors"]})))
    print("\n资料用于定位原文；具体尺寸/公差等条件未核验，库存价格也不是实时数据。")
    usage = report["usage"]
    print(f"检索 {report['retrieval'].get('candidates_retrieved', 0)} 个候选页 · {report['elapsed_seconds']} 秒 · Jev 请求 {usage['network_requests']} · 缓存 {usage['cache_hits']}")
    print("记录：" + report["record_file"])
    for view in report.get("page_views", []):
        print("原页图片：" + view["image"])


def bounded_int(low, high):
    def parse(value):
        try:
            result = int(value)
        except ValueError:
            raise argparse.ArgumentTypeError("需要整数") from None
        if not low <= result <= high:
            raise argparse.ArgumentTypeError(f"范围为 {low}–{high}")
        return result
    return parse


def save_output(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def current_context(args):
    """Read this turn's selected project fields without changing the records."""
    projection = getattr(args, "project_context", None)
    if projection is not None:
        if projection.stat().st_size > 64000:
            raise ValueError("knowledge_context_projection_too_large")
        return project_context(json.loads(projection.read_text(encoding="utf-8")), paths.SKILL_ROOT)
    return load_context(args.context)


def main(argv=None, default_knowledge=True):
    parser = argparse.ArgumentParser(description="按当前工程问题，用共享 Jev 方法检索内置资料并提出下一核查候选。")
    parser.add_argument("query", nargs="?", help="自然语言工程问题；建议用引号包住")
    parser.add_argument("--json", action="store_true", help="输出结构化 JSON")
    parser.add_argument("--local", action="store_true", help="仅本地检索，不调用 Jev")
    contexts = parser.add_mutually_exclusive_group()
    contexts.add_argument("--context", type=Path, help="原项目上下文快照；未绑定来源的快照不证明当前项目状态")
    contexts.add_argument("--project-context", type=Path, metavar="PROJECTION", help="从 var/projects 原记录按显式字段映射生成本轮上下文并核验来源")
    parser.add_argument("--knowledge", action="store_true", default=default_knowledge, help="启用工程知识模式（统一入口默认开启）")
    parser.add_argument("--output", type=Path, help="写入新的私有来源证据包，不覆盖既有文件")
    parser.add_argument("--data-root", type=Path, help="包含 catalog.sqlite3 和 archive/ 的数据目录")
    parser.add_argument("--sources", choices=("auto", "all", "misumi", "local-pdf"), default="auto",
                        help="auto 查询已登记来源；显式 --data-root 或 --book 时保持米思米范围")
    parser.add_argument("--pdf-root", type=Path, help="本地教材的私有登记与索引目录")
    parser.add_argument("--state-root", type=Path, help="skill 内的私有状态目录；默认 var/state/misumi，缓存默认 var/cache/misumi")
    parser.add_argument("--top", type=bounded_int(1, 20), default=5)
    parser.add_argument("--candidates", type=bounded_int(8, 96), default=24)
    parser.add_argument("--book", choices=("all", "standard", "economy"), default="all")
    parser.add_argument("--no-cache", action="store_true")
    parser.add_argument("--full-text", action="store_true")
    parser.add_argument("--open", type=bounded_int(1, 20), metavar="N", help="用系统 PDF 阅读器打开第 N 项原页")
    parser.add_argument("--render", type=bounded_int(1, 20), metavar="N", help="将第 N 项原页渲染为图片并保留来源身份")
    parser.add_argument("--render-dpi", type=bounded_int(72, 300), default=144)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--status", action="store_true", help="只读检查数据就绪状态")
    group.add_argument("--reindex", action="store_true", help="从 archive/ 校验来源并原子重建索引（需要 PyMuPDF）")
    group.add_argument("--register-pdfs", type=Path, metavar="MANIFEST", help="登记本地PDF并建立原生文字索引（需要Poppler）")
    group.add_argument("--view-record", type=Path, metavar="RECORD", help="从查询记录或证据包查看原页；配合 --render N")
    group.add_argument("--view-page", metavar="PAGE_ID", help="按已登记页 ID 直接渲染原页，不受查询排名限制；local_pdf:<书ID>:page:<物理页> 或米思米页 ID")
    group.add_argument("--verify-packet", type=Path, metavar="PACKET", help="核对知识包与 --context 或 --project-context 的当前决定及声明来源")
    parser.add_argument("--replace-sources", action="store_true", help="显式重建已经登记的本地PDF索引，原PDF保持不变")
    parser.add_argument("--version", action="version", version="engineering Jev knowledge / MISUMI provider " + VERSION)
    args = parser.parse_args(argv)
    if args.view_page is not None:
        if not args.view_page.strip():
            parser.error("--view-page 需要非空的已登记页 ID")
        if args.open is not None:
            parser.error("--view-page 直接渲染指定原页，不能与 --open N 组合")
    operation = args.status or args.reindex or args.register_pdfs or args.view_record or args.view_page or args.verify_packet
    if not operation and not args.query:
        parser.print_help()
        return 0
    if args.query and operation:
        parser.error("来源管理、记录查看或校验操作不接受检索词")
    if args.output and (args.output.exists() or operation):
        parser.error("--output 仅用于查询，且须为新的文件")
    if args.query and (not args.query.strip() or len(args.query) > 1000):
        parser.error("检索词需为1至1000个字符")
    if args.query and re.search(r"apikey_[A-Za-z0-9_-]+", args.query):
        parser.error("检索词包含凭据格式，未保存或发送")
    if args.top > args.candidates:
        parser.error("--top 不能大于 --candidates")
    if args.open and args.open > args.top:
        parser.error("--open 不能大于 --top")
    if args.render and args.query and args.render > args.top:
        parser.error("--render 不能大于 --top")
    if args.replace_sources and not args.register_pdfs:
        parser.error("--replace-sources 仅用于 --register-pdfs")
    if args.verify_packet and not (args.context or args.project_context):
        parser.error("--verify-packet 需要 --context 或 --project-context")
    if args.view_page is not None and args.render is not None:
        parser.error("--view-page 已指定原页，不能与 --render N 组合；可用 --render-dpi")
    if args.render and not (args.query or args.view_record):
        parser.error("--render 用于查询或 --view-record")
    try:
        args._enforce_skill_paths = True
        for name in ("data_root", "pdf_root", "state_root", "output", "context", "project_context", "view_record", "verify_packet", "register_pdfs"):
            value = getattr(args, name)
            if value is not None:
                setattr(args, name, paths.skill_path(value, "knowledge_" + name, root=paths.SKILL_ROOT))
        if args.verify_packet:
            from ..supplier_knowledge import verify_packet_binding
            packet = json.loads(args.verify_packet.read_text())
            report = verify_packet_binding(packet, current_context(args), skill_root=paths.SKILL_ROOT)
            print(json.dumps(report, ensure_ascii=False, indent=2))
            return 0 if report["status"] == "ready_for_agent_review" else 2
        config = runtime_config(args.data_root, args.state_root,
                                include_misumi=args.sources != "local-pdf" and not (args.register_pdfs or args.view_record or args.view_page))
        for name in ("data_root", "index_path", "archive_root", "state_root", "cache_root"):
            paths.skill_path(config[name], "knowledge_" + name, root=paths.SKILL_ROOT)
        pdf_root = args.pdf_root or default_data_root("local-pdf")
        if args.register_pdfs:
            from ..local_pdf_sources import _manifest, register_sources
            for source in _manifest(args.register_pdfs):
                paths.skill_path(source["path"], "knowledge_pdf_original", root=paths.SKILL_ROOT)
            report = register_sources(args.register_pdfs, pdf_root, replace=args.replace_sources)
            print(json.dumps(report, ensure_ascii=False, indent=2))
            return 0
        if args.view_record or args.view_page:
            from ..source_citations import render_page
            if args.view_page:
                result = registered_page_source(args.view_page, args)
            else:
                record = json.loads(args.view_record.read_text())
                results = record.get("results")
                if results is None:
                    results = [{"id": e["id"], **e["source"]} for e in record.get("evidence_candidates", [])]
                selected = (args.render or 1) - 1
                if selected >= len(results):
                    raise ValueError("source_view_result_unavailable")
                result = results[selected]
                original = Path(result["source_pdf"]).expanduser().resolve()
                if not original.is_file() or not original.is_relative_to(paths.SKILL_ROOT.resolve()):
                    result = relocated_record_source(result, args)
            paths.skill_path(result["source_pdf"], "knowledge_pdf_original", root=paths.SKILL_ROOT)
            report = render_page(result, prepare_state(config["cache_root"]) / "pages", args.render_dpi)
            report.update(registered_page_id=result["id"], usage={"network_requests": 0})
            print(json.dumps(report, ensure_ascii=False, indent=2))
            return 0
        if args.reindex:
            if not Path(config["archive_root"]).is_dir():
                report = readiness(config)
                report["status"] = "not_ready"
                print(json.dumps(report, ensure_ascii=False, indent=2))
                return 2
            try:
                meta = build_index(config["archive_root"], config["index_path"], progress=lambda msg: print(msg, file=sys.stderr))
            except ModuleNotFoundError as exc:
                if exc.name != "pymupdf":
                    raise
                print(json.dumps({"status": "not_ready", "provider": "misumi_archive",
                    "errors": [{"code": "reindex_requires_pymupdf"}],
                    "setup": "Install the optional pinned reindex dependencies; normal queries require only Python's standard library."}))
                return 2
            report = {"schema": "misumi.index-build/v1", "status": "completed", "index": meta}
            write_json(prepare_state(config["state_root"]) / "evidence/index-build.json", report)
            print(json.dumps(report, ensure_ascii=False, indent=2))
            return 0
        if args.status:
            with ExitStack() as stack:
                providers, statuses = _providers(config, args, stack)
                if len(statuses) == 1 and statuses[0]["provider"] == "misumi_archive":
                    report = statuses[0]
                else:
                    counts = dict(sum((collections.Counter(p["meta"]["counts"]) for p in providers), collections.Counter()))
                    report = {"schema": "ontology-engineering.knowledge-readiness/v1",
                        "status": "ready" if providers and all(s["status"] == "ready" for s in statuses) else "partial" if providers else "not_ready",
                        "sources": statuses, "index": {"counts": counts}, "data_root": str(pdf_root),
                        "state_root": config["state_root"], "missing": [item for s in statuses for item in s.get("missing", s.get("errors", []))],
                        "setup": {"instruction": "Use scripts/source_library.py to import Drive downloads and register books; install sources/misumi with runtime/misumi/setup.py. See docs/PORTABLE-DISTRIBUTION.md."}}
            report.update(version=VERSION, model=config["model"], parameter_database="not_built", ocr="not_run")
            if args.json:
                print(json.dumps(report, ensure_ascii=False, indent=2))
            elif report["status"] == "not_ready":
                print("资料尚未就绪：" + ", ".join(item.get("kind", item.get("code", "source_not_ready")) for item in report["missing"]))
                print(report["setup"]["instruction"])
            else:
                counts = report["index"]["counts"]
                print(f"资料已就绪：{counts['pages']} 页、{counts['chapters']} 章；未做全量 OCR 或工程参数认证。")
                print("数据：" + config["data_root"])
                print("私有状态：" + config["state_root"])
            return 2 if report["status"] in {"not_ready", "partial"} else 0
        if args.context or args.project_context:
            # One in-memory frozen snapshot binds retrieval and handoff; no
            # subprocess or mutable external config participates in the call.
            args.context = current_context(args)
        if args.output:
            args.full_text, args.knowledge = True, True
        report = search(config, args)
        if args.render and report["status"] != "not_ready":
            from ..source_citations import render_page
            if args.render > len(report["results"]):
                report["errors"].append({"code": "source_view_result_unavailable"})
                report["status"] = "partial"
            else:
                try:
                    report["page_views"] = [render_page(report["results"][args.render - 1],
                        prepare_state(config["cache_root"]) / "pages", args.render_dpi)]
                except (OSError, ValueError) as exc:
                    report["errors"].append({"code": str(exc).split(":", 1)[0]})
                    report["status"] = "partial"
            write_json(Path(report["record_file"]), report)
        if args.open and report["status"] != "not_ready":
            report["opened_result"] = None
            if len(report["results"]) >= args.open and Path("/usr/bin/open").is_file():
                item = report["results"][args.open - 1]
                opened = subprocess.run(["/usr/bin/open", item.get("pdf_uri", item["source_pdf"])], capture_output=True)
                if opened.returncode == 0:
                    report["opened_result"] = args.open
                else:
                    report["errors"].append({"code": "pdf_reader_open_failed"})
            else:
                report["errors"].append({"code": "pdf_reader_or_requested_result_unavailable"})
            if report["errors"] and report["status"] not in ("failed", "partial"):
                report["status"] = "partial"
            write_json(Path(report["record_file"]), report)
        if args.output:
            packet = (report if report["status"] == "not_ready"
                      else make_packet(report, report["context_snapshot"], skill_root=paths.SKILL_ROOT))
            packet["producer"] = {"entrypoint": "scripts/jev_knowledge.py", "provider": "registered_sources",
                                  "implementation": "ontology_engineering.misumi"}
            if report.get("page_views"):
                packet["page_views"] = report["page_views"]
            save_output(args.output, packet)
            report["status"] = packet["status"]
            print(json.dumps({"status": packet["status"], "output": str(args.output.resolve()),
                              "evidence_candidates": len(packet.get("evidence_candidates", [])),
                              "context_sha256": packet.get("context_sha256"), "semantic_review": "not_run",
                              "project_source_freshness": packet.get("project_source_freshness"),
                              **({"readiness": report["readiness"]} if report["status"] == "not_ready" else {})}, ensure_ascii=False))
        elif args.json:
            print(json.dumps(report, ensure_ascii=False, indent=2))
        else:
            print_report(report)
        return 2 if report["status"] in ("failed", "partial", "not_ready", "stale_context") else 0
    except (OSError, ValueError, KeyError, sqlite3.Error) as exc:
        code = "local_io_or_index_error" if not isinstance(exc, ValueError) else str(exc).split(":", 1)[0]
        if args.json or args.output or args.verify_packet or args.view_record or args.view_page or args.register_pdfs:
            print(json.dumps({"schema": RESULT_SCHEMA, "status": "failed", "results": [], "errors": [{"code": code}]}, ensure_ascii=False))
        else:
            print("工程知识查询失败：" + code + "。请检查 --status 和数据目录。", file=sys.stderr)
        return 2
