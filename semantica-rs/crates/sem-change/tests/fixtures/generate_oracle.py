"""Generate tests/fixtures/oracle.json from the original Semantica (the oracle).

Every expected value used by the sem-change characterisation tests comes from running the
original code here. Run from var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python -I \
        <crate>/tests/fixtures/generate_oracle.py <crate>/tests/fixtures/oracle.json <scratch-dir>

The script only computes values: no network, no credentials, no writes outside the output file
and the scratch directory (SQLite files are created there and removed afterwards).
"""

import json
import os
import random
import shutil
import sqlite3
import struct
import sys
import tempfile
from datetime import datetime
from urllib.parse import quote, urljoin, urlparse

sys.path.insert(0, os.getcwd())

from semantica.change_management import (  # noqa: E402
    ChangeLogEntry,
    InMemoryVersionStorage,
    OntologyVersionManager,
    SQLiteVersionStorage,
    TemporalVersionManager,
    VersionManager,
    compute_checksum,
    verify_checksum,
)
from semantica.change_management.change_log import (  # noqa: E402
    ChangeCategory,
    ChangeLogAnalyzer,
    ImpactReport,
    MutationRecord,
    Severity,
    generate_change_report,
)
from semantica.change_management.ontology_version_manager import OntologyVersion  # noqa: E402
from semantica.change_management.version_storage import create_graph_snapshot_record  # noqa: E402
from semantica.ontology.namespace_manager import NamespaceManager  # noqa: E402
from semantica.utils.progress_tracker import get_progress_tracker  # noqa: E402

OUT = {}
OUTPUT = sys.argv[1]
SCRATCH = tempfile.mkdtemp(dir=sys.argv[2] if len(sys.argv) > 2 else os.path.dirname(OUTPUT))


def err(e):
    return {"error": type(e).__name__, "message": str(e)}


def run(f, *a, **k):
    try:
        return {"ok": f(*a, **k)}
    except Exception as e:  # noqa: BLE001
        return err(e)


def mask(value, keys=("timestamp", "created_at")):
    """Replace time-dependent fields (recursively) with "<masked>"."""
    if isinstance(value, dict):
        return {k: ("<masked>" if k in keys else mask(v, keys)) for k, v in value.items()}
    if isinstance(value, list):
        return [mask(v, keys) for v in value]
    return value


def db(name):
    return os.path.join(SCRATCH, name)


def snap(value):
    """The value as returned at this moment (Python results may alias live objects)."""
    return json.loads(json.dumps(value))


def tracker_off():
    t = get_progress_tracker()
    t.displays = []


# ---------------------------------------------------------------------------------------------
# datetime.fromisoformat and ChangeLogEntry validation
# ---------------------------------------------------------------------------------------------

ISO_TARGETED = [
    "2024-01-15T10:30:00", "2024-01-15T10:30:00+00:00", "2024-01-15T10:30:00.123+00:00",
    "2024-01-15T10:30:00.123456+00:00", "2024-01-15T10:30:00+05:30", "2024-01-15T10:30:00-08:00",
    "2024-01-15", "20240115", "20240115T103000", "2024-01-15T10", "2024-01-15T10:30",
    "2024-01-15T1030", "2024-01-15T103000", "2024-01-15T10:30:00.1", "2024-01-15T10:30:00.12",
    "2024-01-15T10:30:00.1234", "2024-01-15T10:30:00.12345", "2024-01-15T10:30:00.1234567",
    "2024-01-15T10:30:00,5", "2024-01-15T10:30:00.", "2024-01-15T10:30:00.123456789",
    "2024-01-15T10:30:00Z", "2024-01-15T10:30:00z", "2024-01-15T10:30:00+05", "2024-01-15T10:30:00+0530",
    "2024-01-15T10:30:00+05:30:15", "2024-01-15T10:30:00+05:30:15.25", "2024-01-15T10:30:00+053015",
    "2024-01-15T10:30:00+053015.5", "2024-01-15T10:30:00+24:00", "2024-01-15T10:30:00-24:00",
    "2024-01-15T10:30:00+23:59:59.999999", "2024-01-15T10:30:00+00:00:00.5", "2024-01-15T10:30:00+5",
    "2024-01-15T10:30:00+05:3", "2024-01-15T10:30:00+", "2024-01-15T10:30:00-",
    "2024-01-15T10:30:00+05:30x", "2024-01-15T24:00:00", "2024-01-15T23:60:00", "2024-01-15T23:59:60",
    "2024-02-29T00:00:00", "2023-02-29T00:00:00", "2024-13-01T00:00:00", "2024-00-10T00:00:00",
    "2024-01-00T00:00:00", "2024-01-32T00:00:00", "0000-01-01T00:00:00", "0001-01-01T00:00:00",
    "9999-12-31T23:59:59.999999", "2024-01-15 10:30:00", "2024-01-15_10:30:00", "2024-01-15x10:30:00",
    "2024-01-15\u00e910:30:00", "2024-01-15T10:30:00 ", " 2024-01-15T10:30:00", "2024-01-15T",
    "2024-01-1", "2024-1-15", "2024/01/15", "2024-01-15T10:30:00:5", "2024-01-15T10:3", "2024-01-15T1",
    "2024-01-15T10:30:0", "2024-01-15T10:30:00.12a", "2024-01-15T1030:00", "2024-01-15T10:3000",
    "2024-01-15T10.5", "2024-01-15T10:30.5", "2024-W01", "2024W01", "2024-W01-1", "2024W011",
    "2024-W53-1", "2020-W53-1", "2015-W53-7", "2024-W00-1", "2024-W01-0", "2024-W01-8", "2024-W011",
    "2024W01-1", "2024-W01T10:00", "2024W01T10:00", "2024-W01-1T10:00", "2024W011T10:00",
    "2024-W01-10:30", "2024-W01-", "2024-W", "2024-W1-1T10", "0000-W01-1T00:00", "9999-W52-5T00:00",
    "9999-W52-6T00:00", "2024003", "2024-003", "2024-003T10:00", "\u0662\u0660\u0662\u0664-01-15T10:30:00",
    "2024-01-15T\u0661\u0660:30:00", "2024-01-15T10:30:00+05:30\u0660", "2024-01-15T10:30:00.\u0661\u0662",
    "2024-01-15T10:30:00Z+05:00", "2024-01-15T10:30:00+00:00Z", "2024-01-15T10:30:00-05:00+01:00",
    "2024-01-15T10:30:00+05:00-01:00", "2024-01-15T10:30-05:00", "2024-01-15T10-05", "2024-01-15T10Z",
    "2024-01-15T+05:00", "2024-01-15TZ", "2024-01-15T-", "1234567", "123456", "abcdefg", "2024-01-15T\x00",
    "2024-01-15T10:30:00\n", "2024-01-15T10:30:00.000000", "2024-01-15T00:00:00.000001",
    "2024-01-15T10:30:00.9999995", "2024-01-15T10:30:00,123456",
]


