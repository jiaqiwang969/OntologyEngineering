#!/usr/bin/env python3
"""Offline helpers for centered hole rows and the NX review-export packet format.

No NX calls or model writes. Packet verification covers bytes, inventory joins,
and known-issue labeling; it does not certify geometry or manufacturing drawings.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import ntpath
import re
from decimal import Decimal, ROUND_FLOOR
from pathlib import Path, PurePosixPath


def require(condition, message):
    if not condition:
        raise ValueError(message)


def centered_row(length, pitch, minimum_end_margin, center=0):
    """Maximum count at fixed pitch, equal end margins, and a declared center."""
    length, pitch, margin, center = [Decimal(str(x)) for x in
                                      (length, pitch, minimum_end_margin, center)]
    require(all(x.is_finite() for x in (length, pitch, margin, center)), 'non-finite row parameter')
    require(length > 0 and pitch > 0 and margin >= 0, 'invalid length, pitch, or minimum margin')
    require(length >= 2 * margin, 'no hole center fits the specified end margins')
    count = int(((length - 2 * margin) / pitch).to_integral_value(rounding=ROUND_FLOOR)) + 1
    require(count <= 100000, 'row exceeds this helper\'s 100000-center output limit')
    points = [center + (Decimal(i) - Decimal(count - 1) / 2) * pitch for i in range(count)]
    actual_margin = (length - Decimal(count - 1) * pitch) / 2
    return {'count': count, 'centers': [float(x) for x in points],
            'end_margin': float(actual_margin), 'pitch': float(pitch),
            'center': float(center), 'scope': 'layout_math_only; profile supplies units and design intent'}


def windows_identity(value):
    require(isinstance(value, str) and bool(value.strip()), 'missing Windows file identity')
    return ntpath.normpath(value.replace('/', '\\')).casefold()


def safe_member(root, value):
    require(isinstance(value, str) and bool(value), 'missing packet member path')
    value = value.replace('\\', '/')
    member = PurePosixPath(value)
    require(not member.is_absolute() and not ntpath.splitdrive(value)[0]
            and '..' not in member.parts, 'unsafe packet member: ' + value)
    path = root.joinpath(*member.parts)
    require(path.resolve().is_relative_to(root.resolve()), 'member escapes packet: ' + value)
    return path


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def audit_packet(root):
    """Read the normalized export record, CSV, PDF checks, and SHA256SUMS."""
    root = Path(root)
    sums = {}
    aliases = set()
    for line in (root / 'SHA256SUMS.txt').read_text(encoding='utf-8').splitlines():
        match = re.fullmatch(r'([0-9a-fA-F]{64})  (.+)', line)
        require(match is not None, 'malformed SHA256SUMS entry')
        digest, name = match.groups()
        member = safe_member(root, name)
        relative = member.relative_to(root).as_posix()
        alias = windows_identity(relative)
        require(alias not in aliases, 'duplicate/case-colliding packet member: ' + name)
        aliases.add(alias)
        require(member.is_file() and not member.is_symlink(), 'missing or linked packet member: ' + name)
        require(sha(member) == digest.lower(), 'packet hash mismatch: ' + name)
        sums[relative] = digest.lower()
    actual = {p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()
              and p.relative_to(root).as_posix() != 'SHA256SUMS.txt'}
    require(set(sums) == actual, 'manifest does not close the packet file inventory')

    def read_json(name):
        require(name in sums, 'record is not hash-bound: ' + name)
        return json.loads((root / name).read_text(encoding='utf-8-sig'))

    native = read_json('验证记录/原生导出记录.json')
    pdf_checks = read_json('验证记录/PDF检查.json')
    require(native['finished'] is True and native['export_complete'] is True and not native['errors'],
            'native export did not complete')
    sets, expected = {}, set()
    for spec in native['sets']:
        require(spec['set'] not in sets, 'duplicate model set')
        sets[spec['set']] = spec
        inspected = [windows_identity(x) for x in spec['inspected_part_files']]
        require(len(inspected) == len(set(inspected)) == spec['part_file_count'], 'incomplete/duplicate PRT scan')
        require(not spec['load_errors'], 'PRT load errors leave inventory incomplete')
        for filename, sheets in spec['drawing_inventory'].items():
            require(windows_identity(filename) in inspected, 'drawing part absent from full-folder scan')
            for sheet in sheets:
                key = (spec['set'], windows_identity(filename), sheet)
                require(key not in expected, 'duplicate native drawing sheet')
                expected.add(key)
    parts = {}
    for part in native['parts']:
        key = (part['set'], windows_identity(part['file']))
        require(key not in parts, 'duplicate native part record')
        parts[key] = part
    checks = {}
    for check in pdf_checks:
        path = safe_member(root, check['file']).relative_to(root).as_posix()
        require(path not in checks, 'duplicate PDF check')
        require(check['sha256'] == sums.get(path), 'PDF check does not bind the delivered bytes: ' + path)
        checks[path] = check

    index_name = '审核清单.csv'
    require(index_name in sums, 'CSV index is not hash-bound')
    with (root / index_name).open(encoding='utf-8-sig', newline='') as stream:
        rows = list(csv.DictReader(stream))
    seen, links = set(), set()
    counts = {'primary_sheets': 0, 'association_review_sheets': 0, 'layout_review_sheets': 0,
              'blank_template_sheets': 0, 'priority_review_sheets': 0,
              'baseline_comparisons': 0, 'independent_sheets': 0}
    for position, row in enumerate(rows, 1):
        require(int(row['序号']) == position, 'nonsequential drawing index')
        key = (row['机型'], windows_identity(row['零件'] + '.prt'), row['图纸页'])
        require(key not in seen and key in expected, 'duplicate or unexpected indexed sheet')
        seen.add(key)
        part, spec = parts[key[:2]], sets[key[0]]
        referenced = key[1] in {windows_identity(x) for x in spec['assembly_part_files']}
        require((row['总装引用'] == '是') == referenced, 'incorrect assembly membership after Windows identity join')
        counts['independent_sheets'] += not referenced
        versions = {}
        for kind in ('baseline_pdfs', 'updated_pdfs'):
            items = [entry for entry in part[kind] if entry['sheet'] == key[2]]
            require(len(items) <= 1, 'duplicate PDF version for one native sheet')
            versions[kind] = items[0] if items else None
        primary = versions['updated_pdfs'] or versions['baseline_pdfs']
        require(primary is not None, 'no exported PDF for indexed sheet')
        target = safe_member(root, row['图纸PDF']).relative_to(root).as_posix()
        require(target not in links, 'PDF path reused for multiple sheets')
        require(sums.get(target) == primary['sha256'], 'primary PDF does not match its native sheet export')
        require(target in checks, 'missing content observation for primary PDF')
        check = checks[target]
        require(check['content_status'] in ('blank_template', 'drawing_content_present'), 'unknown drawing content state')
        require(type(check['layout_review_required']) is bool, 'missing layout observation state')
        blank = check['content_status'] == 'blank_template'
        layout = check['layout_review_required']
        require(type(part.get('review_required')) is bool, 'missing native review status')
        after = part.get('after', {})
        association = (part.get('review_required') is not False or bool(part.get('errors'))
                       or bool(part.get('refresh_error')) or bool(after.get('stale_views'))
                       or bool(after.get('retained_dimensions')) or versions['updated_pdfs'] is None)
        priority = bool(blank or layout or association)
        expected_status = ('空白图框，需确认或补图' if blank else
                           ('优先核对' if association else
                            ('优先核对（排版）' if layout else '已刷新，待工程审核')))
        require(row['审核状态'] == expected_status, 'drawing status does not match the recorded observations')
        require(not layout or '图面排版：' in row['重点核对原因'], 'layout finding is missing from index notes')
        links.add(target)
        comparison = row['刷新前对照PDF']
        if (association or layout) and all(versions.values()):
            require(bool(comparison), 'missing saved-state comparison for a flagged drawing')
        if comparison:
            relative = safe_member(root, comparison).relative_to(root).as_posix()
            require(relative not in links and relative in checks, 'reused or unchecked comparison PDF')
            require(versions['baseline_pdfs'] is not None and
                    sums.get(relative) == versions['baseline_pdfs']['sha256'], 'comparison belongs to a different native sheet')
            links.add(relative)
            counts['baseline_comparisons'] += 1
        counts['primary_sheets'] += 1
        counts['association_review_sheets'] += bool(association)
        counts['layout_review_sheets'] += layout
        counts['blank_template_sheets'] += blank
        counts['priority_review_sheets'] += priority
    require(seen == expected, 'drawing inventory contains omitted sheets')
    require(links == set(checks), 'PDF observations and indexed PDFs differ')
    require(len(seen) == native['sheet_count'], 'reported sheet count differs from inventory')
    return {'packet_integrity': 'verified', 'engineering_acceptance': 'not_assessed',
            'native_reexecution': False, 'visual_reinspection': False,
            'scope': 'hashes, inventory, native-export/PDF joins, existing issue labels; source PRTs not reopened',
            'verified_files': len(sums) + 1, 'verified_pdf_links': len(links), **counts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    plan = commands.add_parser('plan-row', help='fixed-pitch, centered row from profile-supplied values')
    for name in ('length', 'pitch', 'minimum-end-margin'):
        plan.add_argument('--' + name, required=True)
    plan.add_argument('--center', default='0')
    audit = commands.add_parser('audit-packet', help='read-only audit of the normalized NX review-export packet')
    audit.add_argument('directory', type=Path)
    args = parser.parse_args()
    try:
        result = (centered_row(args.length, args.pitch, args.minimum_end_margin, args.center)
                  if args.command == 'plan-row' else audit_packet(args.directory))
    except (ValueError, KeyError, OSError, TypeError, ArithmeticError) as exc:
        print(json.dumps({'status': 'incomplete_or_invalid', 'error': str(exc)}, ensure_ascii=False))
        return 2
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
