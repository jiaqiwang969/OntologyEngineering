"""Generate tests/fixtures/oracle.json from the original Semantica (the oracle).

Every expected value of the sem-normalize characterisation tests comes from running the
original code here. Run from var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin PYTHONHASHSEED=0 ../venv/bin/python -I \
        <crate>/tests/fixtures/generate_oracle.py <crate>/tests/fixtures/oracle.json

Dates are computed with `datetime.now()` fixed at 2024-01-02 00:30 (both Semantica's and
dateutil's clocks, and dateutil's two-digit-year pivot), and time zones are read from
tests/fixtures/zoneinfo, the same TZif files the Rust tests load. The script only computes
values (no network, no credentials, no writes outside the output file and a scratch
directory next to it).
"""

import contextlib
import datetime as dtmod
import hashlib
import io
import json
import math
import os
import shutil
import sys
import types
import zoneinfo
from datetime import datetime, timedelta, timezone
from unittest.mock import patch

sys.path.insert(0, os.getcwd())

from semantica.utils.progress_tracker import get_progress_tracker  # noqa: E402

get_progress_tracker().displays = []

import chardet  # noqa: E402
import dateutil.parser._parser as dateutil_parser  # noqa: E402

import semantica.normalize as N  # noqa: E402
from semantica.normalize import date_normalizer as DN  # noqa: E402
from semantica.normalize import methods as M  # noqa: E402
from semantica.normalize.config import NormalizeConfig, normalize_config  # noqa: E402
from semantica.normalize.data_cleaner import (  # noqa: E402
    DataCleaner,
    DataValidator,
    DuplicateDetector,
    DuplicateGroup,
    MissingValueHandler,
)
from semantica.normalize.registry import MethodRegistry, method_registry  # noqa: E402

OUT = {}
HERE = os.path.dirname(os.path.abspath(sys.argv[1]))
ZONEINFO = os.path.join(os.path.dirname(os.path.abspath(__file__)), "zoneinfo")
SCRATCH = os.path.join(HERE, "_oracle_scratch")


def conv(value):
    """JSON form of a result (tuples as lists, dataclasses as dicts, non-finite floats and
    bytes and datetimes tagged)."""
    if isinstance(value, float):
        if math.isnan(value):
            return {"float": "nan"}
        if math.isinf(value):
            return {"float": "inf" if value > 0 else "-inf"}
        return value
    if isinstance(value, (bytes, bytearray)):
        return {"bytes": list(value)}
    if isinstance(value, datetime):
        try:
            iso = value.isoformat()
        except Exception as e:  # noqa: BLE001
            iso = {"error": type(e).__name__, "message": str(e)}
        return {"datetime": iso, "tzname": value.tzname() if not isinstance(iso, dict) else None}
    if isinstance(value, DuplicateGroup):
        return {
            "records": conv(value.records),
            "similarity_score": conv(value.similarity_score),
            "canonical_record": conv(value.canonical_record),
        }
    if isinstance(value, N.ValidationResult):
        return {"valid": value.valid, "errors": conv(value.errors), "warnings": conv(value.warnings)}
    if isinstance(value, (list, tuple)):
        return [conv(v) for v in value]
    if isinstance(value, dict):
        return {str(k): conv(v) for k, v in value.items()}
    return value


def err(e):
    return {"error": type(e).__name__, "message": str(e)}


def run(f, *a, **k):
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            return {"ok": conv(f(*a, **k))}
    except Exception as e:  # noqa: BLE001
        return err(e)


# ---------------------------------------------------------------------------------------
# Fixed clock and zone data

FIXED_NOW = datetime(2024, 1, 2, 0, 30)


class FixedDateTime(datetime):
    @classmethod
    def now(cls, tz=None):
        fixed = cls(2024, 1, 2, 0, 30)
        return fixed if tz is None else fixed.replace(tzinfo=tz)


fake_datetime_module = types.ModuleType("datetime")
fake_datetime_module.__dict__.update(dtmod.__dict__)
fake_datetime_module.datetime = FixedDateTime
dateutil_parser.DEFAULTPARSER.info._year = 2024
dateutil_parser.DEFAULTPARSER.info._century = 2000
zoneinfo.reset_tzpath(to=[ZONEINFO])

clock = contextlib.ExitStack()
clock.enter_context(patch("semantica.normalize.date_normalizer.datetime", FixedDateTime))
clock.enter_context(patch.object(dateutil_parser, "datetime", fake_datetime_module))


def dt_input(spec):
    """A datetime from {"dt": iso, "offset": seconds | None} (an instance of the patched
    class, so the module's isinstance checks accept it)."""
    base = FixedDateTime.fromisoformat(spec["dt"])
    if spec.get("offset") is None:
        return base
    return base.replace(tzinfo=timezone(timedelta(seconds=spec["offset"])))


def date_arg(value):
    return dt_input(value) if isinstance(value, dict) else value


# ---------------------------------------------------------------------------------------
# Text normalization

TEXTS = [
    "Hello   World",
    "  Mixed\tTabs\r\nand\rlines  ",
    "Ünïcödé é ﬁsh ①",
    "“Smart” ‘quotes’ – en — em…",
    "Crème Brûlée",
    "they're bill's friends from the UK",
    "ǆemal straße ΣΑΣ აბ",
    "Multiple\n\n\n\nblank lines\n \n\t\nend",
    "",
    "\u001c\u001d spaces 　 here there",
]
TEXT_KWARGS = [
    {},
    {"case": "lower"},
    {"case": "upper"},
    {"case": "title"},
    {"unicode_form": "NFKC"},
    {"unicode_form": "NFD"},
    {"normalize_diacritics": True},
    {"normalize_diacritics": True, "remove_diacritics": True},
    {"line_break_type": "windows"},
    {"line_break_type": "mac"},
    {"unicode_form": "BOGUS"},
    {"case": 5},
]

tn = N.TextNormalizer()
OUT["normalize_text"] = [
    {"text": t, "kwargs": k, "result": run(tn.normalize_text, t, **k)}
    for t in TEXTS
    for k in TEXT_KWARGS
]
OUT["normalize_text_none"] = run(tn.normalize_text, None)

NORMALIZE_SOURCES = [
    "TEST  Value",
    None,
    42,
    3.5,
    ["A  B", {"content": "X  Y", "id": 1}, {"other": 1}, 5, None, ["l", 1]],
    [{"content": None}, {"content": ""}, {"content": 0}],
    [{"content": 5}],
    [{"content": ["x"]}],
    {"key": "Value"},
]
OUT["normalize"] = [
    {"source": s, "kwargs": k, "result": run(tn.normalize, s, **k)}
    for s in NORMALIZE_SOURCES
    for k in ({}, {"case": "lower"})
]
OUT["clean_text"] = [
    {"text": t, "kwargs": k, "result": run(tn.clean_text, t, **k)}
    for t in ["<p>Hi &amp; bye</p>  there", "  Café <b>#1</b>!  ", None]
    for k in ({}, {"remove_html": False}, {"remove_special_chars": True}, {"unicode_form": "NFD"})
]
OUT["standardize_format"] = [
    {"text": t, "format_type": f, "result": run(tn.standardize_format, t, f)}
    for t in ["  a \t\n b  ", "x\u001cy", ""]
    for f in ["standard", "compact", "preserve", "other"]
]
OUT["process_batch"] = [
    {"texts": ts, "kwargs": k, "result": run(tn.process_batch, ts, **k)}
    for ts in (["TEST 1", "Test  2", ""], [])
    for k in ({}, {"case": "title"})
]