def iso_fields(dt):
    off = dt.utcoffset()
    off_us = None if off is None else (off.days * 86400 + off.seconds) * 1_000_000 + off.microseconds
    return [dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second, dt.microsecond, off_us]


def iso_corpus():
    rnd = random.Random(20240115)
    corpus = list(ISO_TARGETED)
    alphabet = "0123456789-:T+Z.,W 0123456789"
    for _ in range(1500):
        corpus.append("".join(rnd.choice(alphabet) for _ in range(rnd.randint(7, 28))))
    seeds = [s for s in ISO_TARGETED if s.isascii()]
    for _ in range(2500):
        s = list(rnd.choice(seeds))
        for _ in range(rnd.randint(1, 3)):
            op = rnd.random()
            pos = rnd.randrange(len(s) + 1)
            ch = rnd.choice(alphabet)
            if op < 0.33 and s:
                del s[min(pos, len(s) - 1)]
            elif op < 0.66:
                s.insert(pos, ch)
            elif s:
                s[min(pos, len(s) - 1)] = ch
        corpus.append("".join(s))
    out = []
    seen = set()
    for s in corpus:
        if s in seen:
            continue
        seen.add(s)
        try:
            out.append({"s": s, "ok": iso_fields(datetime.fromisoformat(s))})
        except ValueError as e:
            out.append({"s": s, "error": str(e)})
    return out


OUT["fromisoformat"] = iso_corpus()


def entry_result(**kw):
    try:
        e = ChangeLogEntry(**kw)
        return {"ok": {"timestamp": e.timestamp, "author": e.author, "description": e.description,
                       "change_id": e.change_id, "related_changes": e.related_changes}}
    except Exception as e:  # noqa: BLE001
        return err(e)


OUT["changelog_timestamps"] = [
    {"timestamp": ts, "result": entry_result(timestamp=ts, author="a@b.co", description="d")}
    for ts in ISO_TARGETED + ["2024-01-15T10:30:00ZZ", "2024-01-15TZ10:30:00", "2024-01-15T10:30:00.5Z",
                              "Z2024-01-15T10:30:00", "2024-01-15T10:30:00+05:30Z", "T", "2024-01-15t10:30:00"]
]

EMAILS = [
    "user@domain.com", "user.name@domain.co.uk", "user+tag@domain.org", "user123@domain123.net",
    "user_name@sub.domain.com", "plainaddress", "@missingdomain.com", "missing@.com", "missing@domain",
    "spaces @domain.com", "double@@domain.com", "a@b.co", "a@b.c", "a@b.c1", "a@b-c.com", "a%b@c.de",
    "a@b..com", "a@.b.com", ".a@b.com", "a.@b.com", "a@b.com.", "A@B.COM", "a@b.com\n", "a@b.com\n\n",
    "\na@b.com", "a@b.com ", " a@b.com", "a@b.com\t", "\u00e9@b.com", "a@b.c\u00f3m", "a@b.\u0441om",
    "a@b.comm", "a@b.c.d.e.fg", "a@123.45", "a@b.123", "", "@", "a@", "a@b", "a@b.", "a+b-c_d.e%f@g-h.i.jk",
    "x@y.zz", "first.last@sub-domain.example-host.org",
]
OUT["changelog_emails"] = [
    {"author": e, "result": entry_result(timestamp="2024-01-15T10:30:00Z", author=e, description="d")}
    for e in EMAILS
]

DESCRIPTIONS = [
    "", " ", "   ", "\t\n", "\u00a0", "\u3000", "\x1c\x1d\x1e\x1f", "\u200b", "\x85", "\u2028",
    "x", " x ", "x" * 500, "x" * 501, "\u00e9" * 500, "\u00e9" * 501, "\U0001F389" * 500,
    "\U0001F389" * 501, "a\x00b", "\ufeff",
]
OUT["changelog_descriptions"] = [
    {"description": d, "result": entry_result(timestamp="2024-01-15T10:30:00Z", author="a@b.co", description=d)}
    for d in DESCRIPTIONS
]

OUT["changelog_optional"] = entry_result(
    timestamp="2024-01-15T10:30:00Z", author="a@b.co", description="d", change_id="C-1",
    related_changes=["C-0", "C-00"])


def create_now_shape():
    e = ChangeLogEntry.create_now("charlie@company.com", "now", change_id="X", related_changes=None)
    return {"has_T": "T" in e.timestamp, "change_id": e.change_id, "related_changes": e.related_changes}


OUT["changelog_create_now"] = create_now_shape()

# ---------------------------------------------------------------------------------------------
# MutationRecord / Severity / ChangeCategory / ImpactReport / ChangeLogAnalyzer
# ---------------------------------------------------------------------------------------------

mr = MutationRecord(timestamp="2024-01-01T00:00:00", operation="ADD_NODE", entity_id="n1", payload={"a": 1})
OUT["mutation_record"] = {"timestamp": mr.timestamp, "operation": mr.operation, "entity_id": mr.entity_id,
                          "payload": mr.payload, "version_label": mr.version_label}
OUT["enums"] = {
    "severity": [[m.name, m.value, str(m)] for m in Severity],
    "category": [[m.name, m.value, str(m)] for m in ChangeCategory],
}
report = ImpactReport(timestamp="T0", summary={"total_changes": 1}, breaking_changes=[{"a": 1}],
                      potentially_breaking=[{"b": 2}], safe_changes=[{"c": 3}], recommendations=["r"])
OUT["impact_report_to_dict"] = report.to_dict()
OUT["impact_report_default"] = mask(ImpactReport().to_dict())

ANALYZER_DIFFS = [
    {},
    {"removed_classes": [{"uri": "http://ex.org/Person"}]},
    {"changed_properties": [{"uri": "http://ex.org/worksFor",
                             "changes": {"domain": {"old": ["Person", "Organization"], "new": ["Person"]}}}]},
    {"added_classes": [{"uri": "http://ex.org/NewClass"}],
     "changed_properties": [{"uri": "http://ex.org/name", "changes": {"label": {"old": "Name", "new": "Full Name"}}}]},
    {"changed_properties": [{"uri": "http://ex.org/worksFor", "changes": {"domain": {"old": None, "new": ["Person"]}}}]},
    {"changed_classes": [{"name": "Person", "changes": {"subclasses": {"old": [], "new": ["Student"]}}}]},
    {"changed_classes": [{"changes": {"range": {"old": "xsd:string", "new": "xsd:string"}}}]},
    {"changed_classes": [{"uri": "C", "changes": {"range": {"old": ["a", "b"], "new": ["a", "b", "c"]}}}]},
    {"changed_classes": [{"uri": "C", "changes": {"cardinality": {"old": [1, 2], "new": [1.0]}}}]},
    {"changed_classes": [{"uri": "C", "changes": {"max_cardinality": {"old": 2, "new": [2]}}}]},
    {"changed_classes": [{"uri": "C", "changes": {"domain": {"old": "A"}}}]},
    {"changed_classes": [{"uri": "C", "changes": {"equivalent_to": {}, "label": {"old": 1, "new": 2}}}]},
    {"changed_classes": [{"uri": None, "changes": {}}], "removed_properties": [{"name": "p"}],
     "added_properties": [{}], "removed_classes": [{"uri": 5}]},
    {"unrelated": [1, 2]},
    {"removed_classes": None},
    {"removed_classes": ["http://ex.org/A"]},
    {"removed_classes": "abc"},
    {"changed_classes": [{"uri": "C", "changes": [["domain", {}]]}]},
    {"changed_classes": [{"uri": "C", "changes": {"domain": "x"}}]},
    {"changed_classes": [{"uri": "C", "changes": {"domain": {"old": [{"a": 1}], "new": ["x"]}}}]},
    {"changed_classes": [{"uri": "C", "changes": {"domain": {"old": [[1]], "new": ["x"]}}}]},
    {"added_classes": [{"uri": "A"}], "changed_classes": [{"uri": "B", "changes": {"disjoint_with": 1}}],
     "removed_properties": [{"uri": "P"}]},
]
OUT["change_reports"] = []
for d in ANALYZER_DIFFS:
    res = run(generate_change_report, d)
    if "ok" in res:
        res["ok"] = mask(res["ok"])
    OUT["change_reports"].append({"diff": d, "result": res})
