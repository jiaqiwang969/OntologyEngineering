"""Generate tests/fixtures/oracle.json from the original Semantica provenance module (the oracle).

Every expected value used by the sem-provenance characterisation tests comes from running the
original code here. Run from var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python -I \
        <crate>/tests/fixtures/generate_oracle.py <crate>/tests/fixtures/oracle.json

Time is made deterministic by replacing `datetime` in the provenance modules with a class whose
`utcnow()` returns 2026-01-01T00:00:00 + n * 1.25 s for the n-th call since the last
`CLOCK.reset()` (the Rust tests install the same sequence with `sem_provenance::clock`).
Blank-node ids and uuid4 chain ids stay random and are normalised by the tests.

The script only computes values (no network, no credentials); SQLite files live in a temporary
directory that is removed afterwards.
"""

import json
import logging
import os
import sqlite3
import sys
import tempfile
from datetime import datetime, timedelta

sys.path.insert(0, os.getcwd())
logging.disable(logging.CRITICAL)

import semantica.provenance as P  # noqa: E402
import semantica.provenance.bridge_axiom as BA  # noqa: E402
import semantica.provenance.integrity as I  # noqa: E402
import semantica.provenance.manager as M  # noqa: E402
import semantica.provenance.schemas as S  # noqa: E402
from semantica.provenance import (  # noqa: E402
    ActivityRecord,
    AgentRecord,
    InMemoryStorage,
    Invalidation,
    ProvenanceEntry,
    ProvenanceManager,
    SourceReference,
    SQLiteStorage,
)
from semantica.provenance.schemas import PropertySource  # noqa: E402

OUT = {}
START = datetime(2026, 1, 1, 0, 0, 0)
STEP = timedelta(microseconds=1_250_000)


class Clock:
    def __init__(self):
        self.n = 0

    def reset(self):
        self.n = 0

    def tick(self):
        value = START + STEP * self.n
        self.n += 1
        return value


CLOCK = Clock()


class FakeDateTime(datetime):
    @classmethod
    def utcnow(cls):
        return CLOCK.tick()


S.datetime = FakeDateTime
M.datetime = FakeDateTime
BA.datetime = FakeDateTime


def err(e):
    return {"error": type(e).__name__, "message": str(e)}


def call(fn, *args, **kwargs):
    try:
        return {"ok": fn(*args, **kwargs)}
    except Exception as e:  # noqa: BLE001
        return err(e)


def dicts(entries):
    return [e.to_dict() for e in entries]


def ids(entries):
    return [e.entity_id for e in entries]


def exports(m, **kwargs):
    out = {}
    for fmt in ("turtle", "ntriples", "jsonld"):
        try:
            out[fmt] = m.export_prov(format=fmt, **kwargs)
        except Exception as e:  # noqa: BLE001
            out[fmt] = err(e)
    return out


