"""Generate tests/fixtures/oracle.json from the original Semantica (the oracle).

Every expected value used by the sem-utils characterisation tests that is not trivially
readable from the original source comes from running the original code here. Run from
var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python -I \
        <crate>/tests/fixtures/generate_oracle.py <crate>/tests/fixtures/oracle.json

The script only computes values (no network, no credentials, no writes outside the output file
and a scratch directory under the output's parent).
"""

import contextlib
import io
import json
import logging
import os
import sys
import tempfile
import types as pytypes
from datetime import datetime, timedelta, timezone

sys.path.insert(0, os.getcwd())

import yaml  # noqa: E402

import semantica.utils as U  # noqa: E402
from semantica.utils import exceptions as E  # noqa: E402
from semantica.utils import helpers as H  # noqa: E402
from semantica.utils import logging as L  # noqa: E402
from semantica.utils import progress_tracker as P  # noqa: E402
from semantica.utils import skos as S  # noqa: E402
from semantica.utils import types as T  # noqa: E402
from semantica.utils import validators as V  # noqa: E402

OUT = {}


def err(e):
    return {"error": type(e).__name__, "message": str(e)}


def run(f, *a, **k):
    try:
        return {"ok": f(*a, **k)}
    except Exception as e:  # noqa: BLE001
        return err(e)


# ---------------------------------------------------------------- python formatting
FLOATS = [0.1, 0.5, 1.0, -2.5, 1e16, 1e15, 1e-4, 1e-5, 123.456, 1.0 / 3.0, 2.0 ** 60,
          1.2345678901234568e17, 5e-324, 1.7976931348623157e308, 9007199254740993.0,
          -0.0, 100.0, 0.000123, 12345678.9]
OUT["float_repr"] = [[x, repr(x)] for x in FLOATS]
OUT["format_fixed"] = [[x, p, format(x, ".%df" % p)] for x in [0.125, 0.375, 2.5, 1.5, 2.675,
                                                                1.0005, -0.001, 1e16, 0.1]
                       for p in (0, 1, 2, 3, 4)]
REPR_VALUES = [None, True, False, 0, -5, 12345678901234567890, 1.5, 1e16, "plain",
               "it's", 'say "hi"', "both ' and \"", "tab\tnew\nline\r\\", "\x00\x1f\x7f",
               "café", " nbsp", "zero​width", "\U0001f600", "͸",
               [], {}, [1, "a", None, [2.5]], {"k": "v", "n": {"x": [True]}}]
OUT["py_repr"] = [[v, str(v), repr(v)] for v in REPR_VALUES]

# ---------------------------------------------------------------- json.dumps
JSON_VALUES = [
    {"b": [1, 2.5, {"x": None}], "a": "é \x7f\U0001f600"},
    [[], {}, [1, [2, {"z": []}]]],
    {"key": "value", "num": 1e-5, "big": 1e16, "neg": -0.0, "t": True},
    "control \x01\x08\x0c\n\r\t\"\\ chars",
    [12345678901234567890, -9223372036854775808, 0.1, 3.0],
    {},
    {"nested": {"deep": {"deeper": [1, 2, 3]}}, "empty_list": [], "empty_obj": {}},
]
JSON_OPTIONS = [
    {},
    {"sort_keys": True},
    {"indent": 2},
    {"indent": 2, "ensure_ascii": False},
    {"indent": 0},
    {"indent": 4, "sort_keys": True, "ensure_ascii": False},
    {"separators": [",", ":"]},
]
OUT["json_dumps"] = []
for v in JSON_VALUES:
    for o in JSON_OPTIONS:
        kw = dict(o)
        if "separators" in kw:
            kw["separators"] = tuple(kw["separators"])
        OUT["json_dumps"].append({"value": v, "options": o, "expected": json.dumps(v, **kw)})

# ---------------------------------------------------------------- yaml.dump
YAML_VALUES = [
    {"b": [1, 2], "a": {"x": "y"}, "c": [], "d": {}},
    "hello",
    "",
    1e16,
    None,
    [None, True, False, 0, -7, 1.5, -0.0, 1e-05],
    {"k": ["yes", "no", "on", "off", "True", "123", "0o17", "0x1F", "1_000", "1e10", ".5",
           "1.0", "2024-01-01", "2024-01-01 10:00:00", "~", "null", "=", "<<", "!", "&a",
           "*a", "-", "- x", "-x", "a: b", "a:b", "#c", "a #c", "a#c", "?", "? x", "[x]",
           "x]", "{y}", "x,y", "'q'", "\"q\"", "%p", "@a", "`t", "|", ">"]},
    {"k": ["café", " ", "a\tb", "line1\nline2", "trailing ", " leading",
           "multi\n\nbreaks\n", " sep", "it's", "both ' and \"", "back\\slash",
           "\U0001f600", "\x07bell", "﻿bom", "---", "...", "--- x"]},
    {"long": "word " * 30, "longer": ("abcdefghij " * 12).strip(),
     "nospace": "x" * 120, "quoted long": ("'quote' and " * 10).strip(),
     "dq": ("tab\there " * 15).strip()},
    {"x" * 125: 1, "y" * 122: 2, "": 3, "multi\nline key": 4, "k: v": 5, "yes": 6},
    [[1, [2, [3, {"a": [4, {"b": 5}]}]]], {"m": [{"n": [1, 2]}, []]}],
    {"text": "A fairly long sentence that keeps going and going well past the eighty column "
             "limit of the emitter, so it has to be folded somewhere sensible."},
]
OUT["yaml_dump_default"] = [{"value": v, "expected": yaml.dump(v, default_flow_style=False)}
                            for v in YAML_VALUES]
YAML_OPTIONS = [
    {"default_flow_style": None},
    {"default_flow_style": True},
    {"default_flow_style": False, "sort_keys": False},
    {"default_flow_style": False, "allow_unicode": True},
    {"default_flow_style": False, "width": 40},
    {"default_flow_style": False, "indent": 4},
    {"default_flow_style": False, "explicit_start": True},
]
OUT["yaml_dump_options"] = [{"value": v, "options": o, "expected": yaml.dump(v, **o)}
                            for v in YAML_VALUES for o in YAML_OPTIONS]

# ---------------------------------------------------------------- strftime / strptime
DT_SPECS = [
    {"naive": [2024, 3, 5, 7, 8, 9, 12345]},
    {"naive": [999, 1, 2, 23, 59, 0, 0]},
    {"naive": [2021, 1, 1, 0, 0, 0, 0]},
    {"naive": [2020, 12, 31, 12, 30, 45, 999999]},
    {"aware": [2024, 3, 5, 7, 8, 9, 0, 19800]},
    {"aware": [2024, 3, 5, 7, 8, 9, 0, 0]},
    {"aware": [2024, 3, 5, 7, 8, 9, 500, -3605]},
]


def make_dt(spec):
    if "naive" in spec:
        return datetime(*spec["naive"])
    *fields, offset = spec["aware"]
    return datetime(*fields, tzinfo=timezone(timedelta(seconds=offset)))


FORMATS = ["%Y-%m-%d %H:%M:%S", "%c", "%a %A %b %B %h", "%d %e %j %m %y %Y %C %G %g %V %u %w",
           "%H %I %k %l %M %S %p %r %R %T", "%U %W", "%D %F %x %X %v", "%f %z %Z", "%%|%n|%t",
           "%-d|%_d|%0d|%-H", "%Q|%E|%Ey|%Od|%5Y", "%+", "100% literal %", "%Y-%m-%dT%H:%M:%S%z"]
OUT["strftime"] = [{"dt": spec, "format": f, "expected": make_dt(spec).strftime(f)}
                   for spec in DT_SPECS for f in FORMATS]
