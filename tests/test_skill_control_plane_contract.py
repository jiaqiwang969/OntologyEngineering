from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class SkillControlPlaneContractTests(unittest.TestCase):
    def test_skill_is_a_semantica_refinery_not_a_book_only_prompt(self) -> None:
        skill = (ROOT / "SKILL.md").read_text(encoding="utf-8")
        frontmatter = skill.split("---", 2)[1]
        for required in (
            "Semantica",
            "default semantic control and learning plane",
            "industry ontology",
            "TeX/PDF",
            "sole executable semantic authority",
        ):
            self.assertIn(required, frontmatter)
        self.assertIn("references/semantic-engagement-contract.md", skill)
        self.assertIn("skills/domain-ontology-loop/SKILL.md", skill)
        self.assertIn("references/book-authoring-workflow.md", skill)
        self.assertIn("--task /path/to/task-envelope.json", skill)
        self.assertNotIn("~/.codex/skills", skill)

    def test_agent_default_prompt_explicitly_invokes_skill(self) -> None:
        metadata = (ROOT / "agents" / "openai.yaml").read_text(encoding="utf-8")
        self.assertIn("$ontology-engineering", metadata)
        self.assertIn("Semantica", metadata)

    def test_public_docs_use_current_strict_gate_and_task_schema(self) -> None:
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        contract = (ROOT / "references" / "semantic-engagement-contract.md").read_text(
            encoding="utf-8"
        )
        combined = readme + "\n" + contract
        self.assertIn("--mode strict", readme)
        self.assertNotIn("check_semantica_backend_policy.py --strict", readme)
        self.assertIn("ontology-engineering.semantic-task-envelope/v1", combined)
        self.assertIn("ontology-engineering.semantic-project-binding/v1", combined)
        self.assertIn('"requested_actions": ["open", "run", "propose"]', contract)
        self.assertIn('"lifecycle_actions": ["candidate", "proposed"]', contract)
        self.assertIn(
            '"required_capabilities": ["declared-semantica-capability"]', contract
        )
        self.assertIn('"sha256": "0123456789abcdef', contract)
        self.assertNotIn('"schema_version": "1.0",\n  "task_id"', contract)
        self.assertNotIn(
            '"requested_actions": ["discover", "verify", "learn"]', contract
        )
        self.assertNotIn('"evidence_refs"', combined)

    def test_outer_loop_has_full_delta_and_no_machine_specific_repo_path(self) -> None:
        documents = [
            ROOT / "skills" / "domain-ontology-loop" / "SKILL.md",
            ROOT
            / "skills"
            / "domain-ontology-loop"
            / "references"
            / "loop-contract.md",
        ]
        text = "\n".join(path.read_text(encoding="utf-8") for path in documents)
        for required in (
            "ontology",
            "competency_questions",
            "shapes",
            "queries",
            "rules",
            "cases",
            "contract",
            "provenance",
            "book_impact",
            "regression_passed",
            "release_complete",
            "promoted",
        ):
            self.assertIn(required, text)
        personal_checkout = re.compile(r"~/148|/" + r"Users/[^/]+/148-Semantica")
        self.assertNotRegex(text, personal_checkout)

    def test_every_native_write_uses_a_current_exact_action_context(self) -> None:
        contract = (ROOT / "references" / "semantic-engagement-contract.md").read_text(
            encoding="utf-8"
        )
        outer = (
            ROOT / "skills" / "domain-ontology-loop" / "references" / "loop-contract.md"
        ).read_text(encoding="utf-8")
        combined = contract + "\n" + outer
        for action in (
            "candidate",
            "proposed",
            "committed",
            "execute_candidate",
            "derive_regression_gate",
            "regression_passed",
            "derive_release_gate",
            "release_complete",
            "promoted",
        ):
            self.assertIn(action, combined)
        self.assertIn("TransitionContextDTO", combined)
        self.assertIn("当前", combined)
        self.assertIn("context_sha256", combined)
        self.assertIn("auto_applied=false", combined)
        self.assertNotIn("| 任意 OE task | `refine`", contract)
        self.assertNotIn("不投影成 native transition authority", contract)
        self.assertIn("不接收调用方 gate JSON", outer)

    def test_formal_book_guides_use_the_unified_engagement_entrypoint(self) -> None:
        vol1_root = ROOT / "references" / "ontology-engineering-book"
        vol2_root = ROOT / "references" / "product-trustworthiness-book"
        source_lock = vol2_root / "handbook" / "formal-search-guides.sha256"
        chapter_readmes = []
        for raw in source_lock.read_text(encoding="utf-8").splitlines():
            if not re.match(r"^[0-9a-f]{64}  ch\d\d-[^/]+/README\.md$", raw):
                continue
            chapter_readmes.append(vol2_root / raw.split("  ", 1)[1])
        self.assertEqual(20, len(chapter_readmes))
        vol1_chapter_readmes = sorted(vol1_root.glob("ch??-*/README.md"))
        self.assertEqual(9, len(vol1_chapter_readmes))

        markdown_guides = [
            ROOT / "references" / "source-map.md",
            ROOT / "references" / "product-trustworthiness-source-map.md",
            vol1_root / "README.md",
            vol2_root / "README.md",
            *vol1_chapter_readmes,
            *chapter_readmes,
        ]
        for path in markdown_guides:
            text = path.read_text(encoding="utf-8")
            with self.subTest(path=path.relative_to(ROOT)):
                for command in ("discover", "run", "open"):
                    self.assertIn(f"scripts/semantic_engagement.py {command}", text)
                self.assertIn("semantic-engagement-contract.md", text)
                self.assertIn("底层", text)
                self.assertNotIn("SEMANTICA_RUNTIME_COMMIT", text)
                self.assertNotIn("SEMANTICA_RUNTIME_SHA256", text)
                self.assertNotIn("SEMANTICA_WHEEL_SHA256", text)
                self.assertNotIn("\nsemantica package run", text)

        tex = (
            ROOT
            / "references"
            / "ontology-engineering-book"
            / "handbook"
            / "chapters"
            / "ch09.tex"
        ).read_text(encoding="utf-8")
        normalized_tex = tex.replace(r"\_", "_").replace(r"\ ", " ")
        for command in ("discover", "run", "open"):
            self.assertIn(f"scripts/semantic_engagement.py {command}", normalized_tex)
        self.assertIn("semantic-engagement-contract.md", tex)
        self.assertIn("底层诊断接口", tex)
        self.assertNotIn("RUNTIME_COMMIT", tex)
        self.assertNotIn(r"semantica\ package\ run", tex)

    def test_archived_capstone_and_demos_do_not_teach_manual_runtime_identity(
        self,
    ) -> None:
        capstone_root = (
            ROOT
            / "references"
            / "product-trustworthiness-book"
            / "ch11-capstone-three-items"
        )
        capstone_documents = [
            capstone_root / "README.md",
            capstone_root / "chapter.md",
        ]
        for path in capstone_documents:
            text = path.read_text(encoding="utf-8")
            with self.subTest(path=path.relative_to(ROOT)):
                self.assertIn("scripts/semantic_engagement.py discover", text)
                self.assertIn("scripts/semantic_engagement.py run", text)
                self.assertIn("semantic-engagement-contract.md", text)
                self.assertIn("底层", text)
                self.assertNotRegex(
                    text,
                    re.compile(r"(?m)^\s*(?:runtime/\.venv/bin/)?semantica package"),
                )
                self.assertNotRegex(text, re.compile(r"SEMANTICA_(?:RUNTIME|WHEEL)"))

        demos = (ROOT / "demos" / "README.md").read_text(encoding="utf-8")
        self.assertIn("scripts/semantic_engagement.py discover", demos)
        self.assertIn("底层", demos)
        self.assertNotRegex(
            demos,
            re.compile(r"(?m)^\s*(?:runtime/\.venv/bin/)?semantica package"),
        )
        self.assertNotRegex(demos, re.compile(r"SEMANTICA_(?:RUNTIME|WHEEL)"))


if __name__ == "__main__":
    unittest.main()