# ---------------------------------------------------------------------------------------------
# schemas
# ---------------------------------------------------------------------------------------------
def schemas():
    out = {}
    CLOCK.reset()
    entry = ProvenanceEntry(entity_id="e1", entity_type="entity", activity_id="act")
    out["default_entry"] = entry.to_dict()
    out["default_entry_repr"] = repr(entry)
    full = ProvenanceEntry(
        entity_id="e1", entity_type="entity", activity_id="act", agent_id="alice",
        agent_type="person", is_automated=False, role="approver", source_document="doc",
        source_location="p. 4", source_quote="quote", timestamp="2026-01-01T00:00:00",
        first_seen="f", last_updated="l", confidence=0.92, checksum="c", sequence_id=5,
        previous_checksum="abc123", parent_entity_id="p", used_entities=["u1", "u2"],
        previous_version_id="e1:v:1", derived_from_id="source_entity",
        activity_started_at_time="2026-01-01T00:00:00", activity_ended_at_time="2026-01-01T00:00:05",
        acted_on_behalf_of="org1", informed_by_activities=["ingest_activity"], valid_from="2026-01-01",
        valid_until="2026-06-01", revision_type="correction", supersedes="old_claim",
        bundle_id="ingestion_run_1", invalidated=True, invalidated_at_time="2026-01-02T00:00:00",
        invalidated_by="reviewer_jane", invalidation_reason="retracted", start_index=3, end_index=9,
        credibility=0.5, metadata={"k": [1, 2.5, None, True]}, version="2.0",
    )
    out["full_entry"] = full.to_dict()
    CLOCK.reset()
    out["from_dict_minimal"] = ProvenanceEntry.from_dict(
        {"entity_id": "entity_1", "entity_type": "entity", "activity_id": "extraction",
         "agent_id": "semantica", "source_document": "doc_1", "confidence": 0.9}).to_dict()
    errors = {}
    for name, data in {
        "unexpected": {"entity_id": "e", "entity_type": "t", "activity_id": "a", "bogus": 1, "other": 2},
        "missing_one": {"entity_id": "e", "entity_type": "t"},
        "missing_two": {"entity_id": "e"},
        "missing_three": {},
        "unexpected_before_missing": {"bogus": 1},
    }.items():
        errors[name] = call(lambda d=data: ProvenanceEntry.from_dict(dict(d)).to_dict())
    out["entry_from_dict_errors"] = errors

    out["source_basic"] = SourceReference(document="doc_1", page=1).to_dict()
    ts = SourceReference(document="d", page=4, section="Table S4", line=7,
                         timestamp=datetime(2026, 1, 2, 3, 4, 5, 6), confidence=0.92,
                         metadata={"m": 1})
    out["source_full"] = ts.to_dict()
    out["source_round_trip"] = SourceReference.from_dict(ts.to_dict()).to_dict()
    out["source_from_dict_errors"] = {
        "missing": call(lambda: SourceReference.from_dict({"page": 1}).to_dict()),
        "unexpected": call(lambda: SourceReference.from_dict({"document": "d", "pages": 1}).to_dict()),
        "bad_timestamp": call(lambda: SourceReference.from_dict({"document": "d", "timestamp": "yesterday"}).to_dict()),
    }
    prop = PropertySource(property_name="biomass_increase", value="463%",
                          sources=[SourceReference(document="doc_1")], entity_id="cabo_pulmo_mpa",
                          metadata={"x": 1})
    out["property_source"] = prop.to_dict()
    out["property_source_round_trip"] = PropertySource.from_dict(prop.to_dict()).to_dict()
    out["property_source_errors"] = {
        "missing": call(lambda: PropertySource.from_dict({"property_name": "p"}).to_dict()),
        "unexpected": call(lambda: PropertySource.from_dict({"property_name": "p", "value": 1, "x": 2}).to_dict()),
    }
    out["agent"] = AgentRecord(id="reviewer_jane", agent_type="person", is_automated=False, name="Jane").to_dict()
    out["agent_default"] = AgentRecord(id="bot1").to_dict()
    out["activity"] = ActivityRecord(id="run", activity_type="extraction", started_at_time="t0",
                                     ended_at_time="t1", metadata={"a": 1}).to_dict()
    out["activity_default"] = ActivityRecord(id="act1").to_dict()
    out["invalidation"] = Invalidation(entity_id="e1", invalidated_at_time="2026-01-01T00:00:00",
                                       invalidated_by="reviewer_jane", reason="retracted").to_dict()
    def real_from_dict(data):
        # SourceReference.from_dict builds the timestamp with the module's `datetime`; use the
        # real class (not the clock-patching subclass) so the repr names `datetime.datetime`.
        S.datetime = datetime
        try:
            return SourceReference.from_dict(data)
        finally:
            S.datetime = FakeDateTime

    out["reprs"] = {
        "agent": repr(AgentRecord(id="reviewer_jane", agent_type="person", is_automated=False, name="Jane")),
        "activity": repr(ActivityRecord(id="run", activity_type="extraction", started_at_time="t0",
                                        ended_at_time="t1", metadata={"a": 1})),
        "invalidation": repr(Invalidation(entity_id="e1", invalidated_at_time="2026-01-01T00:00:00",
                                          invalidated_by="reviewer_jane", reason="retracted")),
        "source_full": repr(ts),
        "source_aware": repr(real_from_dict({"document": "d", "timestamp": "2026-01-01T00:00:00+05:30"})),
        "source_utc": repr(real_from_dict({"document": "d", "timestamp": "2026-01-01T10:20:00Z"})),
        "source_negative": repr(real_from_dict({"document": "d", "timestamp": "2026-01-01T10:20:30-08:00"})),
        "property_source": repr(prop),
    }
    out["record_errors"] = {
        "agent_missing": call(lambda: AgentRecord.from_dict({}).to_dict()),
        "agent_unexpected": call(lambda: AgentRecord.from_dict({"id": "a", "kind": "x"}).to_dict()),
        "activity_missing": call(lambda: ActivityRecord.from_dict({"activity_type": "x"}).to_dict()),
        "invalidation_missing": call(lambda: Invalidation.from_dict({"entity_id": "e"}).to_dict()),
        "invalidation_missing_all": call(lambda: Invalidation.from_dict({}).to_dict()),
    }
    return out


# ---------------------------------------------------------------------------------------------
# datetime.fromisoformat (via SourceReference.from_dict)
# ---------------------------------------------------------------------------------------------
ISO_CASES = [
    "2026-01-01T00:00:00", "2026-01-01T00:00:00.000000", "2026-01-01T00:00:00.5",
    "2026-01-01T00:00:00.123456789", "2026-01-01T00:00:00Z", "2026-01-01T00:00:00+05:30",
    "2026-01-01T00:00:00-00:00", "2026-01-01", "t0", "2026-13-01T00:00:00", "2026-01-01 00:00:00",
    "20260101T000000", "2026-01-01T00:00", "2026-01-01T24:00:00", " 2026-01-01T00:00:00",
    "2026-01-01T00:00:00 ", "0001-01-01T00:00:00", "2026-02-30T00:00:00", "2026-01-01T00:00:00.1234567",
    "-2026-01-01T00:00:00", "2026-01-01T00:00:60", "2026-01-01T00:00:00+14:00",
    "2026-01-01T00:00:00+15:00", "2026-1-1T0:0:0", "2026-01-01T00:00:00.", "12026-01-01T00:00:00",
    "２０２６-01-01T00:00:00", "2026-01-01T00:00:00,5", "2026-01-01T00:00:00+0530",
    "2026-01-01T00:00:00.000001Z", "2026-W01", "2026-W01-3", "2026W013", "2026-W53-1", "2020-W53-7",
    "2026-001", "202601", "2026-01", "2026-01-01x12:30", "2026-01-01T12", "2026-01-01T1230",
    "2026-01-01T123045", "2026-01-01T12:3045", "2026-01-01T12:30:45.12", "2026-01-01T12:30:45+01",
    "2026-01-01T12:30:45+0130", "2026-01-01T12:30:45+01:30:15", "2026-01-01T12:30:45+01:30:15.5",
    "2026-01-01T12:30:45-08:00", "2026-01-01T12:30:45+1", "2026-01-01T12:30:45+123",
    "2026-01-01T12:30:45Z5", "2026-01-01T12:30:45.123abc", "2026-01-01T12:30:45.1234567abc",
    "2026-01-01T", "2026-01-01T1", "20260101", "2026-0101", "202601-01", "2026-02-29", "2024-02-29",
    "9999-12-31T23:59:59.999999", "0000-01-01", "2026-01-01T00:00:00+24:00", "2026-01-01T00:00:00-23:59",
    "2026-01-01T12:30:45.000000+00:00", "2026-01-01T12:30:45+00:00:00.000001", "2026-01-01T12:30+05",
]