OUT["isoformat"] = [{"dt": spec, "str": str(make_dt(spec)), "iso": make_dt(spec).isoformat()}
                    for spec in DT_SPECS]


def dt_out(d):
    off = d.utcoffset()
    return {"wall": [d.year, d.month, d.day, d.hour, d.minute, d.second, d.microsecond],
            "offset": None if off is None else int(off.total_seconds()), "str": str(d)}


STRPTIME_CASES = [
    ("2024-01-15 10:30:00", "%Y-%m-%d %H:%M:%S"),
    ("2024-01-15T10:30:00+0530", "%Y-%m-%dT%H:%M:%S%z"),
    ("2024-01-15T10:30:00Z", "%Y-%m-%dT%H:%M:%S%z"),
    ("2024-01-15T10:30:00-01:30", "%Y-%m-%dT%H:%M:%S%z"),
    ("2024-1-5", "%Y-%m-%d"),
    ("2024-01-01x", "%Y-%m-%d"),
    ("2024-13-01", "%Y-%m-%d"),
    ("2023-02-29", "%Y-%m-%d"),
    ("02-29", "%m-%d"),
    ("0000-01-01", "%Y-%m-%d"),
    ("2024-01-01 10:00:60", "%Y-%m-%d %H:%M:%S"),
    ("Tue Mar  5 07:08:09 2024", "%c"),
    ("tuesday MARCH 5 2024", "%A %B %d %Y"),
    ("12:30 PM", "%I:%M %p"),
    ("12:30 am", "%I:%M %p"),
    ("2024 100", "%Y %j"),
    ("2024 10 2", "%Y %U %w"),
    ("2024 10 2", "%Y %W %w"),
    ("2024 10 2", "%G %V %u"),
    ("10.5", "%S.%f"),
    ("24/12/31", "%y/%m/%d"),
    ("69/01/01", "%y/%m/%d"),
    ("2024-01-01  \t 10", "%Y-%m-%d %H"),
    ("2024-01-01t10", "%Y-%m-%dT%H"),
    ("x", "%Y"),
    ("2024", "%"),
    ("2024", "%Y %"),
    ("2024", "%q"),
    ("2024 utc", "%Y %Z"),
    ("2024-01-01T10:00:00+99:00", "%Y-%m-%dT%H:%M:%S%z"),
    ("2024-01-01 10:00:00.123", "%Y-%m-%d %H:%M:%S.%f"),
]
OUT["strptime"] = []
for data, fmt in STRPTIME_CASES:
    try:
        OUT["strptime"].append({"data": data, "format": fmt, "ok": dt_out(datetime.strptime(data, fmt))})
    except Exception as e:  # noqa: BLE001
        OUT["strptime"].append({"data": data, "format": fmt, **err(e)})

PARSE_CASES = ["2024-01-15 10:30:00", "2024-01-15T10:30:00", "2024-01-15T10:30:00+0200",
               "2024-01-15", "15/01/2024", "01/15/2024", "05/06/2024", "not a date",
               "2024-01-15 10:30", ""]
OUT["parse_timestamp"] = []
for data in PARSE_CASES:
    try:
        OUT["parse_timestamp"].append({"data": data, "ok": dt_out(H.parse_timestamp(data))})
    except Exception as e:  # noqa: BLE001
        OUT["parse_timestamp"].append({"data": data, **err(e)})
OUT["parse_timestamp_with_format"] = [
    {"data": "05/06/2024", "format": "%m/%d/%Y", "ok": dt_out(H.parse_timestamp("05/06/2024", "%m/%d/%Y"))},
    {"data": "2024", "format": "%d", **run(H.parse_timestamp, "2024", "%d")},
]
TS_CASES = [(0, "%Y-%m-%d %H:%M:%S"), (1700000000.5, "%Y-%m-%d %H:%M:%S.%f %z %Z"),
            (1.0000005, "%S.%f"), (1.0000015, "%S.%f"), (-1.5, "%Y-%m-%d %H:%M:%S.%f"),
            (253402300799, "%Y"), (1e18, "%Y")]
OUT["format_timestamp_aware"] = []
for ts, fmt in TS_CASES:
    OUT["format_timestamp_aware"].append({"ts": ts, "format": fmt, **run(H.format_timestamp, ts, fmt, True)})
OUT["format_timestamp_datetime"] = [
    {"dt": spec, "format": "%Y-%m-%d %H:%M:%S %z", "ok": H.format_timestamp(make_dt(spec), "%Y-%m-%d %H:%M:%S %z")}
    for spec in DT_SPECS]

# ---------------------------------------------------------------- helpers
CLEAN = ["  Hello   World  ", "Line 1\nLine 2", "", "   ", "a​b‌‍﻿c",
         "  ​ hello", "tabs\t\tand\nnew\n\nlines", "\x1c\x1dsep\x1e\x1f",
         " nbsp　ideo ls", "keep   \n\n  spaces"]
OUT["clean_text"] = [[t, p, H.clean_text(t, preserve_whitespace=p)] for t in CLEAN for p in (False, True)]
FILENAMES = [("my file.txt", 255), ('a<b>c:d"e/f\\g|h?i*j.txt', 255), ("...hidden. ", 255),
             ("   ", 255), ("", 255), ("a" * 300 + ".txt", 255), ("verylongname.tar.gz", 10),
             ("abc.defghijkl", 5), (".bashrc" * 3, 8), ("naïve café.md", 6),
             ("no_ext_name", 4), ("x.y", 1), ("name.ext", 2)]
OUT["safe_filename"] = [[f, m, H.safe_filename(f, m)] for f, m in FILENAMES]
OUT["hash_data"] = [
    {"kind": "str", "value": "hello", "expected": H.hash_data("hello")},
    {"kind": "str", "value": "café", "expected": H.hash_data("café")},
    {"kind": "bytes", "value": [0, 255, 10], "expected": H.hash_data(bytes([0, 255, 10]))},
    {"kind": "dict", "value": {"key": "value"}, "expected": H.hash_data({"key": "value"})},
    {"kind": "dict", "value": {"b": [1, 2.5], "a": {"z": None, "y": "é"}},
     "expected": H.hash_data({"b": [1, 2.5], "a": {"z": None, "y": "é"}})},
]
ENTITIES = [
    {"id": "e1", "text": "John", "type": "PERSON"},
    {"entity_id": "e2", "label": "Acme", "entity_type": "ORG", "confidence": 0.5,
     "start_offset": 3, "end_offset": 7, "metadata": {"m": 1}, "relations": []},
    {"id": "", "entity_id": "e3", "text": "", "label": "", "name": "Named", "start": 0,
     "start_offset": 9, "end": 0},
    {"name": "only name", "confidence": None},
    {},
]
OUT["normalize_entities"] = {"input": ENTITIES, "expected": H.normalize_entities(ENTITIES)}
MERGE = [
    ([{"a": 1, "b": {"c": 2}}, {"b": {"d": 3}, "e": 4}], True),
    ([{"a": 1, "b": {"c": 2}}, {"b": {"d": 3}, "e": 4}], False),
    ([{"x": {"y": {"z": 1}}}, {"x": {"y": {"w": 2}, "v": 3}}, {"x": {"y": 5}}], True),
    ([], True),
    ([{"a": [1]}, {"a": {"b": 1}}, {"a": {"c": 2}}], True),
]
OUT["merge_dicts"] = [[ds, deep, H.merge_dicts(*ds, deep=deep)] for ds, deep in MERGE]
OUT["chunk_list"] = [[list(range(7)), 3, H.chunk_list(list(range(7)), 3)],
                     [[], 2, H.chunk_list([], 2)], [[1, 2], 5, H.chunk_list([1, 2], 5)]]