un = N.UnicodeNormalizer()
OUT["normalize_unicode"] = [
    {"text": t, "form": f, "result": run(un.normalize_unicode, t, f)}
    for t in ["é", "é", "ﬁ ① ½", "", None]
    for f in ["NFC", "NFD", "NFKC", "NFKD", "BOGUS"]
]
HANDLE_ENCODING = [
    {"data": "already text", "source": "utf-8", "target": "utf-8"},
    {"data": {"bytes": list("café".encode("utf-8"))}, "source": "utf-8", "target": "utf-8"},
    {"data": {"bytes": list("café".encode("latin-1"))}, "source": "latin-1", "target": "utf-8"},
    {"data": {"bytes": list(b"\x93quoted\x94")}, "source": "cp1252", "target": "utf-8"},
    {"data": {"bytes": list(b"caf\xe9")}, "source": "ascii", "target": "utf-8"},
    {"data": {"bytes": list("café".encode("utf-8"))}, "source": "utf-8", "target": "ascii"},
    {"data": {"bytes": list(b"\xff\xfeh\x00i\x00")}, "source": "utf-16", "target": "utf-8"},
    {"data": {"bytes": list(b"abc")}, "source": "bogus", "target": "utf-8"},
    {"data": {"bytes": list(b"\xff")}, "source": "utf-8", "target": "utf-8"},
]


def data_arg(value):
    return bytes(value["bytes"]) if isinstance(value, dict) else value


OUT["handle_encoding"] = [
    dict(case, result=run(un.handle_encoding, data_arg(case["data"]), case["source"], case["target"]))
    for case in HANDLE_ENCODING
]
OUT["unicode_process_special_chars"] = [
    {"text": t, "result": run(un.process_special_chars, t)}
    for t in ["‘a’ “b” – — …", "plain"]
]

wn = N.WhitespaceNormalizer()
WS_TEXTS = ["a\r\nb\rc\nd", "  x  \t y  \n\n\n z  ", "\r\n\r\n", "line1\n    line2\n\tline3"]
OUT["normalize_whitespace"] = [
    {"text": t, "line_break_type": lb, "result": run(wn.normalize_whitespace, t, lb)}
    for t in WS_TEXTS + [""]
    for lb in ["unix", "windows", "mac", "other"]
]
OUT["handle_line_breaks"] = [
    {"text": t, "line_break_type": lb, "result": run(wn.handle_line_breaks, t, lb)}
    for t in WS_TEXTS
    for lb in ["unix", "windows", "mac"]
]
OUT["process_indentation"] = [
    {"text": t, "indent_type": it, "result": run(wn.process_indentation, t, it)}
    for t in WS_TEXTS + ["        eight", "\t\ttwo tabs"]
    for it in ["spaces", "tabs", "other"]
]

sp = N.SpecialCharacterProcessor()
SP_TEXTS = ["“Crème” — naïve…", "ȩ́", "Ångström Å"]
OUT["process_special_chars"] = [
    {"text": t, "normalize_diacritics": nd, "kwargs": k, "result": run(sp.process_special_chars, t, nd, **k)}
    for t in SP_TEXTS
    for nd in (False, True)
    for k in ({}, {"remove_diacritics": True}, {"remove_diacritics": 0})
]
OUT["normalize_punctuation"] = [
    {"text": t, "result": run(sp.normalize_punctuation, t)} for t in SP_TEXTS
]
OUT["process_diacritics"] = [
    {"text": t, "remove": r, "result": run(sp.process_diacritics, t, r)}
    for t in SP_TEXTS + ["ẛ̣"]
    for r in (False, True)
]

# ---------------------------------------------------------------------------------------
# Text cleaning

tc = N.TextCleaner()
CLEAN_TEXTS = [
    "<p>Hello <b>World</b></p>",
    "  <div>A &amp; B &lt;tag&gt; &quot;q&quot; &#39;s&#39; &nbsp;x &copy; &#169;</div>  ",
    "Line1\n\nLine2\t\tTabbed   spaces",
    "été ﬁ ①",
    "Special!@#$%^&*() chars_here 123",
    "",
    None,
]
CLEAN_KWARGS = [
    {},
    {"remove_html": False},
    {"normalize_whitespace": False},
    {"normalize_unicode": False},
    {"remove_special_chars": True},
    {"remove_special_chars": True, "allow_spaces": False},
    {"unicode_form": "NFKD"},
    {"unicode_form": "BOGUS"},
]
OUT["clean"] = [
    {"text": t, "kwargs": k, "result": run(tc.clean, t, **k)}
    for t in CLEAN_TEXTS
    for k in CLEAN_KWARGS
]
HTML_TEXTS = [
    "<p>Hello <b>World</b></p>",
    "<html><head><title>T</title><style>p{color:red}</style><script>var x = \"<b>\";</script></head><body><p>A &amp; B &lt;tag&gt; &nbsp;x &copy; &#169; &#xA9;</p><!-- comment --><div>  line1\n  line2 </div></body></html>",
    "<!DOCTYPE html><p>One<br>Two</p><p>Three</p>",
    "plain text no tags",
    "<p>unclosed <b>bold",
    "a < b and c > d",
    "<textarea><b>raw</b></textarea>",
    "<p>caf&eacute; &euro;5</p>",
    "<template><p>tpl</p></template>after",
    "<p title=\"a>b\">attr</p>",
    "<style>x</style>y<noscript>ns</noscript>",
    "&lt;&#60;&#x3c;",
    "<p>  spaced　</p>",
    "<ul><li>one</li><li> two </li></ul>",
    "",
]
OUT["remove_html"] = [
    {"text": t, "preserve_structure": ps, "result": run(tc.remove_html, t, ps)}
    for t in HTML_TEXTS
    for ps in (False, True)
]
OUT["cleaner_normalize_whitespace"] = [
    {"text": t, "result": run(tc.normalize_whitespace, t)}
    for t in ["a\t\tb\n\nc\r\nd", "  many    spaces  ", "", "　x　"]
]
OUT["cleaner_normalize_unicode"] = [
    {"text": t, "form": f, "result": run(tc.normalize_unicode, t, f)}
    for t in ["é", "ﬁ", ""]
    for f in ["NFC", "NFD", "NFKC", "NFKD", "XX"]
]
OUT["remove_special_chars"] = [
    {"text": t, "allow_spaces": s, "allow_newlines": n, "result": run(tc.remove_special_chars, t, s, n)}
    for t in ["Hi! How\nare\tyou? été #1", ""]
    for s in (True, False)
    for n in (True, False)
]
SANITIZE_TEXTS = [
    "<p>ok</p><script type='x'>alert(1)</script ><b>keep</b>",
    "<SCRIPT>\nmulti\nline\n</SCRIPT>after",
    "<iframe src='x'></iframe>text<a href=\"JavaScript:alert(1)\">x</a>",
    "<img src=\"data:image/png;base64,AAAA\">",
    "",
]
OUT["sanitize"] = [
    {"text": t, "remove_data_urls": r, "result": run(tc.sanitize, t, r)}
    for t in SANITIZE_TEXTS
    for r in (False, True)
]
OUT["trim"] = [
    {"text": t, "remove_empty_lines": r, "result": run(tc.trim, t, r)}
    for t in ["  a\n\n  \nb  \n", "\n\nx\n", ""]
    for r in (False, True)
]
OUT["clean_batch"] = [
    {"texts": ts, "kwargs": k, "result": run(tc.clean_batch, ts, **k)}
    for ts in (["<b>a</b>  b", "", "x\ty"],)
    for k in ({}, {"remove_html": False, "remove_special_chars": True})
]

# ---------------------------------------------------------------------------------------
# Entities