def isoformat_cases():
    out = []
    for case in ISO_CASES:
        try:
            ref = SourceReference.from_dict({"document": "d", "timestamp": case})
            out.append({"input": case, "ok": ref.to_dict()["timestamp"]})
        except Exception as e:  # noqa: BLE001
            out.append({"input": case, **err(e)})
    return out


# ---------------------------------------------------------------------------------------------
# integrity
# ---------------------------------------------------------------------------------------------
def integrity():
    out = {}
    entries = {
        "plain": ProvenanceEntry(entity_id="x", entity_type="entity", activity_id="act",
                                 timestamp="2026-01-01T00:00:00"),
        "rich": ProvenanceEntry(entity_id="y", entity_type="claim", activity_id="extract",
                                agent_id="alice", agent_type="person", source_document="DOI:10.1/x",
                                timestamp="2026-01-01T00:00:01.250000", confidence=0.92,
                                parent_entity_id="p", previous_version_id="y:v:1", derived_from_id="d",
                                used_entities=["u1", "u2"], previous_checksum="prev",
                                invalidated=True, invalidated_at_time="t", invalidated_by="jane",
                                invalidation_reason="why"),
        "float_confidence": ProvenanceEntry(entity_id="z", entity_type="entity", activity_id="act",
                                            timestamp="2026", confidence=0.1 + 0.2),
        "zero_confidence": ProvenanceEntry(entity_id="z", entity_type="entity", activity_id="act",
                                           timestamp="2026", confidence=0.0, agent_id="", agent_type=""),
        "unicode": ProvenanceEntry(entity_id="é", entity_type="实体", activity_id="act",
                                   source_document="\U0001f389", timestamp="2026", confidence=1e-7),
        "big_confidence": ProvenanceEntry(entity_id="b", entity_type="e", activity_id="a",
                                          timestamp="2026", confidence=1e16),
    }
    out["entries"] = {}
    for name, e in entries.items():
        checksum = I.compute_checksum(e)
        e.checksum = checksum
        out["entries"][name] = {
            "dict": e.to_dict(),
            "checksum": checksum,
            "dict_checksum": I.compute_checksum(e.to_dict()),
            "verify": I.verify_checksum(e),
            "verify_dict": I.verify_checksum(e.to_dict()),
            "verify_other": I.verify_checksum(e, "nope"),
        }
    out["dict_cases"] = []
    for d in [
        {},
        {"entity_type": "t", "confidence": 0},
        {"confidence": None, "agent_id": None},
        {"confidence": "0.5", "timestamp": 5, "entity_type": ["a", 1]},
        {"used_entities": "abc", "invalidated": 1},
        {"used_entities": {"k1": 1, "k2": 2}},
        {"used_entities": [], "invalidated": [], "previous_checksum": {"a": None}},
        {"confidence": True, "agent_type": False, "source_document": 0.0},
        {"confidence": 2.5e-05, "invalidation_reason": 12, "derived_from_id": 1.5},
    ]:
        out["dict_cases"].append({"input": d, "checksum": I.compute_checksum(d)})
    out["verify_cases"] = {
        "dict_without_checksum": I.verify_checksum({"entity_type": "t"}),
        "dict_none_checksum": I.verify_checksum({"entity_type": "t", "checksum": None}),
        "entry_without_checksum": I.verify_checksum(entries["plain"].__class__(
            entity_id="q", entity_type="t", activity_id="a", timestamp="x")),
    }
    out["data_checksums"] = {s: I.compute_data_checksum(s) for s in ["some data", "", "é\U0001f389"]}
    out["verify_data"] = [I.verify_data_checksum("some data", I.compute_data_checksum("some data")),
                          I.verify_data_checksum("some data", "x")]
    out["dict_checksums"] = []
    for d in [
        {},
        {"key1": "value1", "key2": "value2"},
        {"b": 1, "a": 2.5, "c": None, "d": True, "e": False},
        {"z": [1, "two", 3.0, None], "y": {"nested": {"deep": "x'y"}}, "é": "实"},
        {"q": 'he said "hi"', "r": "it's", "s": "both ' and \"", "t": "tab\tnewline\n\\"},
        {"f": 1e16, "g": 1e-05, "h": 0.1, "i": -0.0, "j": 123456789012345678},
        {"B": 1, "a": 2, "_": 3, "1": 4},
        {"ctrl": "\x00\x7f\x80  ", "emoji": "\U0001f389"},
    ]:
        out["dict_checksums"].append({"input": d, "repr": str(sorted(d.items())),
                                      "checksum": I.compute_dict_checksum(d),
                                      "verify": I.verify_dict_checksum(d, I.compute_dict_checksum(d))})
    return out