OUT["chunk_list_zero"] = run(H.chunk_list, [1], 0)
FLAT = [({"a": {"b": 1, "c": 2}}, "", "."), ({"a": {"b": {"c": 1}}, "d": 2}, "", "/"),
        ({"a": {"b": 1}}, "root", "."), ({"": {"a": 1}}, "", "."), ({"a.b": 1, "a": {"b": 2}}, "", "."),
        ({"a": {}, "b": 1}, "", "."), ({"x": [1, {"y": 2}]}, "", ".")]
OUT["flatten_dict"] = [[d, p, s, run(H.flatten_dict, d, p, s)] for d, p, s in FLAT]
NESTED = {"config": {"database": {"host": "db", "port": 5432}, "list": [1, 2]}, "flag": None}
GETS = [("config.database.host", None, "."), ("config.database.missing", "dflt", "."),
        ("config.list.0", "x", "."), ("flag", "d", "."), ("config/database/port", None, "/"),
        ("config", None, "."), ("nope.deeper", 7, "."), ("config.database.host", None, "")]
OUT["get_nested_value"] = {"data": NESTED,
                           "cases": [[k, d, s, run(H.get_nested_value, NESTED, k, d, s)] for k, d, s in GETS]}
SETS = [({}, "a.b.c", 1, "."), ({"a": {"x": 1}}, "a.y", 2, "."), ({"a": 5}, "a.b", 1, "."),
        ({"a": 5}, "a.b.c", 1, "."), ({"a": "text"}, "a.b", 1, "."), ({"a": "text"}, "a.ex.c", 1, "."),
        ({"a": "text"}, "a.zz.c", 1, "."), ({"a": [1, "b"]}, "a.b.c", 1, "."), ({"a": [1]}, "a.b", 1, "."),
        ({"a": None}, "a.b", 1, "."), ({"a": True}, "a.b.c", 1, "."), ({}, "x/y", 3, "/"), ({}, "x", 1, "")]
OUT["set_nested_value"] = []
for d, k, v, s in SETS:
    data = json.loads(json.dumps(d))
    r = run(H.set_nested_value, data, k, v, s)
    OUT["set_nested_value"].append({"data": d, "key": k, "value": v, "sep": s, "result": r, "after": data})
OUT["classify_path_distance"] = [[h, H.classify_path_distance(h)] for h in (-1, 0, 1, 2, 3, 4, 6, 7, 100)]
OUT["format_data"] = [
    {"data": {"key": "value", "n": [1, 2.0], "u": "é"}, "format": "json",
     **run(H.format_data, {"key": "value", "n": [1, 2.0], "u": "é"}, "json")},
    {"data": {"key": "value", "n": [1, 2.0], "u": "é"}, "format": "yaml",
     **run(H.format_data, {"key": "value", "n": [1, 2.0], "u": "é"}, "yaml")},
    {"data": {}, "format": "unknown", **run(H.format_data, {}, "unknown")},
    {"data": [1], "format": "xml", **run(H.format_data, [1], "xml")},
]
OUT["safe_import"] = []
for name, pkg in [("json", None), ("os", None), ("definitely_missing_mod", None),
                  ("json.missing_sub", None), ("msvcrt", None), (".helpers", "semantica.utils"),
                  ("..x", "a")]:
    try:
        mod, ok = H.safe_import(name, package=pkg, default="DEFAULT")
        OUT["safe_import"].append({"name": name, "package": pkg, "available": ok,
                                   "module": getattr(mod, "__name__", None) if ok else mod})
    except Exception as e:  # noqa: BLE001
        OUT["safe_import"].append({"name": name, "package": pkg, **err(e)})

# graph payload normalizer (scenarios of tests/utils/test_normalize_graph_payload.py)
ENTITY = {"id": "e1", "name": "Acme"}
RELATIONSHIP = {"id": "r1", "source": "e1", "target": "e2"}
GRAPH_CASES = [
    {"entities": [ENTITY], "relationships": [RELATIONSHIP]},
    {"nodes": [ENTITY], "edges": [RELATIONSHIP]},
    {"entities": [ENTITY], "nodes": [], "relationships": [], "edges": []},
    {"entities": [], "nodes": [ENTITY]},
    {"entities": [ENTITY], "nodes": [ENTITY]},
    {"entities": [ENTITY], "nodes": [{"id": "different"}]},
    {"entities": [ENTITY, {"id": "e2", "name": "Beta"}], "nodes": [{"id": "e2", "name": "Beta"}, ENTITY]},
    {"entities": [ENTITY, ENTITY], "nodes": [ENTITY]},
    {"triplets": [{"s": "a", "p": "b", "o": "c"}]},
    {"entities": [ENTITY]},
    {"data": [ENTITY]},
    {"records": [ENTITY]},
    {"foo": "bar"},
    {},
    [ENTITY],
    "entities",
    42,
    None,
    {"entities": [], "data": [ENTITY]},
    {"nodes": [], "records": [ENTITY]},
    {"relationships": [], "records": [ENTITY]},
    {"edges": [], "records": [ENTITY]},
    {"triplets": [], "records": [ENTITY]},
    {"nodes": [], "edges": [], "statistics": {"node_count": 0}},
    {"entities": [ENTITY], "statistics": {"n": 1}},
    {"entities": [], "metadata": [ENTITY], "count": [1], "zeta": [2], "alpha": [3]},
    {"entities": "abc"},
    {"nodes": "abc"},
    {"triplets": "abc"},
    {"entities": 42},
    {"edges": 3.5},
    {"relationships": True},
    {"nodes": {"id": "n1"}},
    {"entities": {"e1": ENTITY}},
    {"edges": {"id": "r1"}},
    {"entities": ["Acme"]},
    {"entities": [ENTITY, "Acme"]},
    {"entities": [42]},
    {"entities": [None]},
    {"entities": [[ENTITY]]},
    {"entities": [ENTITY, ENTITY, "Acme"]},
    {"entities": None, "relationships": [RELATIONSHIP]},
    {"entities": None, "data": [ENTITY]},
    {"entities": [ENTITY], "nodes": "abc"},
    {"entities": [{"k": 1.0}], "nodes": [{"k": 1}]},
    {"entities": [{"b": 1, "a": 2}], "nodes": [{"a": 2, "b": 1}]},
    {"edges": [RELATIONSHIP], "relationships": [RELATIONSHIP], "entities": [ENTITY]},
]
OUT["normalize_graph_payload"] = [{"payload": p, **run(H.normalize_graph_payload, p)} for p in GRAPH_CASES]
OUT["require_mapping"] = [
    {"data": d, "keys": k, **run(H._require_mapping, d, k)}
    for d, k in [([ENTITY], ("entities", "relationships")), ({"entities": []}, ("entities",)),
                 ("x", ("nodes", "edges", "triplets")), (None, ("entities",))]]

