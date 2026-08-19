#!/usr/bin/env python3
"""仅用当前 OE clone 中锁定的源码和出版资产隔离构建第二卷。

默认路径不读取任何仓外母版，也不要求环境变量：Markdown、TeX 工厂、图像和
渲染清单都先在当前仓库中接受哈希校验，再复制到 mktemp 并编译。历史母版只
能通过显式 ``--audit-provenance PATH`` 做只读来源对照，不参与默认构建门禁。
"""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath
from typing import Sequence

SCRIPT_DIR = Path(__file__).resolve().parent
LOCAL_BOOK = SCRIPT_DIR.parent
ASSET_LOCK_FILE = SCRIPT_DIR / "authoring-assets.sha256"
SOURCE_LOCK_FILE = SCRIPT_DIR / "current-source.sha256"
PROVENANCE_LOCK_FILE = SCRIPT_DIR / "authoring-provenance.sha256"

FACTORY_FILES = (
    "build_handbook.py",
    "main.tex",
    "preamble.tex",
    "book-metadata.tex",
)
LOCKED_FACTORY_FILES = (
    "handbook/book-metadata.tex",
    "handbook/build_handbook.py",
    "handbook/build_isolated.py",
    "handbook/main.tex",
    "handbook/preamble.tex",
    "handbook/test_build_handbook.py",
)
CHAPTER_DIRS = (
    "ch01-introduction",
    "ch02-concepts-terminology",
    "ch03-safety-management",
    "ch04-concept-hara",
    "ch05-system-development",
    "ch06-hardware-development",
    "ch07-software-development",
    "ch08-asil-decomposition-dfa",
    "ch09-production-operation",
    "ch10-supporting-processes",
    "ch11-claim-ontology",
    "ch12-identity-ontology",
    "ch13-governance-ontology",
    "ch14-context-hazard-ontology",
    "ch15-requirements-ontology",
    "ch16-measurement-ontology",
    "ch17-change-ontology",
    "ch18-dependency-ontology",
    "ch19-field-ontology",
    "ch20-assurance-ontology",
)
LOCKED_CONTENT_FILES = (
    "front-matter/preface.md",
    *(f"{dirname}/chapter.md" for dirname in CHAPTER_DIRS),
    "appendices/appendix-a-semiconductor.md",
    "appendices/appendix-b-motorcycle-truck.md",
    "appendices/appendix-c-glossary.md",
    "appendices/appendix-d-method-tables.md",
)
ASSET_DIRS = (
    "handbook/figures-imagegen",
    "handbook/figures-rendered",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_lock(path: Path) -> dict[PurePosixPath, str]:
    entries: dict[PurePosixPath, str] = {}
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split(maxsplit=1)
        if len(fields) != 2 or len(fields[0]) != 64:
            raise ValueError(f"{path}:{line_number}: 非法 SHA-256 清单行")
        digest, raw_relative = fields
        relative = PurePosixPath(raw_relative.strip())
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"{path}:{line_number}: 路径必须位于锁定根目录内")
        if any(char not in "0123456789abcdef" for char in digest):
            raise ValueError(f"{path}:{line_number}: 非法 SHA-256")
        if relative in entries:
            raise ValueError(f"{path}:{line_number}: 重复路径 {relative}")
        entries[relative] = digest
    if not entries:
        raise ValueError(f"空哈希锁：{path}")
    return entries


def verify_locked_tree(
    root: Path,
    entries: dict[PurePosixPath, str],
    label: str,
) -> None:
    failures: list[str] = []
    for relative, expected in entries.items():
        source = root.joinpath(*relative.parts)
        if not source.is_file():
            failures.append(f"缺失 {relative}")
            continue
        actual = sha256_file(source)
        if actual != expected:
            failures.append(f"哈希漂移 {relative}: {actual} != {expected}")
    if failures:
        details = "\n  - ".join(failures)
        raise RuntimeError(f"{label}哈希锁校验失败：\n  - {details}")


def require_exact_members(
    entries: dict[PurePosixPath, str],
    expected: set[PurePosixPath],
    label: str,
) -> None:
    actual = set(entries)
    missing = sorted(expected - actual, key=str)
    unexpected = sorted(actual - expected, key=str)
    if not missing and not unexpected:
        return
    details: list[str] = []
    details.extend(f"清单缺项 {path}" for path in missing)
    details.extend(f"清单越界 {path}" for path in unexpected)
    raise ValueError(f"{label}范围错误：\n  - " + "\n  - ".join(details))


def current_asset_files() -> set[PurePosixPath]:
    files: set[PurePosixPath] = set()
    for dirname in ASSET_DIRS:
        root = LOCAL_BOOK / dirname
        if not root.is_dir():
            raise FileNotFoundError(f"仓内出版资产目录缺失：{root}")
        files.update(
            PurePosixPath(path.relative_to(LOCAL_BOOK).as_posix())
            for path in root.rglob("*")
            if path.is_file()
        )
    return files