# ---------------------------------------------------------------------------------------------
# storage
# ---------------------------------------------------------------------------------------------
def storage(tmp):
    out = {}
    CLOCK.reset()
    s = InMemoryStorage()
    out["in_memory_chain_head_empty"] = s.get_chain_head()
    s.store(ProvenanceEntry(entity_id="e1", entity_type="entity", activity_id="act", sequence_id=3, checksum="c3"))
    s.store(ProvenanceEntry(entity_id="e2", entity_type="entity", activity_id="act", sequence_id=1, checksum="c1"))
    out["in_memory_chain_head_lower_seq"] = s.get_chain_head()
    s.store(ProvenanceEntry(entity_id="e3", entity_type="entity", activity_id="act"))
    out["in_memory_chain_head_unchained"] = s.get_chain_head()
    # lineage BFS order on a DAG
    CLOCK.reset()
    s = InMemoryStorage()
    for eid, parent, used in [("root", None, []), ("a", "root", []), ("b", "root", ["a"]),
                              ("c", "b", ["a", "root", "missing"]), ("d", "c", ["b"]), ("e", None, ["d", "c"])]:
        s.store(ProvenanceEntry(entity_id=eid, entity_type="entity", activity_id="act",
                                parent_entity_id=parent, used_entities=used))
    s.store(ProvenanceEntry(entity_id="v2", entity_type="entity", activity_id="act",
                            previous_version_id="a", derived_from_id="e"))
    out["in_memory_lineage"] = {
        "e": ids(s.trace_lineage("e")),
        "e_depth_1": ids(s.trace_lineage("e", max_depth=1)),
        "e_depth_2": ids(s.trace_lineage("e", max_depth=2)),
        "e_depth_0": ids(s.trace_lineage("e", max_depth=0)),
        "missing": ids(s.trace_lineage("missing")),
        "d": ids(s.trace_lineage("d")),
    }
    out["in_memory_descendants"] = {
        "root": ids(s.trace_descendants("root")),
        "a": ids(s.trace_descendants("a")),
        "a_depth_1": ids(s.trace_descendants("a", max_depth=1)),
        "e": ids(s.trace_descendants("e")),
        "leaf": ids(s.trace_descendants("v2")),
    }
    out["in_memory_retrieve_all_entity"] = ids(s.retrieve_all(entity_type="entity"))
    out["in_memory_retrieve_all_empty_filter"] = ids(s.retrieve_all(entity_type=""))
    out["in_memory_clear"] = s.clear()

    # SQLite: raw rows, replace ordering, migration
    CLOCK.reset()
    path = os.path.join(tmp, "storage.db")
    st = SQLiteStorage(path)
    st.store(ProvenanceEntry(entity_id="a", entity_type="entity", activity_id="act",
                             metadata={"k": "v", "n": 1.5, "u": "é", "l": [1, None]},
                             used_entities=["x", "y"], informed_by_activities=["i"], sequence_id=1,
                             checksum="ca"))
    st.store(ProvenanceEntry(entity_id="b", entity_type="chunk", activity_id="act", parent_entity_id="a",
                             sequence_id=2, checksum="cb", is_automated=False, invalidated=True))
    st.store(ProvenanceEntry(entity_id="a", entity_type="entity", activity_id="act2", sequence_id=3,
                             checksum="ca2"))
    st.store(ProvenanceEntry(entity_id="c", entity_type="entity", activity_id="act", used_entities=["a"],
                             derived_from_id="b"))
    conn = sqlite3.connect(path)
    out["sqlite_raw_rows"] = [list(r) for r in conn.execute(
        "SELECT entity_id, used_entities, metadata, informed_by_activities, is_automated, invalidated, "
        "confidence, version, sequence_id FROM provenance ORDER BY rowid")]
    out["sqlite_columns"] = [r[1] for r in conn.execute("PRAGMA table_info(provenance)")]
    out["sqlite_indexes"] = sorted(r[1] for r in conn.execute("PRAGMA index_list(provenance)"))
    conn.close()
    out["sqlite_retrieve_all"] = dicts(st.retrieve_all())
    out["sqlite_retrieve_all_chunk"] = ids(st.retrieve_all(entity_type="chunk"))
    out["sqlite_chain_head"] = list(st.get_chain_head())
    out["sqlite_lineage_c"] = ids(st.trace_lineage("c"))
    out["sqlite_descendants_a"] = sorted(ids(st.trace_descendants("a")))
    out["sqlite_descendants_b"] = sorted(ids(st.trace_descendants("b")))
    out["sqlite_clear"] = st.clear()
    out["sqlite_chain_head_after_clear"] = st.get_chain_head()
    st.store(ProvenanceEntry(entity_id="n", entity_type="entity", activity_id="act", sequence_id=4))
    out["sqlite_chain_head_null_checksum"] = list(st.get_chain_head())

    # Pre-#825 database migrated in place
    old = os.path.join(tmp, "old.db")
    conn = sqlite3.connect(old)
    conn.execute("""
        CREATE TABLE provenance (
            entity_id TEXT PRIMARY KEY, entity_type TEXT NOT NULL, activity_id TEXT NOT NULL,
            agent_id TEXT DEFAULT 'semantica', source_document TEXT, source_location TEXT,
            source_quote TEXT, timestamp TEXT NOT NULL, first_seen TEXT, last_updated TEXT,
            confidence REAL DEFAULT 1.0, checksum TEXT, parent_entity_id TEXT, used_entities TEXT,
            start_index INTEGER, end_index INTEGER, credibility REAL, metadata TEXT,
            version TEXT DEFAULT '1.0')""")
    conn.execute("INSERT INTO provenance VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                 ("old_entity", "entity", "legacy_activity", "semantica", None, None, None,
                  "2025-01-01T00:00:00", "2025-01-01T00:00:00", "2025-01-01T00:00:00", 1, "oldchecksum",
                  None, None, 4, 8, 0.25, None, "1.0"))
    conn.commit()
    conn.close()
    migrated = SQLiteStorage(old)
    out["migrated_entry"] = migrated.retrieve("old_entity").to_dict()
    conn = sqlite3.connect(old)
    out["migrated_columns"] = [list(r[1:5]) for r in conn.execute("PRAGMA table_info(provenance)")]
    conn.close()
    return out


# ---------------------------------------------------------------------------------------------
# manager scenarios (fixed clock)
# ---------------------------------------------------------------------------------------------
def scenario_basic():
    CLOCK.reset()
    m = ProvenanceManager()
    e = m.track_entity("entity_1", source="doc_1", metadata={"confidence": 0.9})
    out = {"entry": e.to_dict(), "all": dicts(m.storage.retrieve_all())}
    m.track_entity("entity_2", source="doc_1")
    out["clear"] = m.clear()
    out["lineage_after_clear"] = m.get_lineage("entity_1")
    out["chain_head_after_clear"] = m.storage.get_chain_head()
    fresh = m.track_entity("entity_3", source="doc_2")
    out["fresh_after_clear"] = [fresh.sequence_id, fresh.previous_checksum]
    out["clock_calls"] = CLOCK.n
    return out