ALIASES = {"j. doe": "John Doe", "Bill Gates": "William Henry Gates III", "ibm": "", "acme": "ACME Corp."}
en = N.EntityNormalizer(alias_map=ALIASES)
ENTITY_NAMES = [
    "  john   doe  ",
    "dr. john   smith",
    "MR. JOHN DOE",
    "j. doe",
    "bill gates",
    "Sir Isaac Newton",
    "madam curie",
    "Prof.   ada lovelace",
    "sirius black",
    "o'neil mcdonald-smith",
    "ΣΑΣ ΣΟΦΙΑ",
    "ibm",
    "Acme",
    "",
    None,
]
OUT["normalize_entity"] = [
    {"name": n, "entity_type": t, "resolve_aliases": r, "result": run(en.normalize_entity, n, t, r)}
    for n in ENTITY_NAMES
    for t in (None, "Person", "Organization")
    for r in (True, False)
]
OUT["entity_resolve_aliases"] = [
    {"name": n, "result": run(en.resolve_aliases, n)} for n in ["J. Doe", "bill gates", "BILL GATES", "ibm", "nobody"]
]
OUT["entity_disambiguate"] = [
    {"name": n, "context": c, "result": run(en.disambiguate_entity, n, **c)}
    for n in ["Apple", ""]
    for c in ({}, {"entity_type": "Organization", "context": "tech"}, {"entity_type": None})
]
OUT["link_entities"] = [
    {"entities": es, "kwargs": k, "result": run(en.link_entities, es, **k)}
    for es in (["J. Doe", "bill gates", "dr. who", "J. Doe"], [])
    for k in ({}, {"entity_type": "Person"}, {"resolve_aliases": False})
]

ar = N.AliasResolver(alias_map=ALIASES)
OUT["alias_resolve"] = [
    {"name": n, "result": run(ar.resolve_aliases, n)} for n in ["J. DOE", "bill gates", "Acme", "ibm", "x"]
]
OUT["alias_resolve_empty"] = run(N.AliasResolver().resolve_aliases, "anything")
OUT["map_variants"] = run(ar.map_variants, "Jon", "Person")
OUT["handle_cultural_variations"] = [run(ar.handle_cultural_variations, "Müller", c) for c in (None, "de-DE")]

ed = N.EntityDisambiguator()
OUT["disambiguate"] = [
    {"name": n, "context": c, "result": run(ed.disambiguate, n, **c)}
    for n in ["Paris"]
    for c in ({}, {"entity_type": "Location", "context": "France"}, {"entity_type": 3})
]
OUT["classify_entity_type"] = [
    {"name": n, "result": run(ed.classify_entity_type, n)}
    for n in ["John Doe", "Apple", "apple pie", "", "Ⓐbc", "ǅemal x", "École Normale", "1 Main"]
]
OUT["calculate_confidence"] = run(ed.calculate_confidence, ["a", "b", "a"])

nv = N.NameVariantHandler()
NAMES = ["Dr. John Doe", "MR. JOHN DOE", "mrs.   jane   o'hara", "Prof.Ada", "sirius black", "Smith", "Dr. Dr. Who", ""]
OUT["normalize_name_format"] = [
    {"name": n, "format_type": f, "result": run(nv.normalize_name_format, n, f)}
    for n in NAMES
    for f in ["standard", "title", "lower", "other"]
]
OUT["process_variants"] = [{"name": n, "result": run(nv.process_variants, n)} for n in NAMES]
OUT["handle_titles_and_honorifics"] = [
    {"name": n, "result": run(nv.handle_titles_and_honorifics, n)} for n in NAMES + ["Mrs. Smith", "Ms.Jones"]
]
custom_nv = N.NameVariantHandler(titles=["Captain", "Dr."])
OUT["custom_titles"] = [
    {"name": n, "result": run(custom_nv.normalize_name_format, n, "standard")}
    for n in ["captain jack sparrow", "Dr. No", "Mr. Bean"]
]

# ---------------------------------------------------------------------------------------
# Dates

dn = N.DateNormalizer()
DATE_INPUTS = [
    "2023-01-15",
    "2023-01-15T10:30:00",
    "2023-01-15T10:30:00+05:30",
    "2023-01-15T10:30:00Z",
    "2023-07-04 18:45:30.250",
    "Jan 5 2023",
    "5 January 2023 10:30 PM",
    "March 3rd, 2021 at 4pm",
    "today",
    "yesterday",
    "tomorrow",
    "now",
    "3 days ago",
    "2 weeks from now",
    "1 month ago",
    "2 years later",
    "13 months ago",
    "10 days",
    "next week",
    "2023/02/30",
    "12/31/99",
    "01/02/75",
    "Monday",
    "99",
    "1.5h",
    "20230115T103000",
    "Sep.20.2009",
    "4:30:21.447",
    "2023-01-01 12:00 EST",
    "2023-01-01 20:23 -31",
    "2023-01-01 12:00 +0530",
    "nan",
    "",
    None,
    0,
    12345,
    [],
    [1],
    {"dt": "2023-03-12T07:30:00", "offset": None},
    {"dt": "2023-03-12T07:30:00", "offset": -18000},
    {"dt": "2023-11-05T05:30:00", "offset": 0},
    {"dt": "2023-11-05T06:30:00", "offset": 0},
]
DATE_FORMATS = ["ISO8601", "date", "%Y/%m/%d %H:%M %Z%z"]
DATE_ZONES = ["UTC", "America/New_York", "Asia/Kolkata", "Europe/London", "Invalid/Zone", ""]
OUT["normalize_date"] = [
    {"input": d, "format": f, "timezone": z, "result": run(dn.normalize_date, date_arg(d), f, z)}
    for d in DATE_INPUTS
    for f in DATE_FORMATS
    for z in DATE_ZONES
]
OUT["normalize_time"] = [
    {"input": d, "result": run(dn.normalize_time, date_arg(d))}
    for d in ["10:30", "10:30 PM", "2023-01-15T10:30:45.123", "garbage", "", 123, None,
              {"dt": "2023-01-15T08:09:10.000500", "offset": 3600}]
]
REFERENCES = [
    None,
    {"dt": "2024-01-31T12:00:00", "offset": None},
    {"dt": "2024-03-31T08:00:00", "offset": None},
    {"dt": "2024-02-29T23:59:59", "offset": 3600},
]
RELATIVE = ["3 days ago", "1 month ago", "1 month from now", "2 years later", "14 months ago",
            "5 weeks", "yesterday", "  TOMORROW ", "2023-05-05", "nonsense", "10000 years ago"]
OUT["process_relative_date"] = [
    {"expression": e, "reference": r, "result": run(dn.process_relative_date, e, date_arg(r) if r else None)}
    for e in RELATIVE
    for r in REFERENCES
]
rp = N.RelativeDateProcessor()
OUT["calculate_date_offset"] = [
    {"expression": e, "reference": r, "result": run(rp.calculate_date_offset, e, date_arg(r))}
    for e in ["2 days ago", "3 weeks later"]
    for r in REFERENCES[1:]
]
OUT["handle_relative_terms"] = [
    {"expression": e, "reference": r, "result": run(rp.handle_relative_terms, e, date_arg(r))}
    for e in ["today", "yesterday", "tomorrow", "now"]
    for r in REFERENCES[1:2]
]
TEMPORAL = [
    "from 2023-01-01 to 2023-01-31",
    "between 2023-01-01 and 2023-03-31",
    "The meeting is on March 3rd, 2021 at 4pm",
    "today",
    "10:30",
    "From 9 TO 5",
    "nothing here",
    "Q3 2023 report",
    "",
]
OUT["parse_temporal_expression"] = [{"text": t, "result": run(dn.parse_temporal_expression, t)} for t in TEMPORAL]
tp = N.TemporalExpressionParser()
OUT["extract_date_components"] = [{"text": t, "result": run(tp.extract_date_components, t)} for t in TEMPORAL]
OUT["extract_time_components"] = [{"text": t, "result": run(tp.extract_time_components, t)} for t in TEMPORAL]
OUT["process_temporal_ranges"] = [{"text": t, "result": run(tp.process_temporal_ranges, t)} for t in TEMPORAL]