def verify_current_repository() -> tuple[
    dict[PurePosixPath, str], dict[PurePosixPath, str]
]:
    source_entries = load_lock(SOURCE_LOCK_FILE)
    expected_sources = {
        PurePosixPath(path)
        for path in (*LOCKED_FACTORY_FILES, *LOCKED_CONTENT_FILES)
    }
    require_exact_members(source_entries, expected_sources, "当前源码锁")
    verify_locked_tree(LOCAL_BOOK, source_entries, "当前仓内源码")

    asset_entries = load_lock(ASSET_LOCK_FILE)
    require_exact_members(asset_entries, current_asset_files(), "当前出版资产锁")
    verify_locked_tree(LOCAL_BOOK, asset_entries, "当前仓内出版资产")
    return source_entries, asset_entries


def stage_local_sources(stage_book: Path) -> None:
    stage_book.mkdir(parents=True)
    for dirname in ("front-matter", "appendices", *CHAPTER_DIRS):
        source = LOCAL_BOOK / dirname
        if not source.is_dir():
            raise FileNotFoundError(f"OE 内容正本缺失：{source}")
        shutil.copytree(source, stage_book / dirname)

    stage_handbook = stage_book / "handbook"
    stage_handbook.mkdir()
    for name in FACTORY_FILES:
        source = SCRIPT_DIR / name
        if not source.is_file():
            raise FileNotFoundError(f"OE TeX 工厂缺失：{source}")
        shutil.copy2(source, stage_handbook / name)


def stage_locked_assets(
    source_root: Path,
    entries: dict[PurePosixPath, str],
    stage_book: Path,
) -> int:
    for relative in entries:
        source = source_root.joinpath(*relative.parts)
        destination = stage_book.joinpath(*relative.parts)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
    return len(entries)


def require_success(completed: subprocess.CompletedProcess[str], label: str) -> None:
    """Keep a useful failure tail while subprocess targets remain explicit."""
    if completed.returncode != 0:
        tail = "\n".join(completed.stdout.splitlines()[-120:])
        raise RuntimeError(f"{label} 失败（exit={completed.returncode}）：\n{tail}")


def run_build(stage_book: Path) -> Path:
    handbook = stage_book / "handbook"
    if shutil.which("latexmk") is None:
        raise RuntimeError("找不到 latexmk；请先安装含 XeLaTeX 的 TeX 发行版")

    build_env = os.environ.copy()
    build_env.setdefault("SOURCE_DATE_EPOCH", "1787097600")
    build_env["TZ"] = "UTC"
    generated = subprocess.run(
        [sys.executable, "build_handbook.py"],
        cwd=handbook,
        env=build_env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    require_success(generated, "Markdown→TeX")
    compiled = subprocess.run(
        [
            "latexmk",
            "-xelatex",
            "-interaction=nonstopmode",
            "-halt-on-error",
            "-file-line-error",
            "main.tex",
        ],
        cwd=handbook,
        env=build_env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    require_success(compiled, "XeLaTeX")
    pdf = handbook / "main.pdf"
    if not pdf.is_file():
        raise RuntimeError("latexmk 成功返回，但 main.pdf 不存在")
    return pdf


def build_once(
    entries: dict[PurePosixPath, str],
    work_root: Path,
    output: Path | None,
) -> None:
    stage_book = work_root / "product-trustworthiness-book"
    stage_local_sources(stage_book)
    asset_count = stage_locked_assets(LOCAL_BOOK, entries, stage_book)
    pdf = run_build(stage_book)
    digest = sha256_file(pdf)
    print(f"隔离构建通过：{pdf}（{asset_count} 个仓内锁定图资产，sha256={digest}）")
    if output is not None:
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(pdf, output)
        print(f"已按显式参数写出：{output}")


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help="只校验当前 clone 的源码与出版资产哈希，不运行 XeLaTeX",
    )
    parser.add_argument(
        "--audit-provenance",
        metavar="PATH",
        type=Path,
        help="显式、可选地只读核验历史母版；不参与默认构建",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="显式指定持久 PDF 输出；省略时不写回源码树",
    )
    parser.add_argument(
        "--keep-workdir",
        action="store_true",
        help="保留临时构建目录以便诊断",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> None:
    args = parse_args(argv)
    source_entries, asset_entries = verify_current_repository()
    print(
        f"当前仓内锁通过：{len(source_entries)} 项源码、"
        f"{len(asset_entries)} 项出版资产"
    )

    if args.audit_provenance is not None:
        provenance_root = args.audit_provenance.expanduser().resolve()
        if not provenance_root.is_dir():
            raise SystemExit(f"历史母版根目录不存在：{provenance_root}")
        provenance_entries = load_lock(PROVENANCE_LOCK_FILE)
        verify_locked_tree(provenance_root, provenance_entries, "历史母版 provenance")
        print(
            f"历史母版 provenance 对照通过：{len(provenance_entries)} 项；"
            f"未写入 {provenance_root}"
        )

    if args.verify_only:
        return

    output = args.output.expanduser().resolve() if args.output else None
    if args.keep_workdir:
        work_root = Path(tempfile.mkdtemp(prefix="ptw-vol2-build-"))
        print(f"保留隔离目录：{work_root}")
        build_once(asset_entries, work_root, output)
    else:
        with tempfile.TemporaryDirectory(prefix="ptw-vol2-build-") as tmp:
            build_once(asset_entries, Path(tmp), output)


if __name__ == "__main__":
    main()
