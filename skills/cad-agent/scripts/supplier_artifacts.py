"""Inspect newly received supplier files without equating bytes with engineering acceptance."""
from __future__ import annotations

import hashlib
import io
from pathlib import Path
import re
import zipfile

LIMIT = 50_000_000


def stamp(path):
    s = path.stat()
    return [s.st_size, s.st_mtime_ns, s.st_ino]


def file_kind(label):
    text = label.upper().strip()
    for name in ('STEP', 'PARASOLID', 'IGES', 'ACIS', 'DXF', 'DWG', 'PDF', 'NX'):
        if text.startswith(name):
            return name
    return 'OTHER'


def inspect_file(name, blob, part, selected_label):
    if not blob or len(blob) > LIMIT:
        raise ValueError('empty_or_oversized_supplier_file')
    if blob.lstrip().lower().startswith((b'<!doctype html', b'<html')):
        raise ValueError('HTML_is_not_the_requested_artifact')
    suffix = Path(name).suffix.lower()
    expected = file_kind(selected_label)
    extensions = {'STEP': {'.stp', '.step'}, 'PARASOLID': {'.x_t', '.x_b', '.xmt_txt', '.xmt_bin'},
                  'IGES': {'.igs', '.iges'}, 'ACIS': {'.sat', '.sab'}, 'DXF': {'.dxf'},
                  'DWG': {'.dwg'}, 'PDF': {'.pdf'}, 'NX': {'.prt'}}
    if expected in extensions and suffix not in extensions[expected]:
        raise ValueError('selected_format_and_file_extension_mismatch')
    info = {'name': name, 'bytes': len(blob), 'sha256': hashlib.sha256(blob).hexdigest(),
            'requested_format': selected_label, 'format_family': expected,
            'format_check': 'native_reader_required', 'units': 'unknown',
            'part_identity': 'page_and_filename_only', 'engineering_acceptance': 'not_evaluated'}
    if expected == 'STEP':
        if not (blob.lstrip().startswith(b'ISO-10303-21;') and
                blob.rstrip().endswith(b'END-ISO-10303-21;')):
            raise ValueError('incomplete_STEP')
        schema = re.search(rb"FILE_SCHEMA\s*\(\s*\(\s*'([^']+)'", blob)
        if not schema:
            raise ValueError('STEP_schema_missing')
        info['schema'] = schema[1].decode('ascii', errors='replace')
        info['format_check'] = 'STEP_envelope_and_schema_present'
        products = re.findall(rb"\bPRODUCT\s*\(\s*'([^']*)'", blob)
        permitted = {part.encode(), (part + '_1').encode()}
        if len(products) != 1 or products[0] not in permitted:
            raise ValueError('STEP_product_identity_mismatch')
        info.update(product=products[0].decode(), part_identity='exact_or_observed_single_part_suffix')
        # Record unit evidence without inferring the active model unit from an
        # arbitrary SI_UNIT entity. Native import must resolve its context.
        info['millimetre_unit_definition_present'] = bool(re.search(
            rb'SI_UNIT\s*\(\s*\.MILLI\.\s*,\s*\.METRE\.\s*\)', blob))
    elif expected == 'DXF':
        if blob.startswith(b'AutoCAD Binary DXF'):
            info['format_check'] = 'binary_DXF_signature_present'
        elif all(token in blob for token in (b'SECTION', b'HEADER', b'EOF')):
            info['format_check'] = 'ASCII_DXF_structure_present'
            unit = re.search(rb'\$INSUNITS\s+70\s+(\d+)', blob)
            if unit:
                info['drawing_unit_code'] = int(unit[1])
                info['units'] = 'mm' if int(unit[1]) == 4 else 'unknown'
        else:
            raise ValueError('DXF_signature_missing')
    elif expected == 'DWG':
        if not re.match(rb'AC10\d\d', blob):
            raise ValueError('DWG_signature_missing')
        info['format_check'] = 'DWG_signature_present'
    elif expected == 'PDF':
        if not blob.startswith(b'%PDF-') or b'%%EOF' not in blob[-2048:]:
            raise ValueError('incomplete_PDF')
        info['format_check'] = 'PDF_envelope_present'
    elif expected == 'IGES':
        lines = blob.splitlines()
        sections = {line[72:73] for line in lines if len(line) >= 73}
        if not {b'S', b'G', b'D', b'P', b'T'}.issubset(sections):
            raise ValueError('IGES_sections_missing')
        info['format_check'] = 'IGES_sections_present'
    # Native/binary CAD stays acquired with reader verification pending. Never
    # reject a useful available format merely because STEP is the only parser.
    return info


def inspect_download(filename, blob, part, selected_label):
    if len(blob) > LIMIT:
        raise ValueError('supplier_download_too_large')
    files = {}
    zipped = filename.lower().endswith('.zip')
    if zipped:
        with zipfile.ZipFile(io.BytesIO(blob)) as z:
            members = z.infolist()
            if not members or len(members) > 64 or sum(i.file_size for i in members) > LIMIT:
                raise ValueError('supplier_archive_limits_exceeded')
            if z.testzip() is not None:
                raise ValueError('archive_CRC_failure')
            for i in members:
                if i.is_dir():
                    continue
                if (Path(i.filename).name != i.filename or '\\' in i.filename or
                        i.filename in files or (i.external_attr >> 16) & 0o170000 == 0o120000):
                    raise ValueError('unsafe_archive_member')
                if not re.match(re.escape(part) + r'(?:[_. -]|$)', i.filename, re.I):
                    raise ValueError('archive_member_part_mismatch')
                files[i.filename] = z.read(i)
    else:
        files[filename] = blob
    if not files:
        raise ValueError('empty_supplier_archive')
    receipts = [inspect_file(name, data, part, selected_label) for name, data in files.items()]
    return {'part': part, 'download_name': filename, 'bytes': len(blob),
            'sha256': hashlib.sha256(blob).hexdigest(), 'zip_crc_valid': True if zipped else None,
            'files': receipts, 'status': 'artifact_acquired', 'engineering_acceptance': 'not_evaluated'}, files


class FreshSupplierDownload:
    def __init__(self, directory, part, selected_label):
        if not re.fullmatch(r'[A-Za-z0-9_.-]{1,120}', part):
            raise ValueError('confirmed_simple_part_required')
        self.directory, self.part, self.label = Path(directory), part, selected_label
        self.before = {str(p): stamp(p) for p in self.candidates()}
        self.last = {}

    def candidates(self):
        return [p for p in self.directory.iterdir() if
                not p.name.endswith(('.crdownload', '.tmp')) and
                re.match(re.escape(self.part) + r'(?:[_. -]|$)', p.name, re.I) and p.is_file()]

    def poll(self):
        fresh = [p for p in self.candidates() if self.before.get(str(p)) != stamp(p)]
        # A browser may save a ZIP and the OS may also expand it. Use the actual
        # download archive; its complete contents are independently checked.
        archives = [p for p in fresh if p.suffix.lower() == '.zip']
        if archives:
            fresh = archives
        if len(fresh) > 1:
            raise ValueError('multiple_new_downloads_ambiguous')
        for p in fresh:
            state = stamp(p)
            if Path(str(p) + '.crdownload').exists() or self.last.get(str(p)) != state:
                self.last[str(p)] = state
                return None
            if state[0] > LIMIT:
                raise ValueError('supplier_download_too_large')
            blob = p.read_bytes()
            if stamp(p) != state:
                return None
            receipt, files = inspect_download(p.name, blob, self.part, self.label)
            return {**receipt, 'original_download': str(p), 'file_stamp': state}, blob, files
        return None