tz = N.TimeZoneNormalizer()
TZ_DATES = [
    {"dt": "2023-01-15T12:00:00", "offset": None},
    {"dt": "2023-07-15T12:00:00", "offset": None},
    {"dt": "2023-07-15T12:00:00", "offset": 7200},
    {"dt": "2023-11-05T05:30:00", "offset": 0},
    {"dt": "2023-11-05T06:30:00", "offset": 0},
    {"dt": "2023-03-12T06:59:59", "offset": 0},
    {"dt": "2023-03-12T07:00:00", "offset": 0},
    {"dt": "1850-01-01T00:00:00", "offset": 0},
    {"dt": "2099-07-01T00:00:00", "offset": 0},
]
OUT["normalize_timezone"] = [
    {"input": d, "timezone": z, "result": run(tz.normalize_timezone, date_arg(d), z)}
    for d in TZ_DATES
    for z in DATE_ZONES
]
OUT["convert_to_utc"] = [
    {"input": d, "source": s, "result": run(tz.convert_to_utc, date_arg(d), s)}
    for d in TZ_DATES
    for s in (None, "Asia/Kolkata", "America/New_York")
]
OUT["handle_dst_transitions"] = [
    {"input": d, "timezone": z, "result": run(tz.handle_dst_transitions, date_arg(d), z)}
    for d in TZ_DATES
    for z in ("America/New_York", "Europe/London")
]

# ---------------------------------------------------------------------------------------
# Numbers

nn = N.NumberNormalizer()
NUMBERS = ["1,234.56", "50%", "1.5e3", "2E-4", "1.5k", "2M", "3b", "1t", "  42 ", "$1,000", "€50",
           "abc", "one", "12abc%", "1_000", "١٢٣", "-5", "+7.5", "1e400", "-inf%", "",
           "5.", "0x10", "1 000 000", "£ 3.50", "7K", "1.2.3", 42, 3.5, None, [1]]
OUT["normalize_number"] = [{"input": x, "result": run(nn.normalize_number, x)} for x in NUMBERS]
QUANTITIES = ["5 kg", "100 meters", "3.5 liters", "1,000 ft", "about 12 lbs of flour", "10kgs",
              "no number here", "1.2.3 km", "5 Km", "  7   OZ", "2 parsecs"]
OUT["normalize_quantity"] = [{"input": q, "result": run(nn.normalize_quantity, q)} for q in QUANTITIES]
CONVERSIONS = [(1, "km", "m"), (100, "lb", "kg"), (1, "mile", "kilometer"), (5, "kilogram", "meter"),
               (1, "foo", "m"), (2, "KM", "M"), (1, "km", "mi"), (1, "km", "kg"), (3, "gallon", "liter"),
               (0.5, "inch", "centimeter")]
OUT["convert_units"] = [{"args": list(c), "result": run(nn.convert_units, *c)} for c in CONVERSIONS]
CURRENCIES = ["$1,234.56", "100 EUR", "€50", "¥1000", "₹ 2,500", "USD 99.99", "price: 100",
              "100", "RUB100", "rubles 100", "50 gbp", "100 CNY", "no amount", "1.000,50 €",
              "CHF", "₩", "AUDUSD 5"]
OUT["process_currency"] = [
    {"input": c, "default": d, "result": run(nn.process_currency, c, d)}
    for c in CURRENCIES
    for d in ("USD", "EUR")
]

uc = N.UnitConverter()
OUT["validate_units"] = [
    {"args": [a, b], "result": run(uc.validate_units, a, b)}
    for a, b in [("km", "m"), ("meter", "kilometer"), ("kilogram", "meter"), ("foo", "m"), ("KG", "G"),
                 ("pound", "ounce"), ("liter", "gallon")]
]
OUT["get_conversion_factor"] = [
    {"args": [a, b], "result": run(uc.get_conversion_factor, a, b)}
    for a, b in [("km", "base"), ("mile", "km"), ("foo", "base"), ("g", "kg"), ("KM", "M"), ("x", "y")]
]
OUT["normalize_unit"] = [{"unit": u, "result": run(uc.normalize_unit, u)}
                         for u in ["km", "KGS", "lbs", "Meters", "ml", "parsec", "OZ", "in"]]

cn = N.CurrencyNormalizer()
OUT["normalize_currency"] = [{"input": c, "result": run(cn.normalize_currency, c)} for c in CURRENCIES]
OUT["convert_currency"] = run(cn.convert_currency, 12.5, "USD", "EUR")
OUT["validate_currency_code"] = [{"code": c, "result": run(cn.validate_currency_code, c)}
                                 for c in ["USD", "usd", "XYZ", "eur", ""]]

sh = N.ScientificNotationHandler()
OUT["parse_scientific_notation"] = [{"input": s, "result": run(sh.parse_scientific_notation, s)}
                                    for s in ["1.23e4", "1.23E-2", "abc", " 5e0 ", "1_0e1_0", "inf"]]
OUT["convert_to_scientific"] = [
    {"number": n, "precision": p, "result": run(sh.convert_to_scientific, n, p)}
    for n in [1500, 0.000123, -2.5e-10, 0, 123456789.0, 1e300, float("inf")]
    for p in (None, 0, 2, 10)
]
OUT["normalize_precision"] = [
    {"number": n, "digits": d, "result": run(sh.normalize_precision, n, d)}
    for n in [123.456, 0.0012345, 0, -98765.4321, 5.5, 2.5, 9.99, 1234, 0.000999, 1e15 + 0.3]
    for d in (0, 1, 2, 3, 4)
]

# ---------------------------------------------------------------------------------------
# Data cleaning

DATASET = [
    {"id": 1, "name": "John Doe", "age": 30, "email": "john@example.com", "score": 1.5},
    {"id": 2, "name": "Jane Smith", "age": 25, "email": "jane@example.com", "score": 2},
    {"id": 3, "name": "John Doe", "age": 30, "email": "john@example.com", "score": 1.5},
    {"id": 4, "name": "Bob", "age": None, "email": "bob@example.com", "score": True},
    {"id": 5, "name": "N/A", "age": 40, "email": "", "score": None},
    {"id": 6, "name": "john doe", "age": 30.0, "email": "JOHN@example.com", "score": 1},
]
SCHEMA = {"fields": {"name": {"type": "str", "required": True}, "age": {"type": ["int", "float"]},
                     "email": {"type": "str", "required": True}, "score": {"type": "float"}}}
CLEAN_OPTIONS = [
    {},
    {"missing_strategy": "fill", "fill_value": "?"},
    {"missing_strategy": "impute", "method": "mean"},
    {"missing_strategy": "impute", "method": "median"},
    {"missing_strategy": "bogus"},
    {"schema": SCHEMA},
    {"duplicate_criteria": {"key_fields": ["name"], "threshold": 0.9}},
    {"duplicate_criteria": {"threshold": 0.5}},
    {"duplicate_criteria": {"key_fields": [1, "name"]}},
    {"duplicate_criteria": ["bad"]},
    {"strategy": "remove"},
    {"schema": {"fields": {"name": {"type": 5}}}},
    {"schema": ["not", "a", "dict"]},
]
dc = DataCleaner()
OUT["clean_data"] = [
    {"options": o, "flags": f, "result": run(dc.clean_data, DATASET, *f, **o)}
    for o in CLEAN_OPTIONS
    for f in ([True, True, True], [False, True, False], [True, False, True])
]
OUT["clean_data_empty"] = run(dc.clean_data, [])
OUT["cleaner_detect_duplicates"] = [
    {"threshold": t, "key_fields": k, "result": run(dc.detect_duplicates, DATASET, t, k)}
    for t in (None, 0.5, 1.0)
    for k in (None, ["name", "email"], [])
]
OUT["cleaner_validate_data"] = [
    {"dataset": d, "schema": s, "result": run(dc.validate_data, d, s)}
    for d in (DATASET, [])
    for s in (None, SCHEMA, {})
]
OUT["cleaner_handle_missing_values"] = [
    {"strategy": s, "options": o, "result": run(dc.handle_missing_values, DATASET, s, **o)}
    for s, o in [("remove", {}), ("fill", {}), ("fill", {"fill_value": None}), ("fill", {"fill_value": 0}),
                 ("impute", {}), ("impute", {"method": "zero"}), ("impute", {"method": "mode"}), ("other", {})]
]