# ---------------------------------------------------------------- exceptions
EXC = [
    ("SemanticaError", E.SemanticaError("base")),
    ("SemanticaError", E.SemanticaError("ctx", context={"stage": "load", "n": 2}, error_code="X9", extra=[1, "a"])),
    ("ValidationError", E.ValidationError("bad", field="name", value=None, constraint="required")),
    ("ValidationError", E.ValidationError("bad", validation_context={"row": 3})),
    ("TemporalValidationError", E.TemporalValidationError("tbad", temporal_context={"a": 1}, x=[1, "b"])),
    ("ProcessingError", E.ProcessingError("proc", processing_context={"s": "x"}, stage="parse", input_data={"k": 1})),
    ("ConfigurationError", E.ConfigurationError("cfg", config_key="k", config_value=3, expected_type="str")),
    ("QualityError", E.QualityError("qual", quality_score=0.4, threshold=0.7)),
    ("QualityError", E.QualityError("qual2", metrics={"m": 1})),
]
OUT["exceptions"] = []
for name, e in EXC:
    attrs = {}
    for a in ("field", "value", "constraint", "validation_context", "stage", "input_data", "output_data",
              "processing_context", "config_key", "config_value", "expected_type", "config_context",
              "quality_score", "threshold", "metrics", "quality_context"):
        if hasattr(e, a):
            attrs[a] = getattr(e, a)
    OUT["exceptions"].append({
        "class": name, "message": e.message, "context": e.context, "details": e.details,
        "error_code": e.error_code, "str": str(e), "repr": repr(e), "to_dict": e.to_dict(), "attrs": attrs,
        "format_default": E.format_exception(e),
        "format_no_context": E.format_exception(e, include_context=False),
        "format_traceback": E.format_exception(e, include_traceback=True),
        "format_max_20": E.format_exception(e, max_length=20),
        "format_max_neg": E.format_exception(e, max_length=-5),
        "format_max_0": E.format_exception(e, max_length=0),
        "handle": E.handle_exception(e, context={"caller": "x"}, log_error=False),
        "handle_no_tb": E.handle_exception(e, log_error=False, include_traceback=False,
                                           recovery_action="retry"),
    })
BUILTIN = [ValueError("vv"), TypeError("tt"), KeyError("id"), ValueError(""), RuntimeError("rr"),
           FileNotFoundError(2, "No such file or directory", "missing.txt"), FileNotFoundError("File not found: x")]
OUT["builtin_exceptions"] = [{"type": type(e).__name__, "str": str(e), "repr": repr(e),
                              "format": E.format_exception(e, include_traceback=True),
                              "handle": E.handle_exception(e, log_error=False)} for e in BUILTIN]

# ---------------------------------------------------------------- logging
class Capture(logging.Handler):
    def __init__(self):
        super().__init__(logging.DEBUG)
        self.records = []

    def emit(self, record):
        extra = {k: v for k, v in record.__dict__.items()
                 if k in ("metrics", "error_type", "error_message", "context", "details", "traceback",
                          "error_code", "quality_metrics", "threshold", "report")}
        if "report" in extra:
            extra["report"] = dict(extra["report"])
            extra["report"]["timestamp"] = "<timestamp>"
        self.records.append({"level": record.levelname, "message": record.getMessage(), "extra": extra})


def captured(logger_name, f, *a, **k):
    lg = logging.getLogger("semantica." + logger_name)
    old_level, old_prop = lg.level, lg.propagate
    h = Capture()
    lg.addHandler(h)
    lg.setLevel(logging.DEBUG)
    lg.propagate = False
    try:
        result = run(f, *a, **k)
    finally:
        lg.removeHandler(h)
        lg.setLevel(old_level)
        lg.propagate = old_prop
    return {"result": result, "records": h.records}


OUT["log_performance"] = [
    {"args": ["f", 0.123456, {}], **captured("performance", L.log_performance, "f", 0.123456)},
    {"args": ["load", 2.5, {"memory_usage": 1048576 * 100, "input_size": 10, "output_size": "big",
                            "cache_hit": 0, "items": 3}],
     **captured("performance", L.log_performance, "load", 2.5, memory_usage=1048576 * 100,
                input_size=10, output_size="big", cache_hit=0, items=3)},
    {"args": ["g", 1.0, {"cache_hit": True, "level": 30}],
     **captured("performance", L.log_performance, "g", 1.0, cache_hit=True, level=30)},
    {"args": ["h", 1.0, {"level": "INFO"}], **captured("performance", L.log_performance, "h", 1.0, level="INFO")},
    {"args": ["i", 1.0, {"memory_usage": "x"}], **captured("performance", L.log_performance, "i", 1.0, memory_usage="x")},
]
_ve = E.ValidationError("bad input", validation_context={"row": 1}, field="name")
OUT["log_error"] = [
    {"error": "ValidationError", "context": {"stage": "s"}, "details": {"extra": 1},
     **captured("error", L.log_error, _ve, context={"stage": "s"}, extra=1)},
    {"error": "ValueError", "context": None, "details": {"escalate": True, "include_traceback": False},
     **captured("error", L.log_error, ValueError("oops"), escalate=True, include_traceback=False)},
    {"error": "ValueError", "context": None, "details": {"level": 30},
     **captured("error", L.log_error, ValueError("warn"), level=30)},
    {"error": "ValueError", "context": None, "details": {"level": "ERROR"},
     **captured("error", L.log_error, ValueError("strlevel"), level="ERROR")},
]
OUT["log_data_quality"] = [
    {"metrics": {"score": 0.85, "completeness": 0.9, "accuracy": 1}, "options": {},
     **captured("quality", L.log_data_quality, {"score": 0.85, "completeness": 0.9, "accuracy": 1})},
    {"metrics": {"overall_score": 0.5, "validity": 0.25}, "options": {"threshold": 0.6, "generate_report": True},
     **captured("quality", L.log_data_quality, {"overall_score": 0.5, "validity": 0.25}, threshold=0.6, generate_report=True)},
    {"metrics": {"score": 0.95}, "options": {"generate_report": True, "alert_on_threshold": False},
     **captured("quality", L.log_data_quality, {"score": 0.95}, generate_report=True, alert_on_threshold=False)},
    {"metrics": {"score": 0.0}, "options": {"generate_report": True},
     **captured("quality", L.log_data_quality, {"score": 0.0}, generate_report=True)},
    {"metrics": {"completeness": 0.9}, "options": {}, **captured("quality", L.log_data_quality, {"completeness": 0.9})},
    {"metrics": {"score": "high"}, "options": {}, **captured("quality", L.log_data_quality, {"score": "high"})},
    {"metrics": {"score": 0.8, "accuracy": None}, "options": {}, **captured("quality", L.log_data_quality, {"score": 0.8, "accuracy": None})},
]


@L.log_execution_time
def _ok_func():
    return 42


@L.log_execution_time
def _bad_func():
    raise ValueError("boom")


ok_cap = captured("performance", _ok_func)
bad_cap = captured("performance", _bad_func)
for cap in (ok_cap, bad_cap):
    for r in cap["records"]:
        r["message"] = r["message"].split(" | Execution Time")[0]
OUT["log_execution_time"] = {"ok": ok_cap, "bad": bad_cap}

# Formatter (%-style) on a fixed record
rec = logging.LogRecord("semantica.test", logging.WARNING, "/x/y/mod.py", 42, "hello %s", ("world",), None)
rec.created = 1700000000.25
rec.msecs = 250.0
FMTS = ["%(message)s", "%(levelname)s:%(name)s:%(message)s", "[%(levelname)-8s] %(message)s",
        "%(levelno)d %(levelno)05d %(lineno)d", "%(name)r %(message).3s|%(name)10s|",
        "%(created)f %(msecs)03d", "%(filename)s %(module)s", "%% literal %(message)s",
        "%(levelno)x %(levelno)#o %(levelno)+d %(created).2e %(created)g %(msecs)G"]
OUT["formatter"] = []
for f in FMTS:
    rec2 = logging.LogRecord("semantica.test", logging.WARNING, "/x/y/mod.py", 42, "hello world", (), None)
    rec2.created = 1700000000.25
    rec2.msecs = 250.0
    OUT["formatter"].append({"format": f, "expected": logging.Formatter(f).format(rec2)})
OUT["formatter_invalid"] = [run(lambda f=f: logging.Formatter(f) and "valid") for f in ["hello", "%(x)", "%(message)s", "%(x)q"]]
OUT["formatter_missing_field"] = run(lambda: logging.Formatter("%(nope)s").format(rec))