OUT["analyzer_constants"] = {
    "validity": sorted(ChangeLogAnalyzer.VALIDITY_CONSTRAINTS),
    "structural": sorted(ChangeLogAnalyzer.STRUCTURAL_FIELDS),
}

# ---------------------------------------------------------------------------------------------
# json.dumps / checksums
# ---------------------------------------------------------------------------------------------


def random_floats():
    rnd = random.Random(7)
    floats = [0.0, -0.0, 0.1, 0.2, 0.3, 1.0, -1.0, 1.5, 100.0, 1e15, 1e16, 1e17, 9.999999999999998e15,
              1e-4, 9.999999999999999e-05, 1e-5, 1e-7, 0.00012345, 123456789.123456789, 5e-324,
              1.7976931348623157e308, 2.2250738585072014e-308, 1e22, 1e21, 1e-320, 3.14159, -2.5e-8,
              12345678901234567.0, 0.5, 2.0 ** 53, 2.0 ** 53 + 2, -(2.0 ** 63), 1 / 3, 2 / 3, 10000.0,
              40000.0, 0.75, 0.8 + 19 * 0.01, 999 * 0.1, 4999 * 0.01]
    while len(floats) < 600:
        bits = rnd.getrandbits(64)
        f = struct.unpack("<d", struct.pack("<Q", bits))[0]
        if f == f and f not in (float("inf"), float("-inf")):
            floats.append(f)
    for _ in range(200):
        floats.append(rnd.uniform(-1e6, 1e6))
        floats.append(rnd.random() * 10 ** rnd.randint(-8, 20))
    return floats


FLOATS = random_floats()
STRINGS = ["", "plain", "quote\" back\\slash", "\n\r\t\b\f", "\x00\x01\x1f\x7f", "\u00e9\u4e2d\u6587",
           "\U0001F389\U0001F680", "\u2028\u2029", "\ud7ff\ue000\uffff", "/slash", "<>&'",
           "\U00010000\U0010FFFF"]
OUT["json_floats"] = {"floats": FLOATS, "dumps": json.dumps(FLOATS),
                      "checksum": compute_checksum({"v": FLOATS})}
OUT["json_strings"] = {"strings": STRINGS, "dumps": json.dumps(STRINGS), "dumps_map": json.dumps({s: i for i, s in enumerate(STRINGS)}),
                       "checksum": compute_checksum({s: i for i, s in enumerate(STRINGS)})}

CHECKSUM_PAYLOADS = [
    {},
    {"b": 2, "a": 1},
    {"entities": [{"id": "1", "name": "Entity1"}], "relationships": [{"source": "1", "target": "2"}],
     "metadata": {"version": "1.0"}},
    {"ints": [0, -1, 1, 2 ** 31, -(2 ** 63), 2 ** 63 - 1, 2 ** 64 - 1], "bools": [True, False], "none": None},
    {"nested": {"z": {"y": {"x": [1, {"b": 2, "a": [3, {"d": 4, "c": 5}]}]}}}, "empty": [[], {}, ""]},
    {"unicode": "Test with \u4e2d\u6587\u5b57\u7b26", "emoji": "\U0001F389\U0001F680\U0001F4BB",
     "special": "Special chars: @#$%^&*()", "quotes": 'Single "double" quotes', "ctrl": "a\x00b\x7fc"},
    {"\u00e9": 1, "e": 2, "E": 3, "\u00c9": 4, "_": 5, "a b": 6, "": 7, "\U0001F389": 8, "\uffff": 9},
    {"floats": [10000.0, 50000.0, 0.75, 1e16, 1e-5, -0.0, 0.1 + 0.2]},
]
OUT["checksums"] = [{"data": p, "checksum": compute_checksum(p), "dumps": json.dumps(p)} for p in CHECKSUM_PAYLOADS]

VERIFY_CASES = [
    {"entities": [{"id": "1"}], "metadata": {}},
    {"entities": [{"id": "1"}], "metadata": {}, "checksum": "invalid_checksum"},
    {"a": 1, "checksum": ""},
    {"a": 1, "checksum": None},
    {"a": 1, "checksum": 0},
    {"a": 1, "checksum": 5},
    {"a": 1, "checksum": ["x"]},
]
with_valid = {"entities": [{"id": "1"}], "metadata": {}}
with_valid["checksum"] = compute_checksum(dict(with_valid))
VERIFY_CASES.append(with_valid)
OUT["verify_checksum"] = [{"snapshot": s, "valid": verify_checksum(s)} for s in VERIFY_CASES]

rec = create_graph_snapshot_record("v-1", "http://ex.org/g/1", metadata={"pipeline": "p"})
rec_default = create_graph_snapshot_record("v-2", "http://ex.org/g/2")
OUT["graph_snapshot_record"] = {
    "custom": {"keys": list(rec.keys()), "masked": mask(rec, ("timestamp", "checksum")), "valid": verify_checksum(rec)},
    "default": {"keys": list(rec_default.keys()), "masked": mask(rec_default, ("timestamp", "checksum")),
                "valid": verify_checksum(rec_default)},
    "falsy_metadata": mask(create_graph_snapshot_record("v-3", "g", "me", "desc", {}), ("timestamp", "checksum")),
}

# ---------------------------------------------------------------------------------------------
# Storage backends
# ---------------------------------------------------------------------------------------------

