#!/usr/bin/env python3
"""Create a privacy-first standard-to-book package without ingesting source text."""

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
- 公共候选：原创讲解、合成案例、本体、查询、约束、脚本和经清权利图

## 初始能力问题

在 `cqs/cq-register.csv` 登记 10–30 个可验收问题后再冻结章节结构。
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
        target / "cqs" / "cq-register.csv",
        [
            "cq_id",
            "question",
            "reader_decision",
            "evidence_required",
            "expected_answer_form",
            "acceptance_oracle",
            "status",
        ],
    )
    write_csv(
        target / "chapters" / "chapter-register.csv",
        [
            "chapter_id",
            "title",
            "reader_problem",
            "cq_ids",
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
            "cq_ids",
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

    write_text(
        target / "ontology" / "package-manifest.yaml",
        f"""
schema_version: "1.0"
book_slug: {yaml_value(args.slug)}
namespace: "TODO"
competency_question_register: "cqs/cq-register.csv"
tbox: "TODO"
controlled_abox_or_adapter: "TODO"
queries: "TODO"
constraints: "TODO"
positive_fixtures: "TODO"
single_fault_negative_fixtures: "TODO"
runner: "TODO"
status: "not-started"
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

1. Bind the exact released book version and declared scope.
2. Answer only covered competency questions with registered proposition and source IDs.
3. Report assumptions, evidence gaps and mandatory escalation points.
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

先完成 `book-charter.md`、来源账、CQ、命题账和书本 Skill；公开候选还必须补齐教学图、
本体制品、机器测试报告与公开资产白名单，最后写入 package lock。
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
    print("next: complete the charter, source/CQ/proposition registers and book Skill")
    print("no standard text was read, copied or generated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