def scenario_lineage():
    CLOCK.reset()
    m = ProvenanceManager()
    m.track_entity("e1", "doc_1")
    m.track_chunk(chunk_id="chunk_1", source_document="doc_1", parent_chunk_id="e1")
    return {"chunk_1": m.get_lineage("chunk_1"), "e1": m.get_lineage("e1"),
            "missing": m.get_lineage("nope"), "clock_calls": CLOCK.n}


def run_retrack(m):
    m.track_entity("X", source="doc_1", agent_id="alice", metadata={"status": "draft", "n": 1})
    m.track_entity("X", source="doc_2", agent_id="bob", revision_type="correction",
                   metadata={"status": "final"})
    m.track_entity("P", source="doc_0")
    m.track_entity("X", source="doc_3", agent_id="carol", supersedes="X_old", parent_entity_id="P")
    m.track_entity("Y", source="X")
    m.track_entity("Z", source="doc_1", metadata={"derived_from": "Y", "z": True}, used_entities=["P"])
    m.track_entity("W", source="doc_1", metadata={"derived_from": ""}, parent_entity_id="")
    m.track_entity("V", source="doc_1", metadata={"derived_from": 12345})


def retrack_outputs(m):
    return {
        "all": dicts(m.storage.retrieve_all()),
        "lineage_X": m.get_lineage("X"),
        "lineage_Z": m.get_lineage("Z"),
        "revision_history_X": m.revision_history("X"),
        "revision_history_missing": m.revision_history("missing"),
        "verify_chain": m.verify_chain(),
        "check": m.check(),
        "check_strict": m.check(strict=True),
        "descendants_P": m.get_descendants("P"),
        "descendants_missing": m.get_descendants("missing"),
        "trace_descendants_X": ids(m.trace_descendants("X")),
        "trace_descendants_P_1": ids(m.trace_descendants("P", max_depth=1)),
        "trace_lineage_Z_1": ids(m.trace_lineage("Z", max_depth=1)),
        "trace_lineage_Z_2": ids(m.trace_lineage("Z", max_depth=2)),
        "audit_table": m.audit_log(),
        "audit_csv": m.audit_log(format="csv"),
        "audit_json": m.audit_log(format="json"),
        "audit_since": m.audit_log(since="2026-01-01T00:00:10", format="csv"),
        "audit_other_format": m.audit_log(format="xml"),
        "lineage_cli_Z_2": m.lineage("Z", depth=2),
        "lineage_cli_Z_neg": m.lineage("Z", depth=-1),
        "lineage_cli_missing": m.lineage("missing"),
        "statistics": m.get_statistics(),
        "sources_Z": m.get_all_sources("Z"),
        "recorded_between": m.query_recorded_between("2026-01-01T00:00:05", "2026-01-01T00:00:15"),
        "recorded_between_none": m.query_recorded_between("1990-01-01T00:00:00", "1990-01-02T00:00:00"),
        "provenance_Y": m.get_provenance("Y"),
        "provenance_missing": m.get_provenance("missing"),
    }


def scenario_retrack():
    CLOCK.reset()
    m = ProvenanceManager()
    run_retrack(m)
    out = retrack_outputs(m)
    out["clock_calls"] = CLOCK.n
    return out


def scenario_retrack_sqlite(tmp):
    CLOCK.reset()
    m = ProvenanceManager(storage_path=os.path.join(tmp, "retrack.db"))
    run_retrack(m)
    out = retrack_outputs(m)
    out["clock_calls"] = CLOCK.n
    return out


def scenario_invalidate(storage_path=None):
    CLOCK.reset()
    m = ProvenanceManager(storage_path=storage_path)
    m.track_entity("e1", "doc1")
    m.track_entity("e2", "doc1", parent_entity_id="e1")
    inv = m.invalidate("e1", agent_id="reviewer_jane", reason="retracted", metadata={"ticket": 7})
    inv2 = m.invalidate("e1", agent_id="r2")
    out = {
        "inv": inv.to_dict(),
        "inv2": inv2.to_dict(),
        "missing": call(lambda: m.invalidate("never_tracked", agent_id="x")),
        "all": dicts(m.storage.retrieve_all()),
        "verify_chain": m.verify_chain(),
        "check": m.check(),
        "export": exports(m),
        "clock_calls": CLOCK.n,
    }
    return out