# ---------------------------------------------------------------- types
OUT["enums"] = {}
for enum_cls in (T.EntityType, T.DataType, T.ProcessingStatus, T.QualityLevel, T.RelationshipType):
    OUT["enums"][enum_cls.__name__] = [{"name": m.name, "value": m.value, "str": str(m), "format": f"{m}"}
                                       for m in enum_cls]
# Module-level names `self`/`cls` would be read by ModuleDetector as frame locals.
del enum_cls
ent = T.Entity(id="e1", text="John Doe", type=T.EntityType.PERSON, confidence=0.95, start=0, end=8,
               metadata={"src": "doc"}, relations=[{"id": "r1"}])
OUT["entity_to_dict"] = T.entity_to_dict(ent)
OUT["entity_to_dict_str_type"] = T.entity_to_dict(T.Entity(id="e2", text="Acme", type="ORG"))
rel = T.Relationship(id="r1", source_id="e1", target_id="e2", type=T.RelationshipType.WORKS_FOR,
                     confidence=0.8, metadata={"m": 1}, properties={"since": 2020})
OUT["relationship_to_dict"] = T.relationship_to_dict(rel)
OUT["dict_to_entity"] = [run(lambda d=d: T.entity_to_dict(T.dict_to_entity(d))) for d in [
    {"id": "e1", "text": "t", "type": "PERSON"},
    {"id": "e1", "text": "t", "type": "PERSON", "confidence": 0.5, "start": 1, "end": 2,
     "metadata": {"a": 1}, "relations": [{"id": "r"}]},
    {"text": "t", "type": "X"},
    {"id": "e1", "type": "X"},
]]
OUT["dict_to_relationship"] = [run(lambda d=d: T.relationship_to_dict(T.dict_to_relationship(d))) for d in [
    {"id": "r1", "source_id": "a", "target_id": "b", "type": "REL"},
    {"id": "r1", "source_id": "a", "target_id": "b", "type": "REL", "confidence": 0.3,
     "metadata": {"x": 1}, "properties": {"p": True}},
    {"id": "r1", "source_id": "a", "type": "REL"},
]]
GUARDS = [{"id": 1, "text": "t", "type": "x"}, {"id": 1, "text": "t"}, [1], "s", None,
          {"id": "r", "source_id": "a", "target_id": "b", "type": "t"}, {"id": "r", "source_id": "a", "type": "t"},
          {"status": "completed"}, {"status": "done"}, {"status": "paused"}, {"status": 1}, {}]
OUT["type_guards"] = [[g, T.is_entity_dict(g), T.is_relationship_dict(g), T.is_processing_result(g)] for g in GUARDS]
OUT["entity_eq"] = [
    ent == T.Entity(id="e1", text="other", type="X"), ent == T.Entity(id="e2", text="John Doe", type="PERSON"),
    ent == {"id": "e1"}, ent == {"entity_id": "e1"}, ent == {"id": "", "entity_id": "e1"}, ent == {"id": "e2"},
    ent == "e1", hash(ent) == hash("e1"),
]
r_ok, r_err = T.Result(value=5), T.Result(error="failed")
r_none = T.Result()
batch = T.BatchResult([r_ok, r_err, T.Result(value=7), r_none])
OUT["result"] = {"ok": [repr(r_ok), bool(r_ok), r_ok.success, r_ok.value, r_ok.error],
                 "err": [repr(r_err), bool(r_err), r_err.success, r_err.value, r_err.error],
                 "none": [repr(r_none), bool(r_none)],
                 "batch": [batch.success_count, batch.failure_count, batch.total_count, batch.success_rate],
                 "empty_batch_rate": T.BatchResult([]).success_rate}
pr = T.ProcessingResult(status=T.ProcessingStatus.COMPLETED)
qm = T.QualityMetrics(score=0.9)
OUT["dataclass_defaults"] = {
    "entity": T.entity_to_dict(T.Entity(id="i", text="t", type="x")),
    "relationship": T.relationship_to_dict(T.Relationship(id="r", source_id="a", target_id="b", type="t")),
    "processing_result": [pr.status.value, pr.data, pr.entities, pr.relationships, pr.metadata, pr.error, pr.execution_time],
    "quality_metrics": [qm.score, qm.completeness, qm.accuracy, qm.consistency, qm.validity, qm.timeliness, qm.metadata],
}

# ---------------------------------------------------------------- validators
def vcall(f, *a, **k):
    r = run(f, *a, **k)
    if "ok" in r:
        r["ok"] = list(r["ok"])
    return r


TYPES = {"str": str, "int": int, "float": float, "bool": bool, "list": list, "dict": dict,
         "NoneType": type(None), "object": object}
VD = [
    ({"name": "Alice"}, None, {"required_fields": ["name", "age"]}),
    ({"name": "Alice", "age": "30"}, None, {"field_types": {"name": "str", "age": "int"}}),
    ({"name": "Alice", "age": True}, None, {"field_types": {"age": "int"}}),
    ({"x": 1.0}, None, {"field_types": {"x": "int"}}),
    ({"x": 1}, None, {"field_types": {"x": "float"}}),
    ([1, 2], None, {"field_types": {"a": "int"}}),
    (["a", 2], None, {"field_types": {"a": "int"}}),
    ("abc", None, {"field_types": {"a": "int"}}),
    ("xyz", None, {"field_types": {"a": "int"}}),
    (5, None, {"field_types": {"a": "int"}}),
    (None, None, {"field_types": {"a": "int"}}),
    ("hello", None, {"min_length": 6}),
    ("hello", None, {"max_length": 3}),
    ("hello", None, {"min_length": 2.5}),
    ("hello", None, {"pattern": "h.l"}),
    ("hello", None, {"pattern": "el"}),
    ("hello", None, {"pattern": "[0-9]+"}),
    ("hello", None, {"allowed_values": ["a", "b"]}),
    ("a", None, {"allowed_values": ["a", "b"]}),
    (5, None, {"min_value": 10}),
    (5, None, {"max_value": 2}),
    (5.5, None, {"max_value": 5}),
    (True, None, {"min_value": 2}),
    (3, None, {"allowed_values": [1, 2.0, "3"]}),
    (2, None, {"allowed_values": [1, 2.0, "3"]}),
    (1, None, {"allowed_values": [True]}),
    ([1, 2, 3], None, {"min_length": 5}),
    ([1, 2, 3], None, {"max_length": 2}),
    ([1, "a"], None, {"item_type": "int"}),
    ([1, True], None, {"item_type": "int"}),
    ([1, 1.0], None, {"unique": True}),
    ([1, "1"], None, {"unique": True}),
    ([[1], [2]], None, {"unique": True}),
    ([1, 2], None, {"unique": False}),
    ({"a": 1}, {"type": "object", "properties": {"a": {"type": "string"}}}, {}),
    ({"a": "x"}, {"type": "object", "required": ["a", "b"]}, {}),
    (5, {"type": "string"}, {}),
    (True, {"type": "integer"}, {}),
    (1.5, {"type": "number"}, {}),
    ("s", {"type": "boolean"}, {}),
    ({}, {"type": "array"}, {}),
    ([], {"type": "object"}, {}),
    ({"a": {"b": 1}}, {"properties": {"a": {"properties": {"b": {"type": "string"}}}}}, {}),
    ({"a": 1}, {"properties": ["a"]}, {}),
    ({"a": 1}, {"properties": {"a": "type"}}, {}),
    ({"a": 1}, {"properties": {"a": "nothing"}}, {}),
    ({"a": 1}, {"properties": {"a": ["type"]}}, {}),
    ({"a": 1}, {"properties": {"a": 5}}, {}),
    ({"a": 1}, {"required": "ab"}, {}),
    ({"a": 1}, {"required": [1]}, {}),
    ({"a": 1}, {"required": [[1]]}, {}),
    ({"a": 1}, {"required": 5}, {}),
    ({"a": 1}, {}, {"required_fields": ["b"]}),
    ("text", {"type": "string"}, {"min_length": 10}),
]
OUT["validate_data"] = []
for data, schema, cons in VD:
    kw = dict(cons)
    if "field_types" in kw:
        kw["field_types"] = {k: TYPES[v] for k, v in kw["field_types"].items()}
    if "item_type" in kw:
        kw["item_type"] = TYPES[kw["item_type"]]
    OUT["validate_data"].append({"data": data, "schema": schema, "constraints": cons,
                                 **vcall(V.validate_data, data, schema, **kw)})