LIST_SNAPSHOTS = [
    {"label": "legacy", "timestamp": "2024-01-01T00:00:00", "author": "a@b.co", "description": "d", "checksum": "c",
     "entities": [{"id": 1}, {"id": 2}], "relationships": [{"source": 1, "target": 2}]},
    {"label": "nodes", "timestamp": "2024-01-02T00:00:00", "nodes": [{"id": 1}], "edges": [], "graph_uri": "urn:g"},
    {"label": "mixed", "timestamp": "2024-01-03T00:00:00", "entities": None, "nodes": [1, 2, 3],
     "relationships": [], "edges": [1], "version_id": "vid-3"},
    {"label": "strings", "timestamp": "2024-01-04T00:00:00", "entities": "abcd", "relationships": {"x": 1, "y": 2}},
    {"label": "bare", "timestamp": "2024-01-05T00:00:00"},
    {"label": "nulls", "timestamp": "2024-01-06T00:00:00", "entities": 0, "nodes": [9], "relationships": "",
     "edges": [1, 2], "version_id": None},
]


def storage_scenarios(storage):
    out = {}
    for s in LIST_SNAPSHOTS:
        storage.save(s)
    out["list_all"] = storage.list_all()
    out["get_strings"] = storage.get("strings")
    out["get_missing"] = storage.get("missing")
    out["exists"] = [storage.exists("legacy"), storage.exists("nope")]
    out["errors"] = {
        "no_label": run(storage.save, {"timestamp": "x"}),
        "empty_label": run(storage.save, {"label": ""}),
        "zero_label": run(storage.save, {"label": 0}),
        "duplicate": run(storage.save, {"label": "legacy"}),
        "list_label": run(storage.save, {"label": ["l"]}),
    }
    out["delete"] = [storage.delete("bare"), storage.delete("bare")]
    out["after_delete"] = [m["label"] for m in storage.list_all()]
    storage.save_tag("prod", "legacy")
    storage.save_tag("dev", "nodes")
    storage.save_tag("prod", "mixed")
    out["tags"] = {"list": storage.list_tags(), "get": [storage.get_tag("prod"), storage.get_tag("nope")]}
    storage.save_mutation({"timestamp": "t1", "operation": "ADD_NODE", "entity_id": "n1", "payload": {"a": 1}})
    storage.save_mutation({"timestamp": "t2", "operation": "UPDATE_NODE", "entity_id": "n1", "payload": {"a": 2},
                           "version_label": "pre"})
    storage.save_mutation({"timestamp": "t3", "operation": "ADD_NODE", "entity_id": "n2"})
    out["history_before"] = snap(storage.get_entity_history("n1"))
    storage.assign_version_to_unlabeled_mutations("v1")
    storage.save_mutation({"timestamp": "t4", "operation": "REMOVE_NODE", "entity_id": "n1", "payload": {}})
    out["history_after"] = snap(storage.get_entity_history("n1"))
    out["history_n2"] = snap(storage.get_entity_history("n2"))
    out["history_none"] = storage.get_entity_history("zzz")
    return out


OUT["in_memory_storage"] = storage_scenarios(InMemoryVersionStorage())
sqlite_storage = SQLiteVersionStorage(db("scenarios.db"))
OUT["sqlite_storage"] = storage_scenarios(sqlite_storage)
sqlite_storage.save({"label": "typed", "timestamp": True, "author": 5, "description": 1.5, "checksum": None or "c"})
OUT["sqlite_storage"]["typed_list"] = [m for m in sqlite_storage.list_all() if m["label"] == "typed"]
OUT["sqlite_storage"]["binding_errors"] = {
    "dict_timestamp": run(sqlite_storage.save, {"label": "b1", "timestamp": {"x": 1}}),
    "list_author": run(sqlite_storage.save, {"label": "b2", "author": [1]}),
    "null_author": run(sqlite_storage.save, {"label": "b3", "author": None}),
    "big_int": run(sqlite_storage.save, {"label": "b4", "timestamp": 2 ** 70}),
    "dict_mutation": run(sqlite_storage.save_mutation, {"timestamp": {"a": 1}}),
}
sqlite_storage.save_mutation({"entity_id": 5, "payload": None, "operation": 7, "timestamp": 1.5})
OUT["sqlite_storage"]["numeric_mutation"] = sqlite_storage.get_entity_history("5")
conn = sqlite3.connect(db("scenarios.db"))
OUT["sqlite_schema"] = [list(r) for r in conn.execute("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY name")]
OUT["sqlite_rows"] = [list(r) for r in conn.execute(
    "SELECT label, timestamp, author, description, checksum, snapshot_data FROM versions ORDER BY label")]
OUT["sqlite_mutation_rows"] = [list(r) for r in conn.execute(
    "SELECT id, timestamp, operation, entity_id, payload, version_label FROM mutation_log ORDER BY id")]
conn.close()

# A database written by the original, to be read by the port (stored as SQL text).
compat = SQLiteVersionStorage(db("compat.db"))
compat_snapshot = {"label": "py-v1", "timestamp": "2024-03-01T12:00:00", "author": "py@ex.org", "description": "from python",
                   "entities": [{"id": "e\u00e9", "score": 0.1, "big": 2 ** 63 - 1}], "relationships": [], "metadata": {"k": [1, 2.5]}}
compat_snapshot["checksum"] = compute_checksum(compat_snapshot)
compat.save(compat_snapshot)
compat.save_tag("release", "py-v1")
compat.save_mutation({"timestamp": "2024-03-01T12:00:01", "operation": "ADD_NODE", "entity_id": "e\u00e9",
                      "payload": {"x": "\U0001F389"}, "version_label": None})
conn = sqlite3.connect(db("compat.db"))
OUT["compat_db_dump"] = list(conn.iterdump())
conn.close()
OUT["compat_expected"] = {"get": compat.get("py-v1"), "list_all": compat.list_all(), "tags": compat.list_tags(),
                          "history": compat.get_entity_history("e\u00e9")}

# ---------------------------------------------------------------------------------------------
# TemporalVersionManager
# ---------------------------------------------------------------------------------------------


class FakeStore:
    def __init__(self, fail=False):
        self.queries = []
        self.fail = fail

    def execute_query(self, query):
        self.queries.append(query)
        if self.fail:
            raise RuntimeError("store down")
        return {"ok": True}


class FakeGraph:
    def __init__(self):
        self.graph_id = "graph-1"
        self.mutation_callback = None
        self._suspend_mutation_callback = False
        self.loaded = []
        self.suspend_seen = []

    def from_dict(self, payload):
        self.suspend_seen.append(self._suspend_mutation_callback)
        self.loaded.append(payload)
        return self


