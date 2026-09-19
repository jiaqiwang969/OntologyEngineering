"""Check reporting correspondence and arithmetic, without semantic inference."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import manufacturing_report as report
EXAMPLE = report.ASSETS / "example"


class ManufacturingReportTests(unittest.TestCase):
    def sample(self, name="03-resource-cost.json"):
        return json.loads((EXAMPLE / name).read_text())

    def test_no_future_sources_and_cost_changes(self):
        names = ["01-inquiry.json", "02-technical.json", "03-resource-cost.json", "04-feedback.json"]
        for i, name in enumerate(names, 1):
            s = self.sample(name)
            report.validate(s)
            self.assertEqual({r["id"] for r in s["sources"]}, {"S" + str(n) for n in range(1, i + 1)})
        early = report.cost_result(self.sample()["cost"])
        later = report.cost_result(self.sample(names[-1])["cost"])
        self.assertEqual(early["known_subtotal"], "2088.00")
        self.assertEqual(early["omitted_ids"], ["C-QC", "C-PACK"])
        self.assertEqual(later["known_subtotal"], "2556.00")
        self.assertEqual(later["omitted_ids"], [])

    def test_unknown_quantity_does_not_remove_setup_or_make_zero_unit_total(self):
        s = self.sample()
        s["cost"]["quantity"] = None
        report.validate(s)
        result = report.cost_result(s["cost"])
        self.assertEqual(result["known_subtotal"], "720.00")
        self.assertIn("C-MAT", result["omitted_ids"])

    def test_decimal_batch_cost_and_special_characters(self):
        s = self.sample()
        s["cost"]["lines"] = [{**s["cost"]["lines"][0], "value": "0.10", "basis": "per_batch", "count": 3}]
        self.assertEqual(report.cost_result(s["cost"])["known_subtotal"], "0.30")
        escaped = report.tex(r"A&B_2 5% {draft} \input{secret}")
        self.assertIn(r"\&", escaped)
        self.assertIn(r"\textbackslash{}input\{secret\}", escaped)

    def test_dangling_source_and_cross_collection_id_collision_fail(self):
        s = self.sample()
        s["records"][0]["source_ids"] = ["MISSING"]
        with self.assertRaisesRegex(ValueError, "dangling"):
            report.validate(s)
        s = self.sample()
        s["cost"]["lines"][0]["id"] = s["records"][0]["id"]
        with self.assertRaisesRegex(ValueError, "overlap"):
            report.validate(s)

    def test_unknown_view_and_missing_trigger_fail(self):
        for mutation in (lambda s: s["views"][0].update(kind="automatic_approval"), lambda s: s["views"][0].pop("update_triggers")):
            s = self.sample()
            mutation(s)
            with self.assertRaises(ValueError):
                report.validate(s)

    def test_no_local_semantic_pass_claim(self):
        s = self.sample()
        s["semantic_status"] = "passed"
        with self.assertRaisesRegex(ValueError, "cannot attest"):
            report.validate(s)

    def test_no_double_quantity_count_or_nonfinite_amount(self):
        for value, count in [("Infinity", 1), (True, 1), (1.1, 1), ("10", 2)]:
            s = self.sample()
            s["cost"]["lines"][0].update(value=value, count=count)
            with self.assertRaises(ValueError):
                report.validate(s)

    def test_view_selection_is_explicit_and_sources_stay_bound(self):
        s = self.sample()
        s["views"] = [s["views"][0]]
        s["views"][0]["items"] = ["R-EQUIP"]
        body, mapping = report.render(s, (report.ASSETS / "preamble.tex").read_text())
        self.assertEqual(mapping["views"][0]["selected_ids"], ["R-EQUIP"])
        self.assertEqual(mapping["bindings"]["R-EQUIP"]["source_ids"], ["S3"])
        self.assertIn("资源可用，能力待验", body)
        self.assertNotIn("按上述条件计算的小计：", body)

    def test_generation_freezes_input_and_rejects_overwrite(self):
        with tempfile.TemporaryDirectory() as name:
            out = Path(name) / "report"
            result = report.generate(EXAMPLE / "03-resource-cost.json", out)
            self.assertTrue(result["passed"])
            self.assertFalse(result["compiled"])
            self.assertEqual((out / "snapshot.json").read_bytes(), (EXAMPLE / "03-resource-cost.json").read_bytes())
            with self.assertRaisesRegex(ValueError, "exists"):
                report.generate(EXAMPLE / "03-resource-cost.json", out)

    def test_rejects_frozen_output_tamper(self):
        with tempfile.TemporaryDirectory() as name:
            out = Path(name) / "report"
            report.generate(EXAMPLE / "03-resource-cost.json", out)
            with (out / "report.tex").open("a") as f:
                f.write("changed")
            self.assertFalse(report.verify(out)["passed"])

    def test_relocked_tex_still_must_match_snapshot(self):
        with tempfile.TemporaryDirectory() as name:
            out = Path(name) / "report"
            report.generate(EXAMPLE / "03-resource-cost.json", out)
            file = out / "report.tex"
            file.write_text(file.read_text().replace("2088.00", "9999.00"))
            m = json.loads((out / "manifest.json").read_text())
            next(r for r in m["files"] if r["path"] == "report.tex")["sha256"] = report.digest(file.read_bytes())
            (out / "manifest.json").write_text(json.dumps(m))
            self.assertFalse(report.verify(out)["passed"])


if __name__ == "__main__":
    unittest.main()
