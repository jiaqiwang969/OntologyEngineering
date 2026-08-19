from __future__ import annotations

import contextlib
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from pathlib import PurePosixPath
from unittest import mock

import handbook.build_handbook as builder
import handbook.build_isolated as isolated

from handbook.build_handbook import (
    BOOK,
    CHAPTER_DIRS,
    FIG_SLOT_RE,
    MARKDOWN_IMAGE_RE,
    OUT,
    chapter_source,
    expected_outputs,
    md_to_tex,
    resolve_markdown_image,
    verbatim_line,
    write_outputs,
)


class IsolatedBuildTests(unittest.TestCase):
    def test_current_clone_locks_cover_all_build_inputs(self) -> None:
        source_entries, asset_entries = isolated.verify_current_repository()
        self.assertEqual(len(source_entries), 31)
        self.assertEqual(len(asset_entries), 49)
        self.assertEqual(
            set(source_entries),
            {
                PurePosixPath(path)
                for path in (
                    *isolated.LOCKED_FACTORY_FILES,
                    *isolated.LOCKED_CONTENT_FILES,
                )
            },
        )
        self.assertEqual(set(asset_entries), isolated.current_asset_files())

    def test_default_isolated_path_needs_no_environment_variable(self) -> None:
        source_entries = {PurePosixPath("source.md"): "0" * 64}
        asset_entries = {PurePosixPath("handbook/figure.png"): "1" * 64}
        stdout = io.StringIO()
        with (
            mock.patch.dict(os.environ, {}, clear=True),
            mock.patch.object(
                isolated,
                "verify_current_repository",
                return_value=(source_entries, asset_entries),
            ),
            mock.patch.object(isolated, "build_once") as build_once,
            contextlib.redirect_stdout(stdout),
        ):
            isolated.main([])
        build_once.assert_called_once()
        self.assertEqual(build_once.call_args.args[0], asset_entries)
        self.assertIsNone(build_once.call_args.args[2])
        self.assertIn("当前仓内锁通过", stdout.getvalue())

    def test_locked_tree_rejects_content_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "chapter.md"
            source.write_text("locked\n", encoding="utf-8")
            entries = {
                PurePosixPath("chapter.md"): isolated.sha256_file(source),
            }
            isolated.verify_locked_tree(root, entries, "测试输入")
            source.write_text("drifted\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "哈希漂移 chapter.md"):
                isolated.verify_locked_tree(root, entries, "测试输入")


class ChapterSourceTests(unittest.TestCase):
    def test_missing_chapter_fails_closed_even_when_readme_exists(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            chdir = Path(tmp)
            readme = chdir / "README.md"
            readme.write_text("# navigation\n", encoding="utf-8")
            with self.assertRaisesRegex(FileNotFoundError, "唯一内容正本"):
                chapter_source(chdir)

    def test_chapter_is_the_only_authoritative_source(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            chdir = Path(tmp)
            (chdir / "README.md").write_text("# navigation\n", encoding="utf-8")
            chapter = chdir / "chapter.md"
            chapter.write_text("# prose\n", encoding="utf-8")
            self.assertEqual(chapter_source(chdir), chapter)

    def test_generated_fragments_are_reproducible(self) -> None:
        expected = expected_outputs()
        with tempfile.TemporaryDirectory() as tmp:
            generated = Path(tmp)
            write_outputs(generated)
            actual_names = {path.name for path in generated.iterdir()}
            self.assertEqual(actual_names, set(expected))
            for name, content in expected.items():
                self.assertEqual(
                    (generated / name).read_text(encoding="utf-8"),
                    content,
                )

    def test_checked_in_fragments_match_when_present(self) -> None:
        if not OUT.exists():
            self.skipTest("尚未生成可审计 TeX 快照")
        expected = expected_outputs()
        actual_names = {path.name for path in OUT.iterdir() if path.is_file()}
        self.assertEqual(actual_names, set(expected))
        for name, content in expected.items():
            self.assertEqual(
                (OUT / name).read_text(encoding="utf-8"),
                content,
                f"stale generated fragment: {name}; run handbook/build_handbook.py",
            )


SLOT_MD = (
    "前文。\n\n"
    "<!-- FIG: ch01-fig1 two-failure-modes-fork -->\n"
    "> 图 1-1（占位）：两类失效的对策分叉——系统性失效靠过程与设计措施。\n\n"
    "后文。\n"
)


class FigureSlotTests(unittest.TestCase):
    def test_slot_with_rendered_pdf_becomes_figure(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            figdir = Path(tmp)
            (figdir / "ch01-fig1-two-failure-modes-fork.pdf").write_bytes(b"%PDF-1.7\n")
            tex = md_to_tex(SLOT_MD, chapter_from_h1=False, figures_dir=figdir)
        self.assertIn(r"\begin{figure}[htbp]", tex)
        self.assertIn(
            r"\adjustbox{max size={\linewidth}{0.78\textheight}}"
            r"{\includegraphics{figures-rendered/ch01-fig1-two-failure-modes-fork.pdf}}",
            tex,
        )
        self.assertIn(r"\setcounter{figure}{0}", tex)
        self.assertIn(r"\caption{两类失效的对策分叉——系统性失效靠过程与设计措施。}", tex)
        self.assertNotIn("占位", tex)
        self.assertNotIn("FIG:", tex)
        self.assertNotIn(r"\begin{noteblock}", tex)

    def test_slot_without_rendered_pdf_keeps_placeholder_and_warns(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                tex = md_to_tex(SLOT_MD, chapter_from_h1=False, figures_dir=Path(tmp))
        self.assertNotIn(r"\begin{figure}", tex)
        self.assertNotIn(r"\includegraphics", tex)
        self.assertIn(r"\begin{noteblock}", tex)
        self.assertIn("占位", tex)
        self.assertNotIn("FIG:", tex)
        self.assertIn("ch01-fig1-two-failure-modes-fork", stderr.getvalue())
        self.assertIn("渲染文件缺失", stderr.getvalue())

    def test_extreme_wide_figure_is_rotated_sideways(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            figdir = Path(tmp)
            stem = "ch01-fig1-two-failure-modes-fork"
            (figdir / f"{stem}.pdf").write_bytes(b"%PDF-1.7\n")
            manifest = {
                stem: {
                    "sha256": "x",
                    "width_pt": 1788.75,
                    "height_pt": 597.0,
                    "min_font_pt": 13.0,
                }
            }
            (figdir / ".render-manifest.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            tex = md_to_tex(SLOT_MD, chapter_from_h1=False, figures_dir=figdir)
        self.assertIn(r"\begin{sidewaysfigure}", tex)
        self.assertIn(r"max size={0.92\textheight}{0.85\textwidth}", tex)
        self.assertIn(r"\end{sidewaysfigure}", tex)
        self.assertNotIn(r"\begin{figure}[htbp]", tex)

    def test_text_legibility_rotates_wide_figure_before_extreme_width(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            figdir = Path(tmp)
            stem = "ch01-fig1-two-failure-modes-fork"
            (figdir / f"{stem}.pdf").write_bytes(b"%PDF-1.7\n")
            manifest = {
                stem: {
                    "sha256": "x",
                    "width_pt": 894.0,
                    "height_pt": 461.25,
                    "min_font_pt": 11.0,
                }
            }
            (figdir / ".render-manifest.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            tex = md_to_tex(SLOT_MD, chapter_from_h1=False, figures_dir=figdir)
        self.assertIn(r"\begin{sidewaysfigure}", tex)

    def test_all_book_figure_slots_are_embedded(self) -> None:
        sources = [BOOK / "front-matter" / "preface.md"]
        sources += [chapter_source(BOOK / dirname) for dirname in CHAPTER_DIRS]
        sources += sorted((BOOK / "appendices").glob("appendix-*.md"))
        n_assets = 0
        n_numbered = 0
        missing_assets: list[str] = []
        for src in sources:
            lines = src.read_text(encoding="utf-8").splitlines()
            legacy_slots = [
                match for line in lines
                if (match := FIG_SLOT_RE.fullmatch(line.strip())) is not None
            ]
            n_legacy_slots = len(legacy_slots)
            markdown_images = [
                match for line in lines
                if (match := MARKDOWN_IMAGE_RE.fullmatch(line.strip())) is not None
            ]
            for match in legacy_slots:
                stem = f"{match.group(1)}-{match.group(2)}.pdf"
                if not (builder.FIGDIR / stem).exists():
                    missing_assets.append(f"figures-rendered/{stem}")
            for match in markdown_images:
                if resolve_markdown_image(match.group("target"), src) is None:
                    missing_assets.append(match.group("target"))
            n_assets += n_legacy_slots + len(markdown_images)
            n_numbered += n_legacy_slots + sum(
                match.group("title") != "chapter-art" for match in markdown_images
            )
        if missing_assets:
            self.skipTest(
                "二进制出版资产未装入轻量源码树；用 build_isolated.py 校验母版资产："
                + ", ".join(sorted(set(missing_assets))[:3])
            )
        outputs = expected_outputs()
        n_images = sum(
            content.count(r"\includegraphics{")
            for name, content in outputs.items() if name.endswith(".tex")
        )
        n_figures = sum(
            content.count(r"\caption{") + content.count(r"\captionof{figure}{")
            for name, content in outputs.items() if name.endswith(".tex")
        )
        self.assertEqual(n_images, n_assets)
        self.assertEqual(n_figures, n_numbered)
        for name, content in outputs.items():
            self.assertNotIn("（占位）：", content, f"未替换的图槽占位残留于 {name}")

    def test_project_markdown_png_becomes_figure(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "chapter.md"
            image = Path(tmp) / "figure.png"
            image.write_bytes(b"not decoded by the Markdown conversion unit test")
            md = '![替代文字](figure.png "产品主张仍未闭合")\n'
            tex = md_to_tex(
                md,
                chapter_from_h1=False,
                source_path=source,
            )
        self.assertIn(r"\begin{minipage}{\linewidth}", tex)
        self.assertIn(r"\includegraphics{", tex)
        self.assertIn("figure.png", tex)
        self.assertIn(r"\captionof{figure}{产品主张仍未闭合}", tex)

    def test_self_contained_book_image_path_resolves_to_local_handbook(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            book = Path(tmp) / "product-trustworthiness-book"
            handbook = book / "handbook"
            image = handbook / "figures-imagegen" / "chapter.png"
            image.parent.mkdir(parents=True)
            image.write_bytes(b"image")
            with mock.patch.object(builder, "HERE", handbook):
                resolved = resolve_markdown_image(
                    "../handbook/figures-imagegen/chapter.png",
                    book / "ch01" / "chapter.md",
                )
        self.assertIsNotNone(resolved)
        assert resolved is not None
        self.assertEqual(resolved[1], "figures-imagegen/chapter.png")

    def test_legacy_sibling_handbook_path_no_longer_rebases(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            book = Path(tmp) / "product-trustworthiness-book"
            handbook = book / "handbook"
            image = handbook / "figures-imagegen" / "chapter.png"
            image.parent.mkdir(parents=True)
            image.write_bytes(b"image")
            with mock.patch.object(builder, "HERE", handbook):
                resolved = resolve_markdown_image(
                    "../../handbook/figures-imagegen/chapter.png",
                    book / "ch01" / "chapter.md",
                )
        self.assertIsNone(resolved)

    def test_observe_consume_comments_do_not_leak_to_tex(self) -> None:
        tex = md_to_tex(
            "<!-- FIG:ch01-fig01-example:OBSERVE -->\n正文。\n"
            "<!-- FIG:ch01-fig01-example:CONSUME -->\n",
            chapter_from_h1=False,
        )
        self.assertNotIn("FIG:", tex)
        self.assertIn("正文。", tex)

    def test_blank_blockquote_lines_do_not_render_as_greater_than_signs(self) -> None:
        tex = md_to_tex(
            "> **图题**\n>\n> 图注。\n",
            chapter_from_h1=False,
        )
        self.assertIn(r"\begin{noteblock}", tex)
        self.assertIn("图注。", tex)
        self.assertNotIn("\n>\n", tex)


class MarkdownConversionTests(unittest.TestCase):
    def test_rich_chapter_constructs_are_preserved(self) -> None:
        tex = md_to_tex(
            "# 第6章 硬件层开发\n\n---\n\n1. 第一项 σ\n2. 第二项 λ\n",
            chapter_from_h1=True,
        )
        self.assertIn(r"\chapter{硬件层开发}", tex)
        self.assertIn(r"\begin{enumerate}", tex)
        self.assertIn(r"\(\sigma\)", tex)
        self.assertIn(r"\(\lambda\)", tex)
        self.assertNotIn("\n---\n", tex)

    def test_long_chapter_title_uses_display_only_semantic_break(self) -> None:
        title = "活的安全案例：发布与保证本体"
        tex = md_to_tex(
            f"# 第 20 章 {title}\n",
            chapter_from_h1=True,
        )
        self.assertIn(
            rf"\chapter[{title}]{{活的安全案例：\\发布与保证本体}}",
            tex,
        )

    def test_prose_quotes_and_identifiers_are_typeset_safely(self) -> None:
        tex = md_to_tex(
            '正文"危害"，标识符 `SafetyGoal`，代码 `"literal"`。\n\n'
            '代码如下：\n\n```text\nSafetyGoal\n```\n',
            chapter_from_h1=False,
        )
        self.assertIn("正文“危害”", tex)
        self.assertIn(r"\nolinkurl{SafetyGoal}", tex)
        self.assertIn(r'\nolinkurl{"literal"}', tex)
        self.assertIn("\\Needspace{8\\baselineskip}\n代码如下：", tex)
        self.assertIn("\\nopagebreak[4]\n\\begin{codebox}", tex)
        self.assertIn(r"\begin{Verbatim}[fontsize=\small,breaklines=true,", tex)
        self.assertIn(r"breakanywhere=true,breaksymbolleft={},breaksymbolright={}]", tex)

    def test_verbatim_lines_downgrade_unavailable_marker(self) -> None:
        self.assertEqual(verbatim_line("# ▷ 节选"), "# > 节选")

    def test_long_inline_code_and_paths_are_breakable(self) -> None:
        tex = md_to_tex(
            "路径 `../../ontology/confirmation-independence.ttl` 与 "
            "`SafetyCaseConfirmationGovernanceShape`。\n",
            chapter_from_h1=False,
        )
        self.assertIn(r"\nolinkurl{../../ontology/confirmation-independence.ttl}", tex)
        self.assertIn(r"\nolinkurl{SafetyCaseConfirmationGovernanceShape}", tex)
        self.assertNotIn(r"\mbox{\texttt{SafetyCaseConfirmationGovernanceShape}}", tex)

    def test_inline_code_with_spaces_preserves_word_boundaries(self) -> None:
        tex = md_to_tex(
            "Table 1 字段 `Independence with regard to`，命令 `pdftotext -layout`。\n",
            chapter_from_h1=False,
        )
        self.assertNotIn(r"\nolinkurl{Independence with regard to}", tex)
        self.assertNotIn(r"\nolinkurl{pdftotext -layout}", tex)
        self.assertIn("Independence ", tex)
        self.assertIn("with ", tex)
        self.assertIn("pdftotext ", tex)

    def test_manual_subsection_number_is_not_duplicated(self) -> None:
        tex = md_to_tex("### 2. 受控词表\n", chapter_from_h1=False)
        self.assertIn(r"\subsection{受控词表}", tex)
        self.assertNotIn(r"\subsection{2.", tex)

    def test_compact_rating_matrix_gives_first_column_more_width(self) -> None:
        tex = md_to_tex(
            "| 确认措施 | QM | ASIL A | ASIL B | ASIL C | ASIL D |\n"
            "|---|---|---|---|---|---|\n"
            "| Safety Case 确认评审 | - | I1 | I1 | I2 | I3 |\n",
            chapter_from_h1=False,
        )
        self.assertIn(r"\hsize=2.5\hsize", tex)
        self.assertIn(r"\centering\arraybackslash", tex)

    def test_short_chinese_tail_is_not_misclassified_as_rating_matrix(self) -> None:
        tex = md_to_tex(
            "| 产物 | 目标 | 状态 | 证据 | 备注 |\n"
            "|---|---|---|---|---|\n"
            "| 安全计划 | 完整 | 草稿 | 候选 | 待评审 |\n",
            chapter_from_h1=False,
        )
        self.assertNotIn(r"\hsize=2.5\hsize", tex)
        self.assertIn(r"\begin{tabularx}{\linewidth}{@{}XXXXX@{}}", tex)

    def test_failure_rate_identifiers_use_math_subscripts(self) -> None:
        tex = md_to_tex(
            "分类 λ_RF、λ_SPF、λ_MPF,DP 与 Σλ。\n",
            chapter_from_h1=False,
        )
        self.assertIn(r"\(\lambda_{\mathrm{RF}}\)", tex)
        self.assertIn(r"\(\lambda_{\mathrm{SPF}}\)", tex)
        self.assertIn(r"\(\lambda_{\mathrm{MPF,DP}}\)", tex)
        self.assertIn(r"\(\sum \lambda\)", tex)

    def test_inline_excerpt_marker_uses_available_math_glyph(self) -> None:
        tex = md_to_tex("▷ 完整文件\n", chapter_from_h1=False)
        self.assertIn(r"\(\triangleright\)", tex)
        self.assertNotIn("▷", tex)

    def test_appendix_heading_prefixes_are_stripped(self) -> None:
        tex = md_to_tex(
            "# 附录 A 半导体应用指南\n\n## A.3 基础失效率\n\n### A.3.1 来源\n",
            chapter_from_h1=True,
        )
        self.assertIn(r"\chapter{半导体应用指南}", tex)
        self.assertIn(r"\section{基础失效率}", tex)
        self.assertIn(r"\subsection{来源}", tex)

    def test_starred_sections_for_front_matter(self) -> None:
        tex = md_to_tex(
            "# 前言\n\n## 这本书为什么存在\n",
            chapter_from_h1=True,
            starred_sections=True,
        )
        self.assertIn(r"\chapter{前言}", tex)
        self.assertIn(r"\section*{这本书为什么存在}", tex)

    def test_html_comments_are_stripped(self) -> None:
        tex = md_to_tex("<!-- 草案注记 -->\n正文。\n", chapter_from_h1=False)
        self.assertNotIn("草案注记", tex)
        self.assertIn("正文。", tex)

    def test_long_tables_break_across_pages(self) -> None:
        rows = "\n".join(f"| 词条{i} | 说明{i} |" for i in range(14))
        tex = md_to_tex(f"| 术语 | 定义 |\n|---|---|\n{rows}\n", chapter_from_h1=False)
        self.assertIn(r"\begin{xltabular}{\linewidth}", tex)
        self.assertIn(r"\endhead", tex)
        self.assertNotIn(r"\begin{center}", tex)

    def test_superscript_and_subscript_runs_become_math(self) -> None:
        tex = md_to_tex("每小时 10⁻⁹ 与 λ₃ 的量级。\n", chapter_from_h1=False)
        self.assertIn(r"\(^{-9}\)", tex)
        self.assertIn(r"\(_{3}\)", tex)


if __name__ == "__main__":
    unittest.main()