COMPARE_PAIRS = [
    ({"entities": [{"id": "entity1", "name": "Entity 1", "type": "Person"},
                   {"id": "entity2", "name": "Entity 2", "type": "Organization"}],
      "relationships": [{"source": "entity1", "target": "entity2", "type": "works_for"}]},
     {"entities": [{"id": "entity1", "name": "Entity 1 Updated", "type": "Person"},
                   {"id": "entity3", "name": "Entity 3", "type": "Project"}],
      "relationships": [{"source": "entity1", "target": "entity3", "type": "manages"}]}),
    ({"entities": [{"id": "entity1"}], "relationships": []},
     {"nodes": [{"id": "entity1"}, {"id": "entity2"}], "edges": []}),
    ({"nodes": [{"name": "no-id-a"}, {"name": "no-id-b"}], "edges": [
        {"source": "a", "target": "b", "relationship": "rel", "weight": 1},
        {"source": "a", "target": "b", "type": None, "weight": 2},
        {"target": "b"}]},
     {"nodes": [{"name": "no-id-a"}, {"name": "changed"}, {"name": "new"}], "edges": [
         {"source": "a", "target": "b", "relationship": "rel", "weight": 1.0},
         {"source": "a", "target": "b", "type": None, "weight": 3, "extra": True},
         {"source": None, "target": "b"}]}),
    ({"entities": [{"id": 1, "v": 1}, {"id": "1", "v": 2}, {"id": 1.0, "v": 3}, {"id": True, "v": 4}],
      "relationships": [{"source": 1, "target": 2, "type": "t"}, {"source": "1", "target": "2", "type": "t", "w": 1}]},
     {"entities": [{"id": 1, "v": 1}, {"id": "1", "v": 5}, {"v": 6}],
      "relationships": [{"source": 1, "target": 2, "type": "t", "w": 2}]}),
    ({"entities": [{"id": "a", "x": [1, 2], "y": {"k": 1}}, {"id": "b", "x": 1}], "relationships": []},
     {"entities": [{"id": "a", "x": [1, 2.0], "y": {"k": 1.0}, "z": None}, {"id": "b", "x": True}],
      "relationships": []}),
    ({"entities": None, "relationships": None}, {"nodes": None, "edges": None, "entities": [{"id": "x"}]}),
]


def compare_result(m, a, b):
    res = run(m.compare_versions, a, b)
    return res


def temporal_scenarios():
    out = {}
    m = TemporalVersionManager()
    out["compare"] = []
    for i, (g1, g2) in enumerate(COMPARE_PAIRS):
        m.create_snapshot(g1, f"a{i}", "test@example.com", f"A {i}")
        m.create_snapshot(g2, f"b{i}", "test@example.com", f"B {i}")
        out["compare"].append({"g1": g1, "g2": g2, "labels": compare_result(m, f"a{i}", f"b{i}"),
                               "dicts": compare_result(m, g1, g2)})
    out["compare_errors"] = {
        "missing_first": run(m.compare_versions, "nope", "a0"),
        "missing_second": run(m.compare_versions, "a0", "nope"),
        "non_dict": run(m.compare_versions, ["x"], "a0"),
        "number": run(m.compare_versions, 5, "a0"),
        "no_schema": run(m.compare_versions, {"foo": []}, "a0"),
        "non_dict_entity": run(m.compare_versions, {"entities": ["s"]}, "a0"),
        "unhashable_id": run(m.compare_versions, {"entities": [{"id": [1]}]}, "a0"),
        "non_dict_relationship": run(m.compare_versions, {"entities": [], "relationships": [5]}, "a0"),
        "entities_number": run(m.compare_versions, {"entities": 5}, "a0"),
    }
    out["compare_unknown_label"] = run(m.compare_versions, {"entities": []}, {"nodes": [], "label": None})
    out["diff"] = run(m.diff, "a0", "b0")
    out["create_errors"] = {
        "author": run(m.create_snapshot, {"entities": []}, "e1", "invalid-email", "d"),
        "long": run(m.create_snapshot, {"entities": []}, "e2", "a@b.co", "x" * 501),
        "blank": run(m.create_snapshot, {"entities": []}, "e3", "a@b.co", "  "),
        "non_dict": run(m.create_snapshot, [1], "e4", "a@b.co", "d"),
        "no_schema": run(m.create_snapshot, {"foo": []}, "e5", "a@b.co", "d"),
        "author_before_graph": run(m.create_snapshot, [1], "e6", "bad", "d"),
        "empty_label": run(m.create_snapshot, {"entities": []}, "", "a@b.co", "d"),
        "duplicate": run(m.create_snapshot, {"entities": []}, "a0", "a@b.co", "d"),
        "entities_string": run(lambda: mask(m.create_snapshot({"entities": "ab"}, "e7", "a@b.co", "d"), ("timestamp", "checksum"))),
    }
    snap = m.create_snapshot({"edges": [{"source": "x"}]}, "meta", "a@b.co", "with metadata", metadata={"k": [1]})
    out["create_shape"] = {"keys": list(snap.keys()), "masked": mask(snap, ("timestamp", "checksum")),
                           "valid": verify_checksum(snap)}
    snap_none = m.create_snapshot({"nodes": []}, "meta-none", "a@b.co", "metadata None", metadata=None)
    out["create_metadata_none"] = snap_none["metadata"]

    # tags, mutations, history
    t = TemporalVersionManager()
    t.create_snapshot({"entities": []}, "v1", "a@b.co", "d")
    out["tag_errors"] = run(t.tag_version, "v99", "x")
    t.tag_version("v1", "production-ready")
    t.tag_version("v1", "beta")
    out["tags"] = t.list_tags()
    t.record_mutation("ADD_NODE", "n1", {"properties": {"age": 30}})
    t.record_mutation("UPDATE_NODE", "n1", {"properties": {"age": 31}}, version_label="pinned")
    t.create_snapshot({"entities": []}, "v2", "a@b.co", "d")
    t.record_mutation("REMOVE_NODE", "n1", {})
    out["node_history"] = mask(t.get_node_history("n1"))
    out["node_history_other"] = t.get_node_history("other")

    # attach_to_graph and restore_snapshot with a fake graph
    g = FakeGraph()
    t.attach_to_graph(g)
    out["attached_callback_is_record_mutation"] = g.mutation_callback == t.record_mutation
    g.mutation_callback("ADD_EDGE", "e1", {"w": 1})
    out["callback_history"] = mask(t.get_node_history("e1"))
    out["restore_errors"] = {
        "protected": run(t.restore_snapshot, g, "v1"),
        "protected_explicit": run(t.restore_snapshot, g, "v1", require_confirmation=True),
        "missing": run(t.restore_snapshot, g, "nope", require_confirmation=False),
    }
    t.create_snapshot({"nodes": [{"id": "r1"}], "edges": [{"source": "r1", "target": "r1"}]}, "restore-me",
                      "a@b.co", "restore point")
    g._suspend_mutation_callback = False
    out["restore"] = {"result": t.restore_snapshot(g, "restore-me", require_confirmation=False),
                      "loaded": g.loaded, "suspend_seen": g.suspend_seen,
                      "suspend_after": g._suspend_mutation_callback}
    g2 = FakeGraph()
    g2._suspend_mutation_callback = True
    t.restore_snapshot(g2, "restore-me", require_confirmation=False)
    out["restore_keeps_previous_flag"] = g2._suspend_mutation_callback

    # prune_versions
    def prune_case(snapshots, keep, store_fail=None):
        p = TemporalVersionManager()
        for s in snapshots:
            p.storage.save(s)
        store = None if store_fail is None else FakeStore(store_fail)
        if keep is None:
            res = run(p.prune_versions, triplet_store=store)
        else:
            res = run(p.prune_versions, keep_last_n=keep, triplet_store=store)
        return {"result": res, "queries": None if store is None else store.queries,
                "remaining": [v["label"] for v in p.list_versions()]}

    snaps = [{"label": f"s{i}", "timestamp": f"2024-01-0{i}T00:00:00", "graph_uri": f"http://ex.org/g/{i}"}
             for i in range(1, 8)]
    snaps[2]["graph_uri"] = "http://example.org/graph> } ; DROP ALL ; #"
    snaps[3]["graph_uri"] = "  <urn:x:y z>  "
    snaps[4]["graph_uri"] = "http://ex.org/\u00e9 g?q=1&r=[2]#f%20"
    snaps[5]["graph_uri"] = ""
    del snaps[6]["graph_uri"]
    out["prune"] = {
        "default": prune_case(snaps, None, False),
        "keep_1": prune_case(snaps, 1, False),
        "keep_0": prune_case(snaps, 0, False),
        "keep_negative": prune_case(snaps, -2, False),
        "keep_more": prune_case(snaps, 10, False),
        "no_store": prune_case(snaps, 2, None),
        "failing_store": prune_case(snaps, 5, True),
        "ties": prune_case([{"label": "t1", "timestamp": "x"}, {"label": "t2", "timestamp": "x"},
                            {"label": "t3", "timestamp": "y"}], 1, False),
        "missing_timestamp": prune_case([{"label": "m1"}, {"label": "m2", "timestamp": "2024"}], 1, False),
        "single_missing_timestamp": prune_case([{"label": "m1"}], 0, False),
        "graph_uri_number": prune_case([{"label": "n1", "timestamp": "1", "graph_uri": 5},
                                        {"label": "n2", "timestamp": "2", "graph_uri": 0}], 0, False),
    }
    return out