VC = [
    ({"a": 1}, ["a", "b", "c"], {}),
    ([1], None, {}),
    ({"a": 1, "x": 2}, None, {"allowed_keys": ["a"]}),
    ({"a": 1}, None, {"allowed_keys": []}),
    ({"a": "1"}, None, {"key_types": {"a": "int"}}),
    ({"a": {"b": {"c": "x"}}}, None, {"key_types": {"c": "int"}}),
    ({"a": {"b": {"c": "x"}}}, None, {"key_types": {"c": "int"}, "nested_validation": False}),
    ({"a": {"z": 1}}, ["a"], {"allowed_keys": ["a"]}),
    ({}, [], {}),
]
OUT["validate_config"] = []
for cfg, req, opts in VC:
    kw = dict(opts)
    if "key_types" in kw:
        kw["key_types"] = {k: TYPES[v] for k, v in kw["key_types"].items()}
    OUT["validate_config"].append({"config": cfg, "required_keys": req, "options": opts,
                                   **vcall(V.validate_config, cfg, req, **kw)})
VE = [{"id": "e1", "text": "Alice", "type": "Person"}, {"text": "Alice"},
      {"id": "e1", "text": "A", "type": "P", "confidence": 1.5},
      {"id": "e1", "text": "A", "type": "P", "confidence": "high"},
      {"id": "e1", "text": "A", "type": "P", "confidence": True},
      {"id": "e1", "text": "A", "type": "P", "confidence": 0}]
OUT["validate_entity"] = [[e, list(V.validate_entity(e))] for e in VE]
OUT["validate_entity_object"] = list(V.validate_entity(T.Entity(id="e", text="t", type="x", confidence=7.0)))
VR = [{"id": "r", "source_id": "a", "target_id": "b", "type": "t"}, {"id": "r", "source_id": "a"},
      {"id": "r", "source_id": "a", "target_id": "b", "type": "t", "confidence": -0.1},
      {"id": "r", "source_id": "a", "target_id": "b", "type": "t", "confidence": None}]
OUT["validate_relationship"] = [[r, list(V.validate_relationship(r))] for r in VR]
OUT["validate_relationship_object"] = list(V.validate_relationship(
    T.Relationship(id="r", source_id="a", target_id="b", type="t", confidence=9.0)))
URLS = ["https://example.com", "invalid-url", "http://localhost:8080/path?q=1", "ftp://example.com",
        "http://192.168.0.1", "https://sub.domain.co.uk/a/b", "https://example.com\n",
        "https://example.com/has space", "HTTPS://EXAMPLE.COM", "http://-bad.com",
        "http://example.toolongtld", "http://example.c", "http://ex_ample.com", "https://a.b.c.de?x"]
OUT["validate_url"] = [[u, list(V.validate_url(u))] for u in URLS]
EMAILS = ["test@example.com", "invalid-email", "a.b+c@sub.example.org", "a@b.c", "a@b.co\n",
          "@example.com", "test@example.com ", "UPPER@EXAMPLE.COM", "x@y_z.com"]
OUT["validate_email"] = [[e, list(V.validate_email(e))] for e in EMAILS]
OUT["validate_file_path"] = [
    ["data.json", False, None, list(V.validate_file_path("data.json"))],
    ["data.JSON", False, [".json", ".txt"], list(V.validate_file_path("data.JSON", allowed_extensions=[".json", ".txt"]))],
    ["data.csv", False, [".json", ".TXT"], list(V.validate_file_path("data.csv", allowed_extensions=[".json", ".TXT"]))],
    [".bashrc", False, [".bashrc"], list(V.validate_file_path(".bashrc", allowed_extensions=[".bashrc"]))],
    ["file.", False, [".txt"], list(V.validate_file_path("file.", allowed_extensions=[".txt"]))],
    ["a.tar.gz", False, [".gz"], list(V.validate_file_path("a.tar.gz", allowed_extensions=[".gz"]))],
    ["/definitely/missing/file.txt", True, None, list(V.validate_file_path("/definitely/missing/file.txt", must_exist=True))],
    ["noext", False, [], list(V.validate_file_path("noext", allowed_extensions=[]))],
]
OUT["validate_string_constraints"] = [list(V.validate_string_constraints("abc", {"min_length": 1, "max_length": 5, "pattern": "a", "allowed_values": ["abc"]}))]
OUT["validate_numeric_constraints"] = [list(V.validate_numeric_constraints(7, {"min_value": 1, "max_value": 10})),
                                       vcall(V.validate_numeric_constraints, "7", {"min_value": 1})]
OUT["validate_list_constraints"] = [list(V.validate_list_constraints([1, 2], {"min_length": 1, "item_type": int, "unique": True}))]
OUT["validate_types"] = [list(V.validate_types({"a": 1, "b": "x"}, {"a": int, "b": int, "c": str}))]
OUT["validate_required_fields"] = [list(V.validate_required_fields({"a": 1}, ["a", "b", "c"]))]
OUT["validate_schema"] = [list(V.validate_schema({"a": 1}, {"type": "object", "required": ["a"]}))]

# ---------------------------------------------------------------- skos
SKOS_EDGE_CASES = [{"source": "A", "target": "B", "type": "skos:broader"}, {"type": "skos:narrower"},
                   {"predicate": "skos:broader"}, {"relation": "skos:narrower"},
                   {"relationship": "skos:broader"}, {"edge_type": "skos:broader"},
                   {"type": "rdfs:subClassOf"}, {"type": "rdfs:subClassOf", "predicate": "skos:narrower"},
                   {"type": 5}, "not-a-dict", None, [], {}]
OUT["is_skos_hierarchy_edge"] = [[e, S.is_skos_hierarchy_edge(e)] for e in SKOS_EDGE_CASES]
SKOS_VALIDATE = [
    ([{"source": "A", "target": "B", "type": "skos:broader"}, {"source": "B", "target": "C", "type": "skos:broader"}], []),
    ([{"source": "A", "target": "A", "type": "skos:broader"}], []),
    ([{"source": "B", "target": "A", "type": "skos:narrower"}, {"source": "B", "target": "A", "type": "skos:broader"}], []),
    ([{"source_id": " A ", "target_id": "B", "type": "skos:broader"}, {"source": "B", "target": "A", "type": "skos:broader"}], []),
    ([{"source": 1, "target": 2, "type": "skos:broader"}, {"source": "2", "target": "1", "type": "skos:broader"}], []),
    ([{"source": "C", "target": "A", "type": "skos:broader"}],
     [{"source": "A", "target": "B", "type": "skos:broader"}, {"source": "B", "target": "C", "type": "skos:broader"}]),
    ([{"source": "C", "target": "D", "type": "skos:broader"}],
     [{"source": "X", "target": "Y", "type": "skos:broader"}, {"source": "Y", "target": "X", "type": "skos:broader"}]),
    ([{"source": None, "target": "A", "type": "skos:broader"}, {"source": "A", "target": "", "type": "skos:broader"},
      {"target": "A", "type": "skos:broader"}, {"source": "A", "target": "A", "type": "rdfs:x"}], []),
    ([{"source": "A", "source_id": "Z", "target": "A", "type": "skos:broader"}], []),
    ([{"source": None, "source_id": "A", "target": "A", "type": "skos:broader"}], []),
]
OUT["validate_skos_hierarchy"] = [{"new": n, "existing": x, **run(S.validate_skos_hierarchy, n, x)} for n, x in SKOS_VALIDATE]

