from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts import update_book_authoring_locks as locks


class BookAuthoringLockTests(unittest.TestCase):
    def test_checked_in_authoring_locks_are_current(self) -> None:
        report = locks.check_or_write(write=False)
        self.assertTrue(report["passed"], report)
        self.assertEqual(4, len(report["locks"]))
        self.assertTrue(all(not item["updated"] for item in report["locks"]))

    def test_render_lock_is_sorted_and_content_bound(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "z.txt").write_text("z\n", encoding="utf-8")
            (root / "a.txt").write_text("a\n", encoding="utf-8")
            rendered = locks.render_lock(
                root, ("z.txt", "a.txt"), "# test lock\n"
            )
            lines = rendered.splitlines()
            self.assertTrue(lines[1].endswith("  a.txt"))
            self.assertTrue(lines[2].endswith("  z.txt"))
            self.assertNotEqual(lines[1].split()[0], lines[2].split()[0])

    def test_lock_specs_use_volume_roots_and_a_separate_vol2_guide_lock(self) -> None:
        specs = locks.lock_specs()
        self.assertEqual(
            [
                "vol1_sources",
                "vol2_sources",
                "vol2_assets",
                "vol2_search_guides",
            ],
            [spec[0] for spec in specs],
        )
        self.assertEqual(locks.VOL1_BOOK, specs[0][1])
        self.assertEqual(
            locks.VOL1_BOOK / "authoring-sources.sha256",
            specs[0][2],
        )
        self.assertEqual(
            locks.VOL2_BOOK / "handbook" / "formal-search-guides.sha256",
            specs[3][2],
        )
        self.assertEqual(
            specs[3][3], specs[3][2].read_text(encoding="utf-8")
        )

    def test_missing_authoring_input_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(locks.AuthoringLockError):
                locks.vol1_paths(Path(temporary))

    def test_render_lock_rejects_noncanonical_or_escaping_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "safe.txt").write_text("safe\n", encoding="utf-8")
            for relative in (
                "../escape.txt",
                "./safe.txt",
                "safe//file.txt",
                "safe\\file.txt",
                " safe.txt",
                "safe.txt ",
                str((root / "safe.txt").resolve()),
            ):
                with self.subTest(relative=relative):
                    with self.assertRaises(locks.AuthoringLockError):
                        locks.render_lock(root, (relative,), "# test lock\n")

    def test_vol1_lock_root_covers_guides_and_handbook_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            required = {
                "README.md",
                "resources/README.md",
                "handbook/README.md",
                "handbook/build_handbook.py",
                "handbook/gen_figures.py",
                "handbook/main.tex",
                "handbook/make_deck_plan.py",
                "handbook/preamble.tex",
                "handbook/test_authoring_sources.py",
                *(f"{directory}/README.md" for directory in locks.VOL1_CHAPTER_DIRS),
            }
            for relative in required:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(f"fixture: {relative}\n", encoding="utf-8")
            for directory in ("chapters", "figures", "fragments"):
                path = root / "handbook" / directory / f"{directory}.txt"
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(f"fixture: {directory}\n", encoding="utf-8")

            selected = set(locks.vol1_paths(root))
            self.assertTrue(required <= selected)
            self.assertIn("handbook/chapters/chapters.txt", selected)
            self.assertIn("handbook/figures/figures.txt", selected)
            self.assertIn("handbook/fragments/fragments.txt", selected)
            self.assertTrue(all(".." not in Path(path).parts for path in selected))

            legacy_lock = root / "handbook" / "authoring-sources.sha256"
            legacy_lock.write_text("retired lock\n", encoding="utf-8")
            with self.assertRaises(locks.AuthoringLockError):
                locks.vol1_paths(root)

    def test_vol2_formal_search_guide_lock_is_closed_over_all_guides(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            required = {
                "README.md",
                "propositions-index.md",
                "handbook/README.md",
                *(f"{directory}/README.md" for directory in locks.VOL2_CHAPTER_DIRS),
            }
            for relative in required:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(f"fixture: {relative}\n", encoding="utf-8")

            selected = locks.vol2_search_guide_paths(root)
            self.assertEqual(23, len(selected))
            self.assertEqual(required, set(selected))


if __name__ == "__main__":
    unittest.main()