OUT["temporal"] = temporal_scenarios()


def temporal_sqlite():
    path = db("temporal.db")
    m = TemporalVersionManager(storage_path=path)
    m.create_snapshot({"entities": [{"id": "x"}]}, "s1", "a@b.co", "one")
    m.record_mutation("ADD_NODE", "x", {"p": 1})
    m.tag_version("s1", "t")
    m2 = TemporalVersionManager(storage_path=path)
    return {"versions": [v["label"] for v in m2.list_versions()], "tags": m2.list_tags(),
            "history": mask(m2.get_node_history("x")), "valid": m2.verify_checksum(m2.get_version("s1"))}


OUT["temporal_sqlite"] = temporal_sqlite()

# ---------------------------------------------------------------------------------------------
# OntologyVersionManager
# ---------------------------------------------------------------------------------------------


def ontology_scenarios():
    out = {}
    m = OntologyVersionManager()
    out["initial_versions"] = snap(m.versions)
    o1 = {"uri": "https://example.com/ontology", "version_info": {"version": "1.0"},
          "structure": {"classes": ["Person", "Organization", "Person"], "properties": ["name", "email"],
                        "individuals": ["john_doe"], "axioms": ["Person hasName exactly 1 string"]}}
    o2 = {"uri": "https://example.com/ontology/v2", "version_info": {"version": "2.0"},
          "structure": {"classes": ["Person", "Project"], "properties": ["name", "email", "description"],
                        "individuals": ["john_doe", "acme_corp"], "axioms": ["Project hasDescription some string"]}}
    s1 = m.create_snapshot(o1, "v1.0", "test@example.com", "Version 1")
    m.create_snapshot(o2, "v2.0", "test@example.com", "Version 2", metadata={"reviewed": True})
    out["snapshot"] = {"keys": list(s1.keys()), "masked": mask(s1, ("timestamp", "checksum")), "valid": verify_checksum(s1)}
    out["versions_keys"] = list(m.versions.keys())
    out["compare"] = m.compare_versions("v1.0", "v2.0")
    out["compare_same"] = m.compare_versions("v1.0", "v1.0")
    m.create_snapshot({}, "empty", "test@example.com", "Empty ontology")
    out["empty_snapshot"] = mask(m.get_version("empty"), ("timestamp", "checksum"))
    out["compare_empty"] = m.compare_versions("empty", "v1.0")
    m.create_snapshot({"structure": {"classes": "AB", "properties": None}}, "odd", "test@example.com", "Odd")
    m.create_snapshot({"structure": {"classes": [["x"]]}}, "unhashable", "test@example.com", "Odd")
    m.create_snapshot({"structure": None}, "nostructure", "test@example.com", "Odd")
    m.create_snapshot({"structure": {"classes": [1, 1.0, True, "1"]}}, "numbers", "test@example.com", "Numbers")
    out["errors"] = {
        "missing": run(m.compare_versions, "nonexistent1", "nonexistent2"),
        "missing_second": run(m.compare_versions, "v1.0", "nonexistent2"),
        "none_section": run(m.compare_versions, "odd", "v1.0"),
        "string_section": run(m.compare_versions, "v1.0", "odd"),
        "unhashable": run(m.compare_versions, "unhashable", "v1.0"),
        "structure_none": run(m.compare_versions, "nostructure", "v1.0"),
        "author": run(m.create_snapshot, o1, "x", "bad", "d"),
        "duplicate": run(m.create_snapshot, o1, "v1.0", "a@b.co", "d"),
    }
    out["numbers_vs_v1"] = m.compare_versions("numbers", "v1.0")
    path = db("ontology.db")
    p1 = OntologyVersionManager(storage_path=path)
    p1.create_snapshot(o1, "persistent_v1.0", "test@example.com", "Persistent test")
    p2 = OntologyVersionManager(storage_path=path)
    got = p2.get_version("persistent_v1.0")
    out["persistence"] = {"label": got["label"], "ontology_iri": got["ontology_iri"], "versions": snap(p2.versions)}
    return out


OUT["ontology_manager"] = ontology_scenarios()

# ---------------------------------------------------------------------------------------------
# NamespaceManager
# ---------------------------------------------------------------------------------------------

tracker_off()

NAMES = ["Person", "person", "person record", "has_name", "hasName", "HTTPServer", "  spaced  out  ",
         "snake_case_name", "kebab-case-name", "CamelCase", "already camelCase", "x", "X", "123abc",
         "a1b2", "", "!!!", "\u00e9l\u00e8ve", "na\u00efve name", "\u00e9Name", "aB", "a B", "a_B", "a-B",
         "has.Value", "\u00aaBc", "has\u00c9t\u00e9", "\u03b1\u0392", "Ab", "isPartOf", "is part of",
         "ALL CAPS", "mixed_Case-words here", "tab\tseparated", "dots.and.more", "a/b", "q?x"]