# ---------------------------------------------------------------- progress displays
def item(**kw):
    return P.ProgressItem(**kw)


ITEMS = [
    dict(module="ingest", submodule="FileIngestor", status="running", message="File: a.txt"),
    dict(file="/data/docs/report.pdf", module="parse", submodule="DocumentParser", status="completed",
         start_time=100.0, end_time=102.5, progress_percentage=100.0, estimated_remaining=0.0,
         metadata={"extraction_counts": {"tables": 2, "images": 1, "pages": 10}, "core_dependency": "docling"}),
    dict(module="kg_qa", submodule=None, status="failed", message="Semantica: custom action text",
         start_time=10.0, end_time=70.0, progress_percentage=42.5, estimated_remaining=125.0),
    dict(module="vector_store", submodule="Indexer", status="pending", progress_percentage=33.3,
         estimated_remaining=7200.0, processed_items=5, total_items=15),
    dict(module=None, submodule=None, status="weird", metadata={"extraction_counts": {"pages": 3}}),
    dict(module="core", submodule="Semantica", status="running", message="Building",
         progress_percentage=150.0, estimated_remaining=59.99),
    dict(module="Split_Text", submodule="S", status="completed", start_time=5.0, end_time=5.0,
         metadata={"extraction_counts": {}}),
    dict(module="parse", submodule="P", status="running", message="Converting with Docling"),
    dict(module="", submodule="", file="", status="completed", progress_percentage=-5.0),
]


def console_capture(use_emoji, actions):
    d = P.ConsoleProgressDisplay(use_emoji=use_emoji, update_interval=0)
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        actions(d)
    return buf.getvalue(), dict(d.current_lines)


OUT["console_lines"] = []
for use_emoji in (True, False):
    for spec in ITEMS:
        it = item(**spec)
        text, lines = console_capture(use_emoji, lambda d, it=it: d.update(it))
        OUT["console_lines"].append({"use_emoji": use_emoji, "item": spec, "output": text, "current_lines": lines})
summary_items = [item(**spec) for spec in ITEMS]
OUT["console_summary"] = []
for use_emoji in (True, False):
    text, _ = console_capture(use_emoji, lambda d: d.show_summary(summary_items))
    OUT["console_summary"].append({"use_emoji": use_emoji, "output": text})
text, lines = console_capture(True, lambda d: (d.update(item(**ITEMS[0])), d.clear()))
OUT["console_clear"] = {"output": text, "current_lines": lines}

# Pipeline rendering through the singleton tracker.
P.ProgressTracker._instance = None
P._global_tracker = None
tracker = P.ProgressTracker(enabled=True, use_emoji=True, update_interval=0)
tracker.displays = []
P.ProgressTracker._instance = tracker
tracker.register_pipeline_modules("pipe", ["ingest", "parse", "core"], {"ingest": 0, "parse": 1, "core": 2})
tid1 = tracker.start_tracking(module="ingest", submodule="FileIngestor", message="Ingesting", file="a.txt")
tid2 = tracker.start_tracking(module="core", submodule="Semantica", message="Building", pipeline_id="pipe")
tracker.stop_tracking(tid1, metadata={"extraction_counts": {"pages": 2}})
pipeline_items = tracker.get_pipeline_items("pipe")
OUT["pipeline_items"] = [{"module": i.module, "submodule": i.submodule, "status": i.status,
                          "pipeline_id": i.pipeline_id, "pipeline_order": i.pipeline_order,
                          "metadata": i.metadata, "file": i.file} for i in pipeline_items]
pd = P.ConsoleProgressDisplay(use_emoji=False, update_interval=0)
buf = io.StringIO()
probe = item(module="core", submodule="Semantica", status="running", message="Building", pipeline_id="pipe")
with contextlib.redirect_stdout(buf):
    pd.update(probe)
    pd.update(probe)
import re as _re
_mask = lambda text: _re.sub(r"\d+\.\d\ds", "<t>s", text)
OUT["console_pipeline_output_lines"] = [_mask(line) for line in buf.getvalue().split("\n")]
jd = P.JupyterProgressDisplay(use_emoji=True)
OUT["jupyter_pipeline_html"] = _mask(jd._build_html([probe]))
P.ProgressTracker._instance = None
OUT["jupyter_html"] = []
for use_emoji in (True, False):
    jd = P.JupyterProgressDisplay(use_emoji=use_emoji)
    plain = [item(**{k: v for k, v in spec.items() if k != "start_time" or "end_time" in spec}) for spec in ITEMS]
    OUT["jupyter_html"].append({"use_emoji": use_emoji, "html": jd._build_html(plain)})
many = [item(module="m%d" % i, submodule="S", status="completed") for i in range(12)]
OUT["jupyter_html_last_ten"] = P.JupyterProgressDisplay()._build_html(many)
captured_html = []
P.display = lambda obj, **kw: captured_html.append(obj)
P.HTML = lambda s: s
P.JupyterProgressDisplay(use_emoji=True).show_summary(summary_items)
P.JupyterProgressDisplay(use_emoji=False).show_summary(summary_items)
OUT["jupyter_summary_html"] = captured_html

file_logger = logging.getLogger("semantica.progress_fixture")
cap = Capture()
file_logger.addHandler(cap)
file_logger.setLevel(logging.DEBUG)
file_logger.propagate = False
fd = P.FileProgressDisplay(logger=file_logger)
for spec in ITEMS:
    fd.update(item(**spec))
fd.update(item(**dict(ITEMS[0], message="second")))
fd.show_summary(summary_items)
fd.clear()
OUT["file_display"] = {"messages": [r["message"] for r in cap.records],
                       "items": [[i.module, i.submodule, i.file, i.message] for i in fd.items]}

# ModuleDetector.detect_from_frame on synthetic frames
class _Foo:
    pass


def frame(name, self_obj=None, cls=None, qualname=None, has_self=False, has_cls=False):
    locs = {}
    if has_self:
        locs["self"] = self_obj
    if has_cls:
        locs["cls"] = cls
    return pytypes.SimpleNamespace(f_globals={"__name__": name} if name is not None else {},
                                   f_locals=locs, f_code=pytypes.SimpleNamespace(co_qualname=qualname or "f"))


FRAMES = [
    ({"module_name": "semantica.ingest.file_ingestor", "self_class": "_Foo"}, frame("semantica.ingest.file_ingestor", _Foo(), has_self=True)),
    ({"module_name": "semantica.parse", "cls_name": "_Foo"}, frame("semantica.parse", cls=_Foo, has_cls=True)),
    ({"module_name": "tests.test_x", "qualname": "TestX.test_y"}, frame("tests.test_x", qualname="TestX.test_y")),
    ({"module_name": "x.semantica.y", "qualname": "f"}, frame("x.semantica.y", qualname="f")),
    ({"module_name": "semantica", "qualname": "A.b.c"}, frame("semantica", qualname="A.b.c")),
    ({"qualname": "plain"}, frame(None, qualname="plain")),
    ({"module_name": "semantica..z"}, frame("semantica..z")),
]
OUT["detect_from_frame"] = [[fi, list(P.ModuleDetector.detect_from_frame(fr))] for fi, fr in FRAMES]
OUT["detect_depth0"] = list(P.ModuleDetector.detect_from_call_stack(depth=0))