dd = DuplicateDetector(similarity_threshold=0.6, key_fields=["name"])
OUT["detector_detect_duplicates"] = [
    {"threshold": t, "key_fields": k, "result": run(dd.detect_duplicates, DATASET, t, k)}
    for t in (None, 0.95)
    for k in (None, ["email"], [])
]
PAIRS = [
    ({"a": "hello", "b": 10}, {"a": "hallo", "b": 10}),
    ({"a": "Hello"}, {"a": "hello"}),
    ({"a": "", "b": None}, {"a": "x", "b": None}),
    ({"a": 5}, {"a": 6}),
    ({"a": 1}, {"a": 1.0}),
    ({"a": [1, 2]}, {"a": [1, 2]}),
    ({"x": 1}, {"y": 1}),
    ({"a": "abc", "b": "xyz", "c": True}, {"a": "abd", "c": 1, "b": "XYZ"}),
]
OUT["calculate_similarity"] = [
    {"r1": a, "r2": b, "key_fields": k, "result": run(DuplicateDetector().calculate_similarity, a, b, k)}
    for a, b in PAIRS
    for k in (None, ["a"], ["a", "missing"])
]
GROUPS = [
    {"records": [{"id": 1, "v": None}, {"id": 2, "v": "x", "w": 3}], "similarity_score": 0.9,
     "canonical_record": {"id": 1, "v": None}},
    {"records": [{"id": 5}], "similarity_score": 1.0, "canonical_record": {}},
    {"records": [{"id": 7, "v": 1}, {"id": 8, "v": None}], "similarity_score": 0.5, "canonical_record": None},
]
OUT["resolve_duplicates"] = [
    {"groups": GROUPS, "strategy": s,
     "result": run(DuplicateDetector().resolve_duplicates, [DuplicateGroup(**g) for g in GROUPS], s)}
    for s in ("keep_first", "merge", "other")
]
OUT["resolve_duplicates_empty_records"] = run(
    DuplicateDetector().resolve_duplicates, [DuplicateGroup(records=[], similarity_score=0.0)], "other")

dv = DataValidator()
OUT["validate_dataset"] = [
    {"dataset": d, "schema": s, "result": run(dv.validate_dataset, d, s)}
    for d in (DATASET[:3], [], [{"name": None}, {"name": 5, "tags": "x"}])
    for s in (None, SCHEMA, {"fields": {"name": {"required": True}, "tags": {"type": "list"}}})
]
OUT["validate_record"] = [
    {"record": r, "schema": s, "result": run(dv.validate_record, r, s)}
    for r in ({"name": "x", "age": True}, {"age": 1.5}, {})
    for s in (None, SCHEMA, {"fields": {"age": {"type": {"int": 1}}}}, {"fields": {"age": {"type": "unknown"}}})
]
OUT["check_data_types"] = [
    {"data": v, "types": t, "result": run(dv.check_data_types, v, t)}
    for v in ("s", 1, 1.0, True, None, [1], {"a": 1})
    for t in ("str", "int", "float", "bool", "list", "dict", "unknown", ["int", "float"], ["list", "str"])
]

mv = MissingValueHandler()
MISSING = [
    {"a": 1, "b": "x", "c": None},
    {"a": None, "b": "N/A", "c": 2.5},
    {"a": 3, "b": "", "c": True},
    {"a": 10, "b": "null", "d": "NULL"},
]
OUT["identify_missing_values"] = [
    {"dataset": d, "result": run(mv.identify_missing_values, d)} for d in (MISSING, [], [{"x": 0}])
]
OUT["handle_missing_values"] = [
    {"strategy": s, "options": o, "result": run(mv.handle_missing_values, MISSING, s, **o)}
    for s, o in [("remove", {}), ("fill", {}), ("fill", {"fill_value": -1}), ("impute", {}),
                 ("impute", {"method": "median"}), ("impute", {"method": "zero"}), ("impute", {"method": "x"}),
                 ("bogus", {})]
]
IMPUTE = [{"v": 1}, {"v": 2.5}, {"v": True}, {"v": None}, {"v": 7}, {"w": "N/A"}]
OUT["impute_values"] = [
    {"dataset": d, "method": m, "result": run(mv.impute_values, d, m)}
    for d in (IMPUTE, [], [{"v": 1}, {"v": 3}, {"v": None}])
    for m in ("mean", "median", "zero", "other")
]
mv0 = MissingValueHandler(missing_values=[0, "-"])
OUT["custom_missing_values"] = {
    "identify": run(mv0.identify_missing_values, [{"a": 0, "b": False}, {"a": "-", "b": None}, {"a": 1, "b": 2}]),
    "remove": run(mv0.handle_missing_values, [{"a": 0}, {"a": False}, {"a": 1.0}, {"a": None}], "remove"),
}

# ---------------------------------------------------------------------------------------
# Language detection (langdetect is not installed in the oracle)

ld = N.LanguageDetector()
ld_fr = N.LanguageDetector(default_language="fr", min_confidence=0.0)
LANG_TEXTS = ["This is a simple English sentence.", "Ceci est une phrase française simple.", "Hi", "",
              "          x", None]
OUT["language_unavailable"] = {
    "available": N.language_detector.LANGDETECT_AVAILABLE,
    "detect": [{"text": t, "result": run(ld.detect, t)} for t in LANG_TEXTS],
    "detect_fr": [{"text": t, "result": run(ld_fr.detect, t)} for t in LANG_TEXTS],
    "detect_with_confidence": [{"text": t, "result": run(ld.detect_with_confidence, t)} for t in LANG_TEXTS],
    "detect_multiple": [{"text": t, "top_n": n, "result": run(ld.detect_multiple, t, n)}
                        for t in LANG_TEXTS for n in (1, 3)],
    "detect_batch": run(ld.detect_batch, LANG_TEXTS[:4]),
    "detect_batch_with_confidence": run(ld.detect_batch_with_confidence, LANG_TEXTS[:4]),
    "is_language": [{"text": t, "target": g, "min": m, "result": run(ld.is_language, t, g, m)}
                    for t in LANG_TEXTS[:3] for g in ("en", "fr") for m in (None, 0.0)],
    "is_language_fr": [{"text": t, "target": g, "result": run(ld_fr.is_language, t, g)}
                       for t in LANG_TEXTS[:3] for g in ("en", "fr")],
}


class _FakeLanguage:
    def __init__(self, lang, prob):
        self.lang, self.prob = lang, prob


FAKE_LANGS = [("fr", 0.4), ("it", 0.35), ("es", 0.25)]
import semantica.normalize.language_detector as LDM  # noqa: E402

with patch.object(LDM, "LANGDETECT_AVAILABLE", True), \
        patch.object(LDM, "detect", lambda text: FAKE_LANGS[0][0], create=True), \
        patch.object(LDM, "detect_langs", lambda text: [_FakeLanguage(*x) for x in FAKE_LANGS], create=True):
    fake_ld = N.LanguageDetector()
    OUT["language_fake_backend"] = {
        "languages": FAKE_LANGS,
        "detect": run(fake_ld.detect, "long enough text"),
        "detect_with_confidence": run(fake_ld.detect_with_confidence, "long enough text"),
        "detect_multiple": run(fake_ld.detect_multiple, "long enough text", 2),
        "is_language_fr": run(fake_ld.is_language, "long enough text", "fr"),
        "is_language_en": run(fake_ld.is_language, "long enough text", "en", 0.3),
        "detect_short": run(fake_ld.detect, "short"),
    }