CONFIGS = [
    {},
    {"base_uri": "https://example.org/ontology/", "version": "1.0"},
    {"base_uri": "https://example.org/ontology", "version": "2.0"},
    {"base_uri": "https://example.org/ontology/", "version": "2.1", "use_speaking_iris": False},
    {"base_uri": "https://example.org/ont/v2.0/", "version": "2.0"},
    {"base_uri": "https://example.org/onto#", "version": "1.0"},
    {"base_uri": "https://example.org", "version": None},
    {"base_uri": "https://example.org/a/b", "version": ""},
    {"base_uri": "ontology/", "version": "3"},
    {"base_uri": "", "version": "1.0"},
    {"base_uri": "urn:example:ont", "version": "1.0"},
    {"base_uri": "urn:example:ont", "version": "2"},
    {"base_uri": "https://example.org/x/", "version": 2.0},
    {"base_uri": "https://example.org/x/", "version": 1.0},
    {"base_uri": "https://example.org/x/", "version": "2.0:beta"},
    {"base_uri": "https://example.org/x/", "version": "../up"},
    {"base_uri": "https://example.org/x/", "use_speaking_iris": 0},
    {"base_uri": "https://example.org/x/", "use_speaking_iris": "yes"},
    {"base_uri": "http://[::1]:8080/onto/", "version": "4"},
    {"base_uri": "http://[::1/onto/", "version": "4"},
    {"base_uri": "http://[::1/onto/", "version": "1.0"},
    {"base_uri": "file:///tmp/onto/", "version": "5"},
    {"base_uri": "https://example.org/onto/?q=1#frag", "version": "6"},
]


def ns_case(cfg):
    nm = NamespaceManager(**cfg)
    out = {"config": cfg, "base_uri_attr": nm.base_uri, "version_attr": nm.version,
           "use_speaking_iris_attr": nm.use_speaking_iris, "get_base_uri": run(nm.get_base_uri)}
    out["class"] = [run(nm.generate_class_iri, n) for n in NAMES]
    out["property"] = [run(nm.generate_property_iri, n) for n in NAMES]
    out["individual"] = [run(nm.generate_individual_iri, n) for n in NAMES]
    out["scheme"] = [run(nm.build_concept_scheme_uri, n) for n in
                     ["My Vocabulary", "  --Weird__Name!!  ", "", "\u00e9t\u00e9 2024", "A-B-C"]]
    return out


OUT["namespace"] = {"names": NAMES, "cases": [ns_case(c) for c in CONFIGS]}
nm = NamespaceManager()
OUT["namespace_misc"] = {
    "namespaces": nm.get_all_namespaces(),
    "get": [nm.get_namespace("owl"), nm.get_namespace("nope")],
    "skos": nm.get_skos_uri("Concept"),
    "alignment": nm.get_alignment_predicates(),
}
nm.register_namespace("ex", "https://example.org/")
nm.register_namespace("owl", "http://other/owl#")
OUT["namespace_misc"]["after_register"] = nm.get_all_namespaces()
OUT["namespace_misc"]["alignment_after"] = nm.get_alignment_predicates()
del nm.namespaces["skos"]
del nm.namespaces["owl"]
OUT["namespace_misc"]["skos_missing"] = run(nm.get_skos_uri, "Concept")
OUT["namespace_misc"]["alignment_missing"] = nm.get_alignment_predicates()
OUT["namespace_misc"]["kwargs_config"] = NamespaceManager({"base_uri": "a"}, version="9", extra=1).config

IRIS = [
    "https://example.org/x", "http://", "ftp://a", "mailto:x@y", "urn:x", "example.org", "//example.org/x",
    "http://[::1]/", "http://[::1", "http://::1]/", "http://[1.2.3.4]/", "http://[v1.fe]/", "http://[vz.x]/",
    "http://[v1.]/", "http://[fe80::1%eth0]/", "http://[fe80::1%]/", "http://[::ffff:1.2.3.4]/",
    "http://[1:2:3:4:5:6:7::]/", "http://[1::2::3]/", "http://[::]/", "http://[:::]/", "http://[1:2:3:4:5:6:7:8]/",
    "http://[1:2:3:4:5:6:7:8:9]/", "http://[12345::]/", "http://[::1.2.3.04]/", "http://[::1.2.3]/",
    "http://x[::1]/", "http://[::1]x/", "http://[::1]:80/", "http://user@[::1]/", "http://[a]@host/",
    "http://ex\u2100ample.com/", "http://\uff21.com/", "http://\u00e9.com/", " https://x.org", "\thttps://x.org",
    "h\ttps://x.org", "HTTPS://X.ORG", "1http://x", "a+b-c.d://x", "x:y", "http://user:pw@host:80/p;params?q=1#frag",
    "http:///path", "", "https://", "https://x.org:abc/", "javascript:alert(1)", "http:x", "//", "///x",
    "https://[::1]?x", "https://[::1]#y", "https://host/[x]", "https://exa mple.org/", "\x00http://x.org",
    "http://\u00c5ngstr\u00f6m.org", "http://\u2474.com/", "http://a\uff0fb.com/",
]
OUT["validate_iri"] = [{"iri": i, "valid": NamespaceManager().validate_iri(i)} for i in IRIS]
tracker_off()


def parse_tuple(u):
    p = urlparse(u)
    return [p.scheme, p.netloc, p.path, p.params, p.query, p.fragment]


OUT["urlparse"] = [{"url": i, "result": run(parse_tuple, i)} for i in IRIS + [
    "http://a/b/c/d;p?q", "http://a/b;x/c;y", "ftp://a/b;c", "mailto:a;b", "sip:a;b", "tel:+1;ext=2",
    "http://a/b/c;", "http://a;b/c", "file:///tmp/x;y"]]

JOIN_BASES = ["https://semantica.dev/ontology/", "https://semantica.dev/ontology", "http://a/b/c/d;p?q", "",
              "ontology/", "/abs/path/", "https://ex.org", "file:///tmp/x/", "urn:isbn:123", "mailto:x@y",
              "https://ex.org/a/b/../c/", "https://user@[::1]:8080/p/", "HTTPS://Ex.org/A/", "https://ex.org/a?q#f",
              "https://ex.org/a/", "svn+ssh://h/r/", "ws://h/a/b", "tel:123", "//host/a/b"]
JOIN_URLS = ["Person", "v2.0/", "class/abc12345", "individual/x", "vocab/my-vocab", "../x", "./x", "/x",
             "//host/x", "?q", "#f", "", ".", "..", "../..", "../../../g", "v2.0:beta/", "g;x", "g?y#s", ";x",
             "a/./b/../c", "http:g", "https:x", "mailto:z", "g/", "/./g", "/../g", "g.", ".g", "g..", "..g",
             "./../g", "./g/.", "g/./h", "g/../h", "g;x=1/./y", "g;x=1/../y", "g?y/./x", "g#s/../x",
             "HTTP://other/x", "https://[::1/x", "x//y", "//", "a\tb", " x"]
OUT["urljoin"] = [{"base": b, "url": u, "result": run(urljoin, b, u)} for b in JOIN_BASES for u in JOIN_URLS]

QUOTE_INPUTS = ["", "http://example.org/graph> } ; DROP ALL ; #", "urn:x:y z", "\u00e9\u4e2d\U0001F389",
                "a%20b", "~-._", "!$&'()*+,;=:@/?[]", "<>\"{}|\\^`", "\x00\x1f\x7f", "AZaz09"]
