"""Offline behavioral checks. Synthetic PDF bytes are not PDF/rendering fixtures."""
import csv
import json
import tempfile
import unittest
from pathlib import Path

from nx_review_helpers import audit_packet, centered_row, safe_member, sha, windows_identity


class RowTests(unittest.TestCase):
    def test_historical_native_variant_projection(self):
        path = Path(__file__).resolve().parents[1] / 'assets/nx-drawing-review/printing-20260907.evidence.json'
        evidence = json.loads(path.read_text())
        for variant in evidence['variants']:
            control = variant['parts'][0]
            plan = centered_row(control['length_mm'], 80, 42)
            for part in variant['parts']:
                self.assertEqual(part['hole_count'], plan['count'])
                self.assertEqual(part['hole_centers_x_mm'], plan['centers'])
                margin = (part['length_mm'] - (plan['count'] - 1) * 80) / 2
                self.assertAlmostEqual(part['measured_end_margin_mm'], margin)

    def test_even_count_stays_centered(self):
        plan = centered_row(524, 80, 42, center=17.5)
        self.assertEqual(plan['count'], 6)
        self.assertNotIn(17.5, plan['centers'])
        for left, right in zip(plan['centers'], reversed(plan['centers'])):
            self.assertAlmostEqual(left + right, 35)

    def test_exact_count_transition_and_decimal_pitch(self):
        self.assertEqual(centered_row('723.999999999', 80, 42)['count'], 8)
        self.assertEqual(centered_row('724', 80, 42)['count'], 9)
        self.assertEqual(centered_row('.3', '.1', '.05')['count'], 3)
        self.assertEqual(centered_row(84, 80, 42)['centers'], [0.0])

    def test_invalid_or_infeasible_row(self):
        for args in [(83, 80, 42), (100, 0, 10), ('NaN', 80, 42), (100, 80, -1)]:
            with self.subTest(args=args), self.assertRaises(ValueError):
                centered_row(*args)


class PacketTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.rows, self.parts, self.checks = [], [], []
        for part_name, sheets, issue in [('A', ['SHT1', 'SHT2'], False), ('B', ['SHT1'], True)]:
            part = {'set': 'fixture', 'file': part_name + '.prt', 'baseline_pdfs': [],
                    'updated_pdfs': [], 'review_required': issue, 'errors': [],
                    'after': {'stale_views': [], 'retained_dimensions': ['D1'] if issue else []}}
            for sheet in sheets:
                number = len(self.rows) + 1
                blank = part_name == 'A' and sheet == 'SHT2'
                target, comparison = f'pdf/{number:03d}.pdf', (f'baseline/{number:03d}.pdf' if issue else '')
                for kind, name in [('updated_pdfs', target), ('baseline_pdfs', comparison)]:
                    data = ('SYNTHETIC HASH/JOIN FIXTURE; NOT A PDF: ' + part_name + sheet + kind).encode()
                    if name:
                        path = self.root / name
                        path.parent.mkdir(exist_ok=True)
                        path.write_bytes(data)
                        digest = sha(path)
                        self.checks.append({'file': name, 'sha256': digest,
                                            'content_status': 'blank_template' if blank else 'drawing_content_present',
                                            'layout_review_required': issue})
                    else:
                        import hashlib
                        digest = hashlib.sha256(data).hexdigest()
                    part[kind].append({'sheet': sheet, 'sha256': digest})
                self.rows.append({'序号': number, '机型': 'fixture', '零件': part_name, '图纸页': sheet,
                                  '总装引用': '否，独立图纸' if issue else '是',
                                  '审核状态': '空白图框，需确认或补图' if blank else ('优先核对' if issue else '已刷新，待工程审核'),
                                  '重点核对原因': '图面排版：synthetic layout finding' if issue else '',
                                  '图纸PDF': target, '刷新前对照PDF': comparison})
            self.parts.append(part)
        self.native = {'finished': True, 'export_complete': True, 'errors': [], 'sheet_count': 3,
                       'sets': [{'set': 'fixture', 'part_file_count': 2,
                                 'inspected_part_files': ['a.prt', 'B.PRT'], 'load_errors': [],
                                 'assembly_part_files': ['a.PRT'],
                                 'drawing_inventory': {'A.prt': ['SHT1', 'SHT2'], 'b.prt': ['SHT1']}}],
                       'parts': self.parts}
        self.flush()

    def flush(self):
        records = self.root / '验证记录'
        records.mkdir(exist_ok=True)
        (records / '原生导出记录.json').write_text(json.dumps(self.native), encoding='utf-8')
        (records / 'PDF检查.json').write_text(json.dumps(self.checks), encoding='utf-8')
        with (self.root / '审核清单.csv').open('w', encoding='utf-8-sig', newline='') as stream:
            writer = csv.DictWriter(stream, fieldnames=list(self.rows[0]))
            writer.writeheader()
            writer.writerows(self.rows)
        paths = sorted(p for p in self.root.rglob('*') if p.is_file() and p.name != 'SHA256SUMS.txt')
        (self.root / 'SHA256SUMS.txt').write_text(''.join(sha(p) + '  ' + p.relative_to(self.root).as_posix() + '\n'
                                                      for p in paths), encoding='utf-8')

    def rejects(self):
        self.flush()
        with self.assertRaises(ValueError):
            audit_packet(self.root)

    def test_complete_with_issues_is_not_engineering_acceptance(self):
        result = audit_packet(self.root)
        self.assertEqual(result['primary_sheets'], 3)
        self.assertEqual(result['priority_review_sheets'], 2)
        self.assertEqual(result['independent_sheets'], 1)
        self.assertEqual(result['engineering_acceptance'], 'not_assessed')

    def test_case_normalization(self):
        self.assertEqual(windows_identity('DIR/A.PRT'), windows_identity('dir\\a.prt'))
        self.assertEqual(audit_packet(self.root)['independent_sheets'], 1)

    def test_assembly_only_omits_independent_sheet(self):
        self.rows.pop()
        self.rejects()

    def test_second_sheet_omission(self):
        self.rows.pop(1)
        self.rows[1]['序号'] = 2
        self.rejects()

    def test_blank_template_labeled_clean(self):
        self.rows[1]['审核状态'] = '已刷新，待工程审核'
        self.rejects()

    def test_layout_and_association_labeled_clean(self):
        self.rows[2]['审核状态'] = '已刷新，待工程审核'
        self.rejects()

    def test_stale_state_overrides_false_review_flag(self):
        self.parts[0]['after']['stale_views'] = ['View1']
        self.rejects()

    def test_unknown_content_is_not_clean(self):
        self.checks[0]['content_status'] = 'unknown'
        self.rejects()

    def test_unrecognized_acceptance_label_is_rejected(self):
        self.rows[2]['审核状态'] = '已审核通过'
        self.rejects()

    def test_missing_native_review_status_is_rejected(self):
        self.parts[0].pop('review_required')
        self.rejects()

    def test_swapped_sheet_pdf(self):
        self.rows[0]['图纸PDF'], self.rows[1]['图纸PDF'] = self.rows[1]['图纸PDF'], self.rows[0]['图纸PDF']
        self.rejects()

    def test_missing_comparison(self):
        self.rows[2]['刷新前对照PDF'] = ''
        self.rejects()

    def test_wrong_assembly_membership(self):
        self.rows[0]['总装引用'] = '否，独立图纸'
        self.rejects()

    def test_export_not_finished(self):
        self.native['finished'] = False
        self.rejects()

    def test_duplicate_sheet_identity(self):
        self.native['sets'][0]['drawing_inventory']['A.prt'].append('SHT1')
        self.rejects()

    def test_tampered_pdf_bytes(self):
        (self.root / self.rows[0]['图纸PDF']).write_bytes(b'tampered')
        with self.assertRaises(ValueError):
            audit_packet(self.root)

    def test_unlisted_file(self):
        (self.root / 'extra.pdf').write_bytes(b'extra')
        with self.assertRaises(ValueError):
            audit_packet(self.root)

    def test_unsafe_cross_platform_paths(self):
        for name in ['../escape.pdf', '..\\escape.pdf', 'C:\\outside.pdf', '//server/share/file.pdf']:
            with self.subTest(name=name), self.assertRaises(ValueError):
                safe_member(self.root, name)

    def test_duplicate_case_colliding_hash_entry(self):
        manifest = self.root / 'SHA256SUMS.txt'
        manifest.write_text(manifest.read_text() + sha(self.root / 'pdf/001.pdf') + '  PDF/001.PDF\n')
        with self.assertRaises(ValueError):
            audit_packet(self.root)


if __name__ == '__main__':
    unittest.main()