OUT["get_language_name"] = [
    {"code": c, "result": run(ld.get_language_name, c)}
    for c in ["en", "fr", "de", "es", "it", "pt", "ru", "zh", "ja", "ko", "ar", "hi", "nl", "sv", "da", "no",
              "fi", "pl", "tr", "cs", "hu", "ro", "th", "vi", "xx", "zh-cn", ""]
]

# ---------------------------------------------------------------------------------------
# Encodings

eh = N.EncodingHandler()
FRENCH = ("C'était déjà l'été à Montréal; François dégustait un café "
          "près de l'hôtel. ") * 4
DETECT_INPUTS = [
    "",
    "Hello",
    "Héllo wörld",
    {"bytes": list(b"Hello")},
    {"bytes": list(FRENCH.encode("utf-8"))},
    {"bytes": list(FRENCH.encode("latin-1"))},
    {"bytes": list(b"\xef\xbb\xbfHello")},
    {"bytes": list("﻿Hi".encode("utf-16-le"))},
    {"bytes": list("﻿Hi".encode("utf-16-be"))},
    {"bytes": list("﻿Hi".encode("utf-32-le"))},
    {"bytes": list(b"\x00\x01\x02\x03")},
    {"bytes": list(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3")},
    {"bytes": list(b"BMW is a car")},
    {"bytes": list(b"Hello\x00World")},
    {"bytes": list(b"x" * 100 + b"\x00")},
    {"bytes": list(b"line one\nline two\r\n\ttabbed")},
    {"bytes": list(b"\x89PNG\r\n\x1a\n\x00\x00")},
    {"bytes": list("naïve — 日本".encode("utf-8") + b"\xe6\x97")},
]
OUT["detect"] = [{"data": d, "result": run(eh.detect, data_arg(d))} for d in DETECT_INPUTS]

os.makedirs(SCRATCH, exist_ok=True)
FILES = {
    "utf8.txt": "Café crème\r\nline two\rline three\n".encode("utf-8"),
    "latin1.txt": FRENCH.encode("latin-1"),
    "ascii.txt": b"plain ascii\r\ntext",
    "empty.txt": b"",
    "bom.txt": b"\xef\xbb\xbfwith bom\n",
}
for name, content in FILES.items():
    with open(os.path.join(SCRATCH, name), "wb") as f:
        f.write(content)


def scrub(result):
    text = json.dumps(result)
    return json.loads(text.replace(SCRATCH, "<dir>"))


OUT["files"] = {name: list(content) for name, content in FILES.items()}
OUT["detect_file"] = [
    {"file": name, "sample_size": s, "result": scrub(run(eh.detect_file, os.path.join(SCRATCH, name), s))}
    for name in list(FILES) + ["missing.txt"]
    for s in (10000, 4)
]
convert_results = []
for name in list(FILES) + ["missing.txt"]:
    out_path = os.path.join(SCRATCH, "out", name + ".utf8")
    result = scrub(run(eh.convert_file_to_utf8, os.path.join(SCRATCH, name), out_path))
    written = None
    if os.path.exists(out_path):
        with open(out_path, "rb") as f:
            written = list(f.read())
    convert_results.append({"file": name, "result": result, "written": written})
OUT["convert_file_to_utf8"] = convert_results
OUT["convert_file_to_utf8_no_output"] = scrub(run(eh.convert_file_to_utf8, os.path.join(SCRATCH, "utf8.txt")))

CONVERT_CASES = [
    {"data": "already text é", "source": None},
    {"data": {"bytes": list("Héllò Wörld".encode("latin-1"))}, "source": "latin-1"},
    {"data": {"bytes": list("café".encode("utf-8"))}, "source": None},
    {"data": {"bytes": list(FRENCH.encode("latin-1"))}, "source": None},
    {"data": {"bytes": list(b"abc")}, "source": "bogus"},
    {"data": {"bytes": list(b"\x00\x01\x02\x03")}, "source": None},
    {"data": {"bytes": list(b"\x93hi\x94")}, "source": "cp1252"},
    {"data": {"bytes": list(b"caf\xe9")}, "source": "ascii"},
    {"data": {"bytes": list(b"\xff\xfeh\x00i\x00")}, "source": "utf-16"},
    {"data": {"bytes": list(b"\xe9\x81")}, "source": "cp1252"},
]
OUT["convert_to_utf8"] = [
    dict(case, result=run(eh.convert_to_utf8, data_arg(case["data"]), case["source"])) for case in CONVERT_CASES
]
OUT["remove_bom"] = [
    {"data": d, "result": run(eh.remove_bom, data_arg(d))}
    for d in [{"bytes": list(b"\xef\xbb\xbfHello")}, {"bytes": list(b"\xff\xfeH\x00")},
              {"bytes": list(b"\xfe\xff\x00H")}, {"bytes": list(b"plain")}, {"bytes": []},
              "﻿Hello", "Hello﻿", ""]
]
OUT["handle_encoding_error"] = [
    {"data": d, "strategy": s, "result": run(eh.handle_encoding_error, data_arg(d), s)}
    for d in [{"bytes": list("café".encode("utf-8"))}, {"bytes": list(FRENCH.encode("latin-1"))},
              {"bytes": list(b"\x00\x01\x02\xff")}, {"bytes": list(b"plain")}]
    for s in ("replace", "ignore", "strict", "backslashreplace", "bogus")
]
eh_strict = N.EncodingHandler(fallback_encodings=["ascii"], default_encoding="ascii")
OUT["handle_encoding_error_strict_fallbacks"] = [
    {"data": d, "strategy": s, "result": run(eh_strict.handle_encoding_error, data_arg(d), s)}
    for d in [{"bytes": list(b"\x00\x01\x02\xff")}, {"bytes": list(b"ok")}]
    for s in ("strict", "replace")
]
OUT["validate_encoding"] = [
    {"data": d, "encoding": e, "result": run(eh.validate_encoding, data_arg(d), e)}
    for d, e in [("Hello", "utf-8"), ("Héllo", "ascii"), ("Héllo", "latin-1"), ("€", "latin-1"),
                 ("€", "cp1252"), ({"bytes": [0xFF]}, "utf-8"), ({"bytes": [0xE9]}, "latin-1"),
                 ("x", "bogus"), ({"bytes": [0x81]}, "cp1252"), ({"bytes": [0x81]}, "latin-1"),
                 ("日本", "utf-16"), ({"bytes": list("é".encode("utf-8"))}, "ascii"),
                 ("日本", "shift_jis"), ("é", "Windows-1252")]
]
EH_CONFIG = N.EncodingHandler(default_encoding="latin-1", fallback_encodings=["utf-8"])
OUT["encoding_config"] = {
    "default_encoding": EH_CONFIG.default_encoding,
    "fallback_encodings": EH_CONFIG.fallback_encodings,
    "detect_empty": run(EH_CONFIG.detect, b""),
    "default": {"default_encoding": eh.default_encoding, "fallback_encodings": eh.fallback_encodings},
}

# chardet's own answers for every byte input above (to keep the recorded table honest).
CHARDET_INPUTS = [data_arg(d) for d in DETECT_INPUTS if isinstance(d, dict)] + [
    data_arg(c["data"]) for c in CONVERT_CASES if isinstance(c["data"], dict)]
OUT["chardet_raw"] = [
    {"sha256": hashlib.sha256(b).hexdigest(), "result": conv(chardet.detect(b))} for b in CHARDET_INPUTS if b
]

# ---------------------------------------------------------------------------------------
# Convenience functions

OUT["methods"] = {
    "normalize_text": [
        {"text": t, "kwargs": k, "result": run(M.normalize_text, t, **k)}
        for t in ["Hello   World", "  “Quoted” — TEXT  ", None]
        for k in ({}, {"case": "lower"}, {"case": "title", "normalize_diacritics": True, "remove_diacritics": True})
    ],
    "clean_text": [
        {"text": t, "kwargs": k, "result": run(M.clean_text, t, **k)}
        for t in ["<p>Hi</p>  there &amp; here", ""]
        for k in ({}, {"remove_special_chars": True})
    ],
    "normalize_entity": [
        {"name": n, "entity_type": e, "kwargs": k, "result": run(M.normalize_entity, n, e, **k)}
        for n in ["  john   doe  ", "dr. jane roe", "j. doe"]
        for e in (None, "Person")
        for k in ({}, {"alias_map": {"j. doe": "John Doe"}}, {"alias_map": {"j. doe": "John Doe"}, "resolve_aliases": False})
    ],
    "resolve_aliases": [
        {"name": n, "kwargs": k, "result": run(M.resolve_aliases, n, None, **k)}
        for n in ["J. Doe", "nobody"]
        for k in ({}, {"alias_map": {"j. doe": "John Doe"}})
    ],
    "disambiguate_entity": [
        {"name": n, "context": c, "result": run(M.disambiguate_entity, n, **c)}
        for n in ["Apple"]
        for c in ({}, {"entity_type": "Organization", "context": "tech"})
    ],
    "normalize_date": [
        {"input": d, "format": f, "timezone": z, "result": run(M.normalize_date, date_arg(d), f, z)}
        for d in ["2023-01-15", "2023-01-15T10:30:00+05:30", "yesterday", 5]
        for f in ("ISO8601", "date")
        for z in ("UTC", "Asia/Kolkata")
    ],
    "normalize_time": [{"input": d, "result": run(M.normalize_time, d)} for d in ["10:30 PM", "bad", 7]],
    "normalize_number": [{"input": x, "result": run(M.normalize_number, x)} for x in ["2.5k", "12%", "x", 3]],
    "normalize_quantity": [{"input": q, "result": run(M.normalize_quantity, q)} for q in ["10 meters", "none"]],
    "clean_data": [
        {"flags": f, "kwargs": k, "result": run(M.clean_data, DATASET, *f, **k)}
        for f in ([True, True, True], [True, False, False])
        for k in ({}, {"duplicate_criteria": {"key_fields": ["name"]}, "missing_strategy": "fill"})
    ],
    "detect_duplicates": [
        {"threshold": t, "key_fields": kf, "result": run(M.detect_duplicates, DATASET, t, kf)}
        for t in (None, 0.9)
        for kf in (None, ["email"])
    ],
    "detect_language": [
        {"text": t, "method": m, "result": run(M.detect_language, t, m)}
        for t, m in [("This is an English sentence for testing.", "default"), ("Hi", "default"),
                     ("Hi", "confidence"), ("", "batch")]
    ],
    "handle_encoding": [
        {"data": d, "operation": op, "kwargs": k, "result": run(M.handle_encoding, data_arg(d), op, **k)}
        for d, op, k in [
            ({"bytes": list(b"Hello")}, "detect", {}),
            ("Héllo", "detect", {}),
            ({"bytes": list(b"\xef\xbb\xbfHi")}, "remove_bom", {}),
            ("﻿Hi", "remove_bom", {}),
            ({"bytes": list(b"x")}, "remove_bom", {"foo": 1}),
            ({"bytes": list("café".encode("utf-8"))}, "convert", {}),
            ({"bytes": list(b"x")}, "convert", {"source_encoding": "latin-1"}),
            ("x", "bogus", {}),
        ]
    ],
    "get_normalize_method": run(M.get_normalize_method, "text", "nope"),
    "list_available_methods": run(M.list_available_methods),
    "list_available_methods_text": run(M.list_available_methods, "text"),
}

# Keyword arguments persist in an existing method configuration (config.update(kwargs)).
normalize_config.set_method_config("number", precision=2)
OUT["method_config_mutation"] = {
    "before": conv(normalize_config.get_method_config("number")),
    "call": run(M.normalize_number, "5", foo=1),
    "after": conv(normalize_config.get_method_config("number")),
    "absent_before": conv(normalize_config.get_method_config("entity")),
    "absent_call": run(M.normalize_entity, "x", None, bar=2),
    "absent_after": conv(normalize_config.get_method_config("entity")),
}

# ---------------------------------------------------------------------------------------
# Registry

reg = MethodRegistry()
registry_trace = {"initial": conv(method_registry.list_all())}
reg.register("text", "upper", lambda text, **kw: text.upper())
reg.register("custom_task", "noop", lambda *a, **k: None)
registry_trace["after_register"] = conv(method_registry.list_all())
registry_trace["get_upper_is_none"] = method_registry.get("text", "upper") is None
registry_trace["get_missing_is_none"] = method_registry.get("text", "missing") is None
registry_trace["get_unknown_task_is_none"] = method_registry.get("nope", "upper") is None
registry_trace["list_text"] = conv(method_registry.list_all("text"))
registry_trace["list_unknown"] = conv(method_registry.list_all("unknown"))
registry_trace["list_empty_string"] = conv(method_registry.list_all(""))
registry_trace["normalize_text_upper"] = run(M.normalize_text, "hello", "upper")
registry_trace["get_normalize_method_is_none"] = M.get_normalize_method("text", "upper") is None
method_registry.unregister("text", "upper")
method_registry.unregister("text", "never-registered")
method_registry.unregister("no-task", "x")
registry_trace["after_unregister"] = conv(method_registry.list_all())
reg.register("text", "a", lambda t, **k: t)
reg.register("entity", "b", lambda t, *a, **k: t)
method_registry.clear("text")
registry_trace["after_clear_text"] = conv(method_registry.list_all())
method_registry.clear("not-a-task")
method_registry.clear()
registry_trace["after_clear_all"] = conv(method_registry.list_all())
registry_trace["list_available_methods"] = conv(M.list_available_methods())
OUT["registry"] = registry_trace

# ---------------------------------------------------------------------------------------
# Configuration

CONFIG_FILES = {
    "cfg.yaml": "normalize:\n  unicode_form: NFKC\n  case: lower\n  max_length: 10\nnormalize_methods:\n  text:\n    case: upper\n",
    "cfg.yml": "normalize: {timezone: UTC}\n",
    "cfg.json": json.dumps({"normalize": {"date_format": "%Y", "flag": True}, "normalize_methods": {"date": {"format": "date"}}}),
    "cfg.toml": "[normalize]\ndefault_language = \"de\"\nratio = 0.5\n\n[normalize_methods.number]\nprecision = 3\n",
    "empty.yaml": "",
    "list.json": "[1, 2]",
    "bad.json": "{not json",
    "cfg.txt": "normalize: {x: 1}",
}
for name, content in CONFIG_FILES.items():
    with open(os.path.join(SCRATCH, name), "w") as f:
        f.write(content)
OUT["config_files"] = CONFIG_FILES
OUT["config_load"] = [
    {"file": name, "result": run(lambda p: NormalizeConfig(p).get_all(), os.path.join(SCRATCH, name))}
    for name in list(CONFIG_FILES) + ["missing.yaml"]
]
OUT["config_none"] = run(lambda: NormalizeConfig().get_all())
cfg = NormalizeConfig()
cfg.set("unicode_form", "NFKD")
cfg.set("count", 3)
cfg.set_method_config("text", case="lower", unicode_form="NFC")
cfg.set_method_config("date")
OUT["config_ops"] = {
    "get_set": run(cfg.get, "unicode_form"),
    "get_count": run(cfg.get, "count", 0),
    "get_missing": run(cfg.get, "missing"),
    "get_missing_default": run(cfg.get, "missing", 5),
    "get_missing_default_str": run(cfg.get, "missing", "dflt"),
    "method_text": run(cfg.get_method_config, "text"),
    "method_date": run(cfg.get_method_config, "date"),
    "method_missing": run(cfg.get_method_config, "nope"),
    "all": run(cfg.get_all),
}

# ---------------------------------------------------------------------------------------
# Export surface (`from semantica.normalize import X`), mirrored by normalize_init.rs

_aliases = {"alias_map": {"j. doe": "John Doe"}}
_init_records = [{"a": 1}, {"a": 1}, {"a": None}]
OUT["init_exports"] = {
    "entity_normalizer": run(N.EntityNormalizer(**_aliases).normalize_entity, "  john   doe  ", "Person", True),
    "alias_resolver": run(N.AliasResolver(**_aliases).resolve_aliases, "J. Doe"),
    "convert_to_utc": run(N.TimeZoneNormalizer().convert_to_utc, FixedDateTime(2023, 1, 1, 12, 0, 0)),
    "relative_yesterday": run(N.RelativeDateProcessor().process_relative_expression, "yesterday"),
    "normalize_number": run(N.NumberNormalizer().normalize_number, "1,234.56"),
    "normalize_unit": run(N.UnitConverter().normalize_unit, "km"),
    "validate_currency_code": run(N.CurrencyNormalizer().validate_currency_code, "usd"),
    "convert_to_scientific": run(N.ScientificNotationHandler().convert_to_scientific, 1500.0),
    "clean_data": run(N.DataCleaner().clean_data, _init_records),
    "detect_duplicates_count": run(lambda: len(N.DuplicateDetector().detect_duplicates(_init_records))),
    "validate_dataset_valid": run(lambda: N.DataValidator().validate_dataset(_init_records).valid),
    "fill": run(N.MissingValueHandler().handle_missing_values, _init_records, "fill"),
    "detect_language_hi": run(N.LanguageDetector().detect, "Hi"),
    "language_name_fr": run(N.LanguageDetector().get_language_name, "fr"),
    "detect_hello": run(N.EncodingHandler().detect, b"Hello"),
    "normalize_text": run(N.normalize_text, "Hello   World"),
    "clean_text": run(N.clean_text, "<b>x</b>"),
    "normalize_entity": run(N.normalize_entity, "  john   doe  ", "Person"),
    "resolve_aliases": run(N.resolve_aliases, "x"),
    "disambiguate_entity": run(N.disambiguate_entity, "Apple"),
    "normalize_date": run(N.normalize_date, "2023-01-01"),
    "normalize_time": run(N.normalize_time, "10:30 PM"),
    "normalize_number_fn": run(N.normalize_number, "2.5k"),
    "normalize_quantity": run(N.normalize_quantity, "1 km"),
    "clean_data_fn": run(lambda: len(N.clean_data([{"a": 1}, {"a": 1}]))),
    "detect_duplicates_fn": run(lambda: len(N.detect_duplicates([{"a": 1}, {"a": 1}]))),
    "detect_language_fn": run(N.detect_language, "Hi"),
    "handle_encoding_fn": run(N.handle_encoding, b"Hello"),
    "get_normalize_method_is_none": N.get_normalize_method("text", "nope") is None,
    "list_available_methods_date": run(N.list_available_methods, "date"),
    "registry_language": run(N.method_registry.list_all, "language"),
    "config_set_get": run(lambda: (lambda c: (c.set("k", "v"), c.get("k"))[1])(N.NormalizeConfig())),
    "global_method_config": run(N.normalize_config.get_method_config, "never-configured"),
}

# ---------------------------------------------------------------------------------------
# normalize_provenance: the constructor imports `.normalizer`, which does not exist. With a
# stand-in module (the Rust tests inject the same normalizer) and a fixed `utcnow`, a run
# records one `normalized_data` entity in an in-memory ProvenanceManager.

import semantica.normalize.normalize_provenance as NP  # noqa: E402

OUT["normalize_provenance_new"] = [
    {"kwargs": k, "result": run(NP.NormalizerWithProvenance, **k)}
    for k in ({}, {"provenance": True}, {"provenance": False, "agent_id": "a", "x": 1})
]


class _StandInNormalizer:
    def __init__(self, **config):
        self.config = config

    def normalize(self, data, **kwargs):
        return {"normalized": data, "kwargs": kwargs}


class _UtcClock:
    """datetime.utcnow() -> 2024-01-02T00:30:00 + n * 1.5 s."""

    n = 0

    @classmethod
    def utcnow(cls):
        value = datetime(2024, 1, 2, 0, 30) + timedelta(seconds=1.5 * cls.n)
        cls.n += 1
        return value


_stand_in = types.ModuleType("semantica.normalize.normalizer")
_stand_in.Normalizer = _StandInNormalizer
ENTRY_FIELDS = ["entity_type", "agent_id", "agent_type", "is_automated", "source_document", "metadata",
                "activity_started_at_time", "activity_ended_at_time"]
PROVENANCE_SCENARIOS = [
    ({}, "Hello", {}, {"method": "unicode"}),
    ({"provenance": True}, "Hello", {}, {"method": "unicode"}),
    ({"provenance": True, "agent_id": "norm-agent", "is_automated": False, "lang": "en"}, [1, 2],
     {"source": "doc.txt"}, {}),
    ({"provenance": True, "agent_id": ""}, {"k": "v"}, {"source": ""}, {"case": "lower", "method": 7}),
]
provenance_runs = []
with patch.dict(sys.modules, {"semantica.normalize.normalizer": _stand_in}), patch.object(NP, "datetime", _UtcClock):
    for init_kw, data, source_kw, call_kw in PROVENANCE_SCENARIOS:
        _UtcClock.n = 0
        wrapper = NP.NormalizerWithProvenance(**init_kw)
        result = wrapper.normalize(data, **source_kw, **call_kw)
        manager = wrapper._prov_manager
        entries = manager.storage.retrieve_all() if manager else []
        provenance_runs.append({
            "init": init_kw,
            "data": data,
            "source": source_kw.get("source"),
            "kwargs": call_kw,
            "result": conv(result),
            "provenance": wrapper.provenance,
            "agent_id": wrapper._agent_id,
            "is_automated": wrapper._is_automated,
            "has_manager": manager is not None,
            "normalizer_config": conv(wrapper.config),
            "entries": [
                dict({f: conv(getattr(e, f)) for f in ENTRY_FIELDS},
                     entity_id_prefix=e.entity_id[:10], entity_id_len=len(e.entity_id),
                     entity_id_hex=all(c in "0123456789abcdef" for c in e.entity_id[10:]))
                for e in entries
            ],
        })
OUT["normalize_provenance_runs"] = provenance_runs


class _FailingNormalizer:
    def __init__(self, **config):
        pass

    def normalize(self, data, **kwargs):
        raise ValueError("cannot normalize")


_failing = types.ModuleType("semantica.normalize.normalizer")
_failing.Normalizer = _FailingNormalizer
with patch.dict(sys.modules, {"semantica.normalize.normalizer": _failing}):
    failing_wrapper = NP.NormalizerWithProvenance(provenance=True)
    OUT["normalize_provenance_failing"] = {
        "result": run(failing_wrapper.normalize, "x", "doc"),
        "entries_after": len(failing_wrapper._prov_manager.storage.retrieve_all()),
    }

shutil.rmtree(SCRATCH, ignore_errors=True)
clock.close()

with open(sys.argv[1], "w") as f:
    json.dump(conv(OUT), f, indent=1, ensure_ascii=False, sort_keys=False, allow_nan=False)
    f.write("\n")