OUT["quote"] = [{"s": s, "result": quote(s, safe="/:?&=@[]!$'()*+,%-._~")} for s in QUOTE_INPUTS]

# ---------------------------------------------------------------------------------------------
# VersionManager / OntologyVersion
# ---------------------------------------------------------------------------------------------


def version_manager_scenarios():
    out = {}
    vm = VersionManager(base_uri="https://example.org/ontology/")
    tracker_off()
    onto = {"name": "Test"}
    v1 = vm.create_version("1.0", onto, changes=["Added Person class"], metadata={
        "structure": {"classes": ["Person", "Org"], "properties": ["name"], "individuals": [], "axioms": []},
        "imports": ["http://ex.org/dep/v1.0", "http://ex.org/other"]})
    out["create_v1"] = {"record": mask(v1.__dict__), "ontology": onto}
    onto2 = {"uri": "https://example.org/custom/", "x": 1}
    v2 = vm.create_version("2.0", onto2, metadata={"structure": {"classes": ["Person", "Project"],
                                                                "properties": ["name", "email"],
                                                                "individuals": ["i1"], "axioms": ["ax"]}})
    out["create_v2"] = {"record": mask(v2.__dict__), "ontology": onto2}
    onto3 = {"uri": "https://example.org/custom/v3.0"}
    v3 = vm.create_version("3.0", onto3)
    out["create_v3"] = {"record": mask(v3.__dict__), "ontology": onto3}
    out["create_errors"] = {
        "uri_number": run(vm.create_version, "9.0", {"uri": 5}),
        "uri_list": run(vm.create_version, "9.1", {"uri": ["x"]}),
    }
    out["after_errors"] = {"versions": list(vm.versions.keys()), "latest": vm.latest_version}
    out["list_versions"] = vm.list_versions()
    out["get_version"] = [mask(vm.get_version("1.0").__dict__), vm.get_version("nope")]
    out["latest"] = mask(vm.get_latest_version().__dict__)
    out["compare"] = vm.compare_versions("1.0", "2.0")
    out["compare_3"] = vm.compare_versions("2.0", "3.0")
    out["compare_errors"] = [run(vm.compare_versions, "x", "1.0"), run(vm.compare_versions, "1.0", "y")]
    out["element_iris"] = [[n, t, run(vm.generate_element_iri, n, t)] for n in ["Person", "has name", "John Doe!"]
                           for t in ["class", "property", "individual", "other"]]
    out["element_iri_default"] = vm.generate_element_iri("Person")
    mig = {"version": "1.0", "uri": "x"}
    out["migrate"] = {"result": vm.migrate_ontology("1.0", "2.0", mig), "mutated": mig}
    out["migrate_errors"] = [run(vm.migrate_ontology, "x", "1.0", {}), run(vm.migrate_ontology, "1.0", "y", {})]
    out["imports"] = {
        "basic": sorted(map(str, vm.resolve_versioned_imports({"imports": ["http://ex.org/a/v1.0", "http://ex.org/b"]}))),
        "transitive": sorted(map(str, vm.resolve_versioned_imports({"imports": ["http://ex.org/x/v1.0/y", "http://ex.org/x/v1.0/y"]}))),
        "unknown_version": vm.resolve_versioned_imports({"imports": ["http://ex.org/v9.9"]}),
        "no_minor": vm.resolve_versioned_imports({"imports": ["http://ex.org/v1"]}),
        "none": vm.resolve_versioned_imports({}),
        "number": run(vm.resolve_versioned_imports, {"imports": [5]}),
        "unhashable": run(vm.resolve_versioned_imports, {"imports": [["x"]]}),
        "not_iterable": run(vm.resolve_versioned_imports, {"imports": None}),
    }
    out["diff"] = []
    diffs = [
        ({}, {}),
        ({"classes": [{"uri": "http://ex.org/C1", "domain": ["A", "B"]}]},
         {"classes": [{"uri": "http://ex.org/C1", "domain": ["B", "A"]}]}),
        ({"classes": [{"name": "Person", "label": "Human"}]}, {"classes": [{"name": "Person", "label": "Homo Sapiens"}]}),
        ({"classes": ["A", "B", {"uri": "C", "x": 1}, {"name": "D"}, {"x": "no key"}, 5, None],
          "properties": [{"uri": "p", "domain": [1, "1"], "range": "x"}],
          "individuals": [{"uri": "i", "types": ["T"]}], "axioms": [{"uri": "ax1", "expression": "e"}]},
         {"classes": ["B", {"uri": "C", "x": 2, "y": None}, {"name": "D", "uri": "D2"}, "E"],
          "properties": [{"uri": "p", "domain": ["1"], "range": ["x"]}],
          "individuals": [{"uri": "i", "types": ["T", "U"]}], "axioms": []}),
        ({"classes": [{"uri": "", "name": "N", "a": 1}, {"uri": "dup", "v": 1}, {"uri": "dup", "v": 2}]},
         {"classes": [{"name": "N", "a": 2}, {"uri": "dup", "v": 3}]}),
        ({"classes": [{"uri": 1, "v": 1}, {"uri": 1.0, "v": 2}]}, {"classes": [{"uri": True, "v": 3}]}),
        ({"classes": [{"uri": "x", "l": [1.0, True]}]}, {"classes": [{"uri": "x", "l": ["1.0", "True"]}]}),
        ({"classes": [{"uri": "x", "l": [1]}]}, {"classes": [{"uri": "x", "l": 1}]}),
        ({"classes": None}, {}),
        ({"classes": "ab"}, {"classes": "bc"}),
        ({"classes": [{"uri": ["u"]}]}, {}),
    ]
    for base, target in diffs:
        out["diff"].append({"base": base, "target": target, "result": run(vm.diff_ontologies, base, target)})
    return out


OUT["version_manager"] = version_manager_scenarios()
nm_custom = NamespaceManager(base_uri="https://custom.org/", version="7")
vm_custom = VersionManager(namespace_manager=nm_custom)
tracker_off()
OUT["version_manager_custom_ns"] = {
    "element": vm_custom.generate_element_iri("Thing"),
    "create": mask(vm_custom.create_version("8", {}).__dict__),
}
vm_cfg = VersionManager({"base_uri": "https://cfg.org/o"}, version="2.0")
tracker_off()
OUT["version_manager_config"] = {
    "config": {k: v for k, v in vm_cfg.config.items()},
    "ns_base": vm_cfg.namespace_manager.get_base_uri(),
    "element": vm_cfg.generate_element_iri("has value", "property"),
    "create": mask(vm_cfg.create_version("2.0", {}).__dict__),
}
OUT["ontology_version_dataclass"] = OntologyVersion("1", "iri", "1", "now").__dict__

shutil.rmtree(SCRATCH, ignore_errors=True)
with open(OUTPUT, "w", encoding="utf-8") as fh:
    json.dump(OUT, fh, ensure_ascii=False, indent=1, sort_keys=False)
    fh.write("\n")
print("wrote", OUTPUT)