def scenario_kwargs():
    CLOCK.reset()
    m = ProvenanceManager()
    out = {}
    out["agent_record"] = m.track_entity(
        "e1", "doc1", agent=AgentRecord(id="reviewer_bob", agent_type="person", is_automated=False),
        role="approver", agent_id="ignored").to_dict()
    out["activity_record"] = m.track_entity(
        "e2", "doc1", activity=ActivityRecord(id="run42", started_at_time="2026-01-01T00:00:00",
                                              ended_at_time="2026-01-01T00:00:03"),
        activity_id="ignored").to_dict()
    out["scalars"] = m.track_entity(
        "e3", "doc1", agent_id="alice", agent_type="person", is_automated=False, role="r",
        activity_id="parse", activity_started_at_time="t0", activity_ended_at_time="t1",
        acted_on_behalf_of="org1", informed_by=["ingest"], valid_from="2026-01-01",
        valid_until="2026-06-01", revision_type="correction", supersedes="old", bundle_id="run_1",
        source_location="p.4", source_quote="q", confidence=0.75, entity_type="claim").to_dict()
    out["relationship"] = m.track_relationship(
        "r1", "doc1", metadata={"type": "founded"}, agent_id="bot1", confidence=0.5,
        source_location="loc", informed_by=["x"], source_quote="ignored", parent_entity_id="ignored",
        entity_type="ignored", used_entities=["ignored"]).to_dict()
    out["relationship_plain"] = m.track_relationship("r2", "doc2").to_dict()
    out["chunk"] = m.track_chunk("c1", source_document="doc1", agent_id="chunker_v2", note="hello",
                                 activity_id="split", start_index=5, end_index=10,
                                 source_path="/p.pdf").to_dict()
    out["chunk_records"] = m.track_chunk(
        "c2", source_document="doc1", parent_chunk_id="c1", agent=AgentRecord(id="ag"),
        activity=ActivityRecord(id="act2", started_at_time="s"), role="x", is_automated=False,
        activity_started_at_time="ignored").to_dict()
    src = SourceReference(document="DOI:10.1038/x", page=4, confidence=0.92,
                          metadata={"credibility": 0.8, "note": "n"})
    out["property"] = m.track_property_source("e1", "biomass", "463%", src, agent_id="prop_tracker",
                                              extra=1, value_unit="pct").to_dict()
    src2 = SourceReference(document="doc2", page=0, section="Table 1", metadata={"value": "shadow"})
    out["property_section"] = m.track_property_source("e2", "p", {"v": [1, 2]}, src2).to_dict()
    src3 = SourceReference(document="doc3")
    out["property_plain"] = m.track_property_source("e3", "q", None, src3, entity_id_note="x").to_dict()
    out["statistics"] = m.get_statistics()
    out["export"] = exports(m)
    out["clock_calls"] = CLOCK.n
    return out


def scenario_batch(storage_path=None):
    CLOCK.reset()
    m = ProvenanceManager(storage_path=storage_path)
    out = {}
    out["entities_count"] = m.track_entities_batch(
        [{"id": "b1"}, {"entity_id": "b2", "metadata": {"k": 1, "extra_note": "override"}},
         {"id": ""}, {"name": "no id"}, {"id": "b3", "entity_id": "ignored", "confidence": 0.1}],
        source="doc_batch", agent_id="batch_v2", entity_type="credit_feature",
        activity_id="bureau_parsing", extra_note="free", informed_by=["ingest"], bundle_id="run_1",
        confidence=0.5)
    out["chunks_count"] = m.track_chunks_batch(
        [{"id": "c1", "start_index": 0, "end_index": 10},
         {"chunk_id": "c2", "start_index": 10, "end_index": 20, "parent_chunk_id": "c1",
          "metadata": {"m": 2, "lang": "de"}},
         {"id": "c3", "metadata": {"source_path": "boom"}},
         {"id": "c4", "metadata": None},
         {"id": "c5"}],
        source_document="doc1", source_path="/p", lang="en", agent_id="chunker")
    out["block_failure_count"] = m.track_entities_batch([{"id": "z1"}, {"id": "z2", "metadata": None}],
                                                         source="d")
    out["empty_count"] = m.track_entities_batch([], source="d")
    out["all"] = dicts(m.storage.retrieve_all())
    out["verify_chain"] = m.verify_chain()
    out["clock_calls"] = CLOCK.n
    return out


def scenario_export_rich():
    CLOCK.reset()
    m = ProvenanceManager()
    m.track_entity("e_parent", source="doc_rdf")
    m.track_entity("e_child", source="doc_rdf", parent_entity_id="e_parent", activity_id="act_transform",
                   agent_id="alice", agent_type="person", role="approver", acted_on_behalf_of="org1",
                   informed_by=["ingest"], bundle_id="run1")
    m.track_entity("e3", source="doc", activity=ActivityRecord(
        id="a1", started_at_time="2026-01-01T00:00:00", ended_at_time="2026-01-01T00:00:05"),
        agent_id="unknown")
    m.track_entity("e4", source="doc", activity_id="unknown", agent_id="org9", agent_type="organization",
                   used_entities=["e3"])
    m.track_entity("e5", source="doc", agent_type="robot", used_entities=["e_parent"],
                   parent_entity_id="e_parent")
    m.track_relationship("rel", source="doc", agent_id="bob", activity_id="link")
    m.track_chunk("chunk", source_document="doc", parent_chunk_id="e4")
    m.invalidate("e5", agent_id="reviewer")
    out = {"default": exports(m), "custom_base": exports(m, base_uri="https://example.org/kg#"),
           "urn_base": exports(m, base_uri="urn:x:"), "no_sep_base": exports(m, base_uri="http://x.org/ns")}
    out["unknown_format"] = call(lambda: m.export_prov(format="xml"))
    out["clock_calls"] = CLOCK.n
    return out


def scenario_export_multi():
    CLOCK.reset()
    m = ProvenanceManager()
    m.track_entity("e_parent", source="doc_rdf")
    m.track_entity("e_child", source="doc_rdf", parent_entity_id="e_parent",
                   used_entities=["e_parent", "u2", "u3"], activity_id="act_transform",
                   informed_by=["i1", "i2"])
    return {"export": exports(m), "clock_calls": CLOCK.n}