# ---------------------------------------------------------------- tracker state machine
P.ProgressTracker._instance = None
t = P.ProgressTracker(enabled=True, use_emoji=True, update_interval=0)
t.displays = []
a = t.start_tracking(file="f.txt", module="ingest", submodule="FileIngestor", message="m")
b = t.start_tracking(module="parse", submodule="Parser")
t.update_progress(b, processed=0, total=0, message="")
t.update_progress(b, processed=3, total=4, message="three")
c = t.start_tracking(module="kg", submodule="Builder")
t.update_tracking(c, status="failed", message="boom")
t.stop_tracking(a, metadata={"k": 1})
OUT["tracker_state"] = {
    "ids": [a, b, c],
    "items": [[i.module, i.status, i.message, i.progress_percentage, i.estimated_remaining, i.metadata, i.emoji]
              for i in t.items],
    "active": {k: [v.module, v.status, v.message, v.processed_items, v.total_items, v.progress_percentage]
               for k, v in t.active_items.items()},
    "update_unknown": run(t.update_tracking, "nope"),
    "disabled_start": P.ProgressTracker(enabled=False).start_tracking(module="m", submodule="s"),
}
t2 = P.ProgressTracker(enabled=True, use_emoji=False, update_interval=0)
t2.displays = []
t2.register_pipeline_modules("p1", ["a", "b", "a"])
OUT["tracker_pipeline_order"] = json.loads(json.dumps(
    {"contexts": t2.pipeline_contexts, "order": t2.pipeline_module_order,
     "items": list(t2.pipeline_items.keys())}))
x = t2.start_tracking(module="b", submodule="B")
OUT["tracker_pipeline_join"] = {"id": x, "pipeline_id": t2.active_items[x].pipeline_id,
                                "order": t2.active_items[x].pipeline_order, "emoji": t2.active_items[x].emoji}
t2.update_tracking(x, status="completed")
OUT["tracker_pipeline_kept"] = {"items": len(t2.items), "pipeline": list(t2.pipeline_items["p1"].keys()),
                                "pct": t2.pipeline_items["p1"][x].progress_percentage}
t2.clear_pipeline_context("p1")
OUT["tracker_after_clear"] = json.loads(json.dumps(
    {"contexts": t2.pipeline_contexts, "items": list(t2.pipeline_items.keys())}))

# ---------------------------------------------------------------- track(), @track_progress, throttle
P.ProgressTracker._instance = None
P._global_tracker = None
t3 = P.ProgressTracker(enabled=True, use_emoji=True, update_interval=0)
t3.displays = []
P.ProgressTracker._instance = t3
with t3.track(file="doc.pdf", module="ingest", submodule="X", message="m") as tid_ok:
    pass
try:
    with t3.track(file="doc.pdf", module="ingest", submodule="Y", message="m") as tid_bad:
        raise ValueError("boom")
except ValueError as e:
    propagated = str(e)
OUT["track"] = {"ids": [tid_ok, tid_bad], "propagated": propagated,
                "items": [[i.file, i.module, i.submodule, i.status, i.message] for i in t3.items]}


@P.track_progress(file="doc.pdf", module="ingest", submodule="Decorated")
def decorated_ok(x):
    return x * 2


@P.track_progress(module="ingest", submodule="Decorated")
def decorated_bad():
    raise RuntimeError("bad things")


@P.track_progress
def bare():
    return "bare"


P._global_tracker = t3
r1 = decorated_ok(21)
try:
    decorated_bad()
except RuntimeError as e:
    r2 = str(e)
r3 = bare()
OUT["track_progress"] = {"results": [r1, r2, r3],
                         "items": [[i.file, i.module, i.submodule, i.status, i.message] for i in t3.items[2:]]}

throttle_buf = io.StringIO()
t4 = P.ProgressTracker(enabled=True, use_emoji=False, update_interval=3600)
console = P.ConsoleProgressDisplay(use_emoji=False, update_interval=3600)
t4.displays = [console]
with contextlib.redirect_stdout(throttle_buf):
    tid = t4.start_tracking(module="m", submodule="S", message="a")
    t4.update_tracking(tid, message="b")
    t4.update_progress(tid, processed=1, total=2, message="c")
    t4.update_tracking(tid, message="d")
_masked = _re.sub(r"\d+\.\d\ds", "<t>s", throttle_buf.getvalue())
_masked = _re.sub(r"Rate: [0-9.]+/s", "Rate: <r>/s", _masked)
_masked = _re.sub(r"ETA: [0-9.]+[smh]", "ETA: <e>", _masked)
OUT["throttle"] = {"output": _masked, "last_update_restored": console.last_update > 0}

P.ProgressTracker._instance = None
P._global_tracker = None
g1 = P.get_progress_tracker()
same = g1 is P.ProgressTracker.get_instance()
P.ProgressTracker._instance = None
g2 = P.get_progress_tracker()
fresh = P.ProgressTracker.get_instance()
OUT["singletons"] = {"global_is_instance": same, "global_cached": g2 is g1, "instance_recreated": fresh is not g1,
                     "default_enabled": g1.enabled, "default_use_emoji": g1.use_emoji,
                     "default_interval": g1.update_interval,
                     "display_types": [type(d).__name__ for d in g1.displays]}
P.ProgressTracker._instance = None
P._global_tracker = None

summary_buf = io.StringIO()
t5 = P.ProgressTracker(enabled=True, use_emoji=False, update_interval=0)
t5.displays = [P.ConsoleProgressDisplay(use_emoji=False, update_interval=0)]
with contextlib.redirect_stdout(summary_buf):
    a5 = t5.start_tracking(module="parse", submodule="P", file="/x/a.txt")
    t5.stop_tracking(a5, status="failed", message="bad")
    b5 = t5.start_tracking(module="kg", submodule="K")
    summary_buf.truncate(0)
    summary_buf.seek(0)
    t5.show_summary()
OUT["tracker_show_summary"] = _re.sub(r"\d+\.\d\ds", "<t>s", summary_buf.getvalue())
t5.enabled = False
OUT["tracker_disabled_noops"] = [t5.get_pipeline_items("x"), t5.start_tracking(module="a", submodule="b")]

# ---------------------------------------------------------------- constants and exports
from semantica.utils import constants as C  # noqa: E402

OUT["constants"] = {name: getattr(C, name) for name in [
    "SUPPORTED_DOCUMENT_FORMATS", "SUPPORTED_IMAGE_FORMATS", "SUPPORTED_AUDIO_FORMATS",
    "SUPPORTED_VIDEO_FORMATS", "SUPPORTED_RDF_FORMATS", "SUPPORTED_VECTOR_STORES",
    "SUPPORTED_GRAPH_DBS", "DEFAULT_CONFIG", "ERROR_CODES", "PERFORMANCE_THRESHOLDS",
    "QUALITY_LEVELS", "ENTITY_TYPES", "RELATIONSHIP_TYPES", "PROCESSING_STATUS", "DATA_TYPES",
    "API_ENDPOINTS", "FILE_SIZE_LIMITS", "RETRY_CONFIG", "CACHE_CONFIG"]}
OUT["exports"] = list(U.__all__)

json.dump(OUT, open(sys.argv[1], "w", encoding="utf-8"), indent=1, ensure_ascii=False, sort_keys=False)
assert "cls" not in globals() and "self" not in globals()
print("wrote", sys.argv[1])
