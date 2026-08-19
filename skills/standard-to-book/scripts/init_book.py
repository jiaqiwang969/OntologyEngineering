#!/usr/bin/env python3
"""Create a privacy-first book corpus bound to Semantica-owned semantics.

The generated tree is deliberately *not* an ontology implementation.  It
contains the external specification (the book/source corpus) and a proposal
for a built-in Semantica package.  CQ definitions, ontologies, shapes,
queries, cases, rules, fixtures and runners belong in Semantica itself.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import sys
from datetime import date
from pathlib import Path


SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Create a new privacy-first OntologyEngineering book package."
    )
    parser.add_argument("--slug", required=True, help="Lowercase hyphenated package name.")
    parser.add_argument("--title", required=True, help="Human-facing book title.")
    parser.add_argument("--standard", required=True, help="Standard family or controlled corpus.")
    parser.add_argument(
        "--audience",
        default="进入陌生专业领域的工程师和中小型制造企业",
        help="Primary readers.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("workbooks"),
        help="Parent directory for the new package.",
    )
    return parser.parse_args()


def yaml_value(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def semantica_package_id(slug: str) -> str:
    """Return the stable proposed built-in package identifier for a book."""

    return "semantica.books." + slug.replace("-", "_")


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content.rstrip() + "\n", encoding="utf-8")


def write_csv(path: Path, header: list[str], rows: list[list[str]] | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(header)
        writer.writerows(rows or [])


def build_package(args: argparse.Namespace) -> Path:
    if not SLUG_RE.fullmatch(args.slug):
        raise ValueError("--slug must use lowercase letters, digits and single hyphens")
    for field in ("title", "standard", "audience"):
        value = getattr(args, field, None)
        if not isinstance(value, str) or not value.strip() or "\n" in value or "\r" in value:
            raise ValueError(f"--{field} must be a non-empty single-line value")

    target = args.output.expanduser().resolve() / args.slug
    if target.exists():
        raise FileExistsError(f"refusing to overwrite existing path: {target}")

    target.mkdir(parents=True)
    created = date.today().isoformat()

    write_text(
        target / ".gitignore",
        """
# Emergency denylist only. Never create these paths here; keep evidence outside the package.
private/
sources/raw/
evidence/
sessions/
rollouts/
.env
.env.*
!.env.example
*.pem
*.key
*.p12
*.pfx
__pycache__/
*.pyc
.DS_Store
""",
    )

    write_text(
        target / "book.yaml",
        f"""
schema_version: "1.0"
slug: {yaml_value(args.slug)}
title: {yaml_value(args.title)}
standard_family: {yaml_value(args.standard)}
audience: {yaml_value(args.audience)}
mission: "把专家标准转化为普通制造业工程师可读、可查、可验证的知识产品"
created: {yaml_value(created)}
status: "charter"
source_policy: "private-controlled"
private_evidence_location: "external-required"
private_evidence_linkage: "logical-id-and-sha256-only"
public_release_policy: "allowlist"
rights_status: "pending"
technical_review_status: "pending"
reader_review_status: "pending"
""",
    )

    write_text(
        target / "book-charter.md",
        f"""
# {args.title} — Book Charter

## 目标读者

{args.audience}

## 目标标准或知识域

{args.standard}

## 要解决的制造业问题

- TODO

## 读完后允许作出的决定

- TODO

## 必须升级给专家或责任人的决定

- TODO

## 适用范围与排除范围

- 适用：TODO
- 排除：官方翻译、认证意见、真实产品合规结论及未经授权的标准再分发

## 审阅责任

- 领域审阅者：TODO
- 普通工程师冷读者：TODO
- 权利与隐私审阅者：TODO
- 发布责任人：TODO

## 公共与私有边界

- 私有：标准原文、受限抽取、企业资料、会话和真实项目数据；必须位于书包外部的受控根
- 公共候选：原创讲解、来源/命题映射、教学图、书本路由 Skill 和 Semantica 发布证据
- 唯一可执行语义：Semantica 内置包；CQ、本体、形状、查询、案例、规则、fixture 和 runner 不进入书包

## 初始能力问题