def scenario_export_ids():
    CLOCK.reset()
    s = InMemoryStorage()
    for i, eid in enumerate(["doc:X", "a.b", "ab.", "a(b)", "a%zz", "a%41", "a/b", "123", "_x", "-x",
                             "été", "x·y", "e_parent:v:2026-01-01T00:00:00", "a#b",
                             "ns#tail", "", "a-b", "实体", "a~b", "a'b"]):
        s.store(ProvenanceEntry(entity_id=eid, entity_type="entity", activity_id="act",
                                timestamp="2026-01-01T00:00:00", agent_id="ag"))
    m = ProvenanceManager(storage=s)
    out = {"export": exports(m)}
    s2 = InMemoryStorage()
    s2.store(ProvenanceEntry(entity_id="a b", entity_type="entity", activity_id="act",
                             timestamp="2026-01-01T00:00:00"))
    out["invalid"] = exports(ProvenanceManager(storage=s2))
    s3 = InMemoryStorage()
    s3.store(ProvenanceEntry(entity_id="ok", entity_type="entity", activity_id="act",
                             timestamp='2026 "quoted" \\ back\nline\r\ttab'))
    out["literal_escapes"] = exports(ProvenanceManager(storage=s3))
    s4 = InMemoryStorage()
    for i, t in enumerate(["t0", "2026-01-01", "2026-01-01T00:00:00Z", "2026-01-01T00:00:00.000000",
                           "20260101T000000", "2026-13-01T00:00:00", "2026-01-01T00:00:00+05:30"]):
        s4.store(ProvenanceEntry(entity_id=f"t{i}", entity_type="entity", activity_id=f"act{i}",
                                 timestamp=t, activity_started_at_time=t))
    out["literal_norm"] = exports(ProvenanceManager(storage=s4))
    out["empty"] = exports(ProvenanceManager())
    return out


def scenario_sqlite(tmp):
    CLOCK.reset()
    db = os.path.join(tmp, "manager.db")
    m = ProvenanceManager(storage_path=db)
    m.track_entity("e1", "doc1", metadata={"k": "v", "n": 1.5, "u": "é"}, used_entities=["a", "b"])
    m.track_entity("e2", "doc1", parent_entity_id="e1")
    m.track_entity("e1", "doc1")
    m.track_chunk("c1", "doc1", parent_chunk_id="e2", start_index=1, end_index=2)
    out = {
        "all": dicts(m.storage.retrieve_all()),
        "chain_head": list(m.storage.get_chain_head()),
        "lineage_c1": m.get_lineage("c1"),
        "descendants_e1_sorted": sorted(ids(m.trace_descendants("e1"))),
        "verify_chain": m.verify_chain(),
    }
    conn = sqlite3.connect(db)
    out["raw"] = [list(r) for r in conn.execute(
        "SELECT entity_id, used_entities, metadata FROM provenance ORDER BY rowid")]
    conn.execute("DELETE FROM provenance WHERE entity_id = 'e2'")
    conn.commit()
    conn.close()
    out["verify_after_delete"] = ProvenanceManager(storage_path=db).verify_chain()
    conn = sqlite3.connect(db)
    conn.execute("UPDATE provenance SET sequence_id = 10 WHERE entity_id = 'c1'")
    conn.execute("UPDATE provenance SET source_document = 'tampered' WHERE entity_id = 'e1'")
    conn.commit()
    conn.close()
    out["verify_after_tamper"] = ProvenanceManager(storage_path=db).verify_chain()
    out["clock_calls"] = CLOCK.n
    return out


def scenario_config(tmp):
    out = {}
    p1 = os.path.join(tmp, "cfg.db")
    out["provenance_section"] = type(ProvenanceManager(config={"provenance": {"storage_path": p1}}).storage).__name__
    out["top_level"] = type(ProvenanceManager(config={"storage_path": p1}).storage).__name__
    out["section_none"] = type(ProvenanceManager(config={"provenance": {"storage_path": None}, "storage_path": p1}).storage).__name__
    out["section_not_mapping"] = type(ProvenanceManager(config={"provenance": "x", "storage_path": p1}).storage).__name__
    out["empty"] = type(ProvenanceManager(config={}).storage).__name__
    out["not_mapping"] = type(ProvenanceManager(config=["storage_path"]).storage).__name__
    out["bad_type"] = call(lambda: type(ProvenanceManager(config={"storage_path": 5}).storage).__name__)
    out["unopenable"] = call(lambda: type(ProvenanceManager(storage_path=os.path.join(tmp, "missing_dir", "x.db")).storage).__name__)
    return out


FORMAT_NAMES = [
    "turtle", "ttl", "text/turtle", "n3", "text/n3", "nt", "ntriples", "nt11",
    "application/n-triples", "json-ld", "jsonld", "application/ld+json", "xml",
    "application/rdf+xml", "hext", "nquads", "application/n-quads", "trix", "application/trix",
    "patch", "TURTLE", "pretty-xml", "longturtle", "trig", "application/trig",
]


def formats():
    s = InMemoryStorage()
    s.store(ProvenanceEntry(entity_id="e1", entity_type="entity", activity_id="act",
                            timestamp="2026-01-01T00:00:00", agent_id="ag"))
    m = ProvenanceManager(storage=s)
    return {name: call(lambda name=name: m.export_prov(format=name)) for name in FORMAT_NAMES}


def default_path(tmp):
    out = []

    def record(label):
        path = M.ProvenanceManager._default_storage_path
        kind = type(ProvenanceManager().storage).__name__
        out.append([label, os.path.basename(path) if path else None, kind])

    outer = os.path.join(tmp, "outer.db")
    inner = os.path.join(tmp, "inner.db")
    x = os.path.join(tmp, "x.db")
    e = os.path.join(tmp, "exc.db")
    record("initial")
    with M.default_storage_path(outer):
        record("outer")
        with ProvenanceManager.default_storage_path(inner):
            record("inner")
            with M.default_storage_path(None):
                record("none")
            record("back_inner")
        record("back_outer")
    record("after")
    with M.default_storage_path(None):
        record("guard")
        ProvenanceManager.set_default_storage_path(x)
        record("set")
        with M.default_storage_path(None):
            record("nested_none")
        record("after_nested")
        ProvenanceManager.set_default_storage_path(None)
        record("unset")
    record("after_guard")
    try:
        with M.default_storage_path(e):
            record("in_exception_ctx")
            raise RuntimeError("boom")
    except RuntimeError:
        pass
    record("after_exception")
    return out