在本节起草 10–30 个读者问题；评审后把 CQ 定义和验收 oracle 提交到
`semantica/package-proposal.yaml` 所标识的 Semantica 内置包，再冻结章节结构。
""",
    )

    write_csv(
        target / "sources" / "source-register.csv",
        [
            "source_id",
            "title",
            "edition",
            "content_owner",
            "rights_basis",
            "rights_status",
            "private_logical_id",
            "sha256",
            "public_distribution",
            "technical_review",
            "notes",
        ],
    )
    write_csv(
        target / "chapters" / "chapter-register.csv",
        [
            "chapter_id",
            "title",
            "reader_problem",
            "semantica_cq_ids",
            "source_ids",
            "figure_ids",
            "review_status",
        ],
    )
    write_csv(
        target / "propositions" / "proposition-register.csv",
        [
            "proposition_id",
            "chapter_id",
            "semantica_cq_ids",
            "source_ids",
            "statement_summary",
            "claim_class",
            "authority_limit",
            "evidence_oracle",
            "review_status",
        ],
    )
    write_csv(
        target / "figures" / "figure-register.csv",
        [
            "figure_id",
            "chapter_id",
            "source_ids",
            "visual_question",
            "semantic_baseline",
            "input_rights",
            "input_rights_status",
            "generator",
            "sha256",
            "caption",
            "alt_text",
            "release_status",
        ],
    )
    write_csv(
        target / "release" / "public-assets.csv",
        [
            "asset_id",
            "relative_path",
            "asset_role",
            "chapter_ids",
            "figure_id",
            "sha256",
            "creator_or_method",
            "rights_basis",
            "rights_status",
            "contains_personal_data",
            "technical_review",
            "privacy_review",
            "release_status",
        ],
    )
    write_csv(
        target / "release" / "package-lock.csv",
        ["relative_path", "sha256"],
    )

    package_id = semantica_package_id(args.slug)
    write_text(
        target / "semantica" / "package-proposal.yaml",
        f"""
schema_version: "1.0"
book_slug: {yaml_value(args.slug)}
proposed_package_id: {yaml_value(package_id)}
external_specification_kind: "book"
external_source_register: "sources/source-register.csv"
chapter_register: "chapters/chapter-register.csv"
proposition_register: "propositions/proposition-register.csv"
requested_semantics:
  - "ontology"
  - "competency-questions"
  - "shapes"
  - "queries"
  - "cases"
  - "engineering-rules"
execution_owner: "Semantica"
proposal_status: "draft"
""",
    )
    write_text(
        target / "semantica" / "package-binding.yaml",
        f"""
schema_version: "1.0"
book_slug: {yaml_value(args.slug)}
semantica_package_id: {yaml_value(package_id)}
semantica_package_version: "unbound"
binding_status: "proposed"
execution_authority: "semantica-only"
runtime_gateway: "ontology_engineering.semantica_runtime"
source_lock: "release/semantica-source-lock.json"
runtime_receipt: "release/semantica-runtime-receipt.json"
release_verdict: "release/semantica-release-verdict.json"
bound_cq_ids:
""",
    )
    write_text(
        target / "privacy" / "public-export.yaml",
        """
schema_version: "1.0"
policy: "default-deny"
controlled_evidence_root: "external-required"
evidence_linkage: "logical-id-and-sha256-only"
forbidden_package_paths:
  - "sources/raw"
  - "private"
  - "evidence"
  - "sessions"
  - "rollouts"
forbidden_public_content:
  - "standard originals or restricted extracts"
  - "enterprise, customer, supplier, worker or product identifiers"
  - "personal absolute paths"
  - "credentials, tokens, keys or cookies"
  - "private model sessions or attachment caches"
  - "assets with pending input rights"
  - "book-local ontology, CQ, shape, query, case, rule, fixture or runner payloads"
public_manifest: "release/public-assets.csv"
human_privacy_review: "required"
""",
    )
    write_text(
        target / "skill" / "SKILL.md",
        f"""---
name: {yaml_value(args.slug)}
description: {yaml_value(f"Use when a reader needs the reviewed knowledge and decision boundaries in {args.title}.")}
---

# {args.title} Skill

## Authority boundary

- TODO: describe what this Skill may answer from the released book package.
- TODO: describe when it must stop and escalate to a qualified reviewer or accountable owner.

## Workflow

1. Bind the exact released book version and its `semantica/package-binding.yaml`.
2. Route executable questions only through `ontology_engineering.semantica_runtime`.
3. Answer only Semantica-receipted competency questions with registered proposition and source IDs.
4. Report assumptions, evidence gaps and mandatory escalation points.
""",
    )
    write_text(
        target / "README.md",
        f"""
# {args.title}

这是一个由 OntologyEngineering `standard-to-book` 流程创建的候选书包。

- 目标标准/知识域：{args.standard}
- 目标读者：{args.audience}
- 当前状态：Book Charter，尚未形成标准解释、合规结论或发布物

先完成 `book-charter.md`、来源账、命题账和书本 Skill，并将能力问题、形状、查询、案例、
规则和本体提交到 `semantica/package-proposal.yaml` 指定的 Semantica 内置包。书包本身不得
生成或保留平行的语义资产与 runner。

公开候选还必须绑定已安装的 Semantica 包，并补齐 source lock、runtime receipt、
`complete` release verdict、教学图和公开资产白名单，最后写入 package lock。
原始标准、企业资料和模型会话必须位于书包外部的受控证据根，不得放入本公共候选包。
运行 `validate_book.py` 的 structure、charter 和 release 阶段检查当前成熟度。
""",
    )
    return target


def main() -> int:
    args = parse_args()
    try:
        target = build_package(args)
    except (ValueError, FileExistsError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    print(f"created privacy-first book package: {target}")
    print("next: complete the charter/source/proposition maps and submit the Semantica package proposal")
    print("no standard text was read, copied or generated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