def bridge():
    out = {}
    CLOCK.reset()
    ba = BA.BridgeAxiom(axiom_id="BA-FINANCE-001", name="biomass_tourism_elasticity",
                        rule="1% biomass increase → 0.346% tourism revenue increase", coefficient=0.346,
                        source_doi="10.1038/s41586-021-03371-z", source_page="Table S4",
                        input_domain="ecological", output_domain="financial", confidence=0.92)
    out["to_dict"] = ba.to_dict()
    out["apply"] = {}
    for name, value in {"int": 463, "float": 38.5, "bool": True, "zero": 0, "neg": -2.5, "big": 10**18}.items():
        out["apply"][name] = ba.apply(input_entity="cabo_pulmo_biomass", input_value=value)
    out["apply_errors"] = {}
    for name, value in {"none": None, "str": "abc", "list": [1], "dict": {"a": 1}}.items():
        out["apply_errors"][name] = call(lambda v=value: ba.apply(input_entity="x", input_value=v))
    out["apply_none_entity"] = ba.apply(input_entity=None, input_value=2)

    m = ProvenanceManager()
    ba2 = BA.BridgeAxiom(axiom_id="BA-TEST-005", name="prov", rule="r", coefficient=1.5,
                         source_doi="10.1234/test", source_page="Page 1", source_quote="Test quote",
                         metadata={"ignored": True})
    out["apply_tracked"] = ba2.apply(input_entity="input_entity", input_value=100, prov_manager=m,
                                     unit="pct", axiom_id="kw_override")
    out["tracked_entry"] = m.get_provenance("input_entity_transformed_BA-TEST-005")
    out["tracked_clock_calls"] = CLOCK.n

    CLOCK.reset()
    m = ProvenanceManager()
    axioms = [BA.BridgeAxiom(axiom_id="BA-L1", name="layer_1", rule="Test", coefficient=2.0,
                             source_doi="10.1234/l1", source_page="P1", confidence=0.9),
              BA.BridgeAxiom(axiom_id="BA-L2", name="layer_2", rule="Test", coefficient=0.5,
                             source_doi="10.1234/l2", source_page="P2", confidence=0.8)]
    chain = BA.create_translation_chain({"entity_id": "lineage_test", "value": 75, "source": "test_doc"},
                                        axioms, m)
    out["chain"] = chain.to_dict()
    out["trace"] = BA.trace_translation_chain(chain, m)
    out["chain_clock_calls"] = CLOCK.n
    CLOCK.reset()
    out["chain_no_entity"] = BA.create_translation_chain({"value": 3}, axioms[:1], None).to_dict()
    out["chain_no_axioms"] = BA.create_translation_chain({"entity_id": "e", "value": 1.5}, [], None).to_dict()
    out["chain_error"] = call(lambda: BA.create_translation_chain({"entity_id": "x"}, axioms, None).to_dict())
    CLOCK.reset()
    tc = BA.TranslationChain(chain_id="chain_002")
    tc.add_layer("L1", "input", 100, source="doc_1")
    tc.add_layer("L2", "bridge_axiom", 0.5, source="axiom_1", timestamp="override", extra=[1])
    tc.add_layer("L3", "output", 50)
    out["manual_chain"] = tc.to_dict()
    out["manual_get_missing"] = tc.get_layer("L9")
    out["manual_trace"] = BA.trace_translation_chain(tc, ProvenanceManager())
    out["repr_axiom"] = repr(BA.BridgeAxiom(axiom_id="A", name="n", rule="r", coefficient=0.5,
                                            source_doi="d", source_page="p"))
    out["repr_chain"] = repr(BA.TranslationChain(chain_id="c"))
    out["repr_manual_chain"] = repr(tc)
    return out


def main():
    target = sys.argv[1]
    with tempfile.TemporaryDirectory() as tmp:
        OUT["schemas"] = schemas()
        OUT["isoformat"] = isoformat_cases()
        OUT["integrity"] = integrity()
        OUT["storage"] = storage(tmp)
        OUT["basic"] = scenario_basic()
        OUT["lineage"] = scenario_lineage()
        OUT["retrack"] = scenario_retrack()
        OUT["retrack_sqlite"] = scenario_retrack_sqlite(tmp)
        OUT["invalidate"] = scenario_invalidate()
        OUT["invalidate_sqlite"] = scenario_invalidate(os.path.join(tmp, "inv.db"))
        OUT["kwargs"] = scenario_kwargs()
        OUT["batch"] = scenario_batch()
        OUT["batch_sqlite"] = scenario_batch(os.path.join(tmp, "batch.db"))
        OUT["export_rich"] = scenario_export_rich()
        OUT["export_multi"] = scenario_export_multi()
        OUT["export_ids"] = scenario_export_ids()
        OUT["sqlite"] = scenario_sqlite(tmp)
        OUT["config"] = scenario_config(tmp)
        OUT["bridge"] = bridge()
        OUT["default_path"] = default_path(tmp)
        OUT["formats"] = formats()
    OUT["module"] = {
        "all": list(P.__all__),
        "version": P.__version__,
        "default_base_uri": M.DEFAULT_BASE_URI,
        "track_entity_kwargs": sorted(M.ProvenanceManager._TRACK_ENTITY_KWARGS),
        "migration_columns": [list(c) for c in SQLiteStorage._MIGRATION_COLUMNS],
    }
    with open(target, "w", encoding="utf-8") as fh:
        json.dump(OUT, fh, ensure_ascii=False, indent=1, sort_keys=False)
        fh.write("\n")


if __name__ == "__main__":
    main()
