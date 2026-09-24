"""Exact configuration allowlists for bounded shadow execution.

This is transport/run bookkeeping, not an ontology registry or a quality verdict.
Entries retain a complete observed run. Matching component versions, a saved
PASS, author labels or a model score cannot qualify an untested combination.
"""
from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import sqlite3

from ontology_engineering.judgment_contracts import digest, encoded, prepare_batch, validate_lock
from ontology_engineering.jev_transport import strict_json
from ontology_engineering.method_evidence import _inside, _keys, _text

CATALOG_SCHEMA = "ontology-engineering.judgment-compatibility/v1"
ENTRY_SCHEMA = "ontology-engineering.judgment-combination-run/v1"


def _write(path, value):
    with Path(path).open("xb") as stream:
        stream.write(value if isinstance(value, bytes) else encoded(value) + b"\n")


def _ref(path, root):
    return {"path": Path(path).relative_to(root).as_posix(), "sha256": digest(Path(path).read_bytes())}


def _read(root, ref):
    _keys(ref, {"path", "sha256"}, "compatibility evidence")
    path = _inside(Path(root).resolve(), ref["path"])
    if digest(path.read_bytes()) != ref["sha256"]:
        raise ValueError("compatibility_evidence_digest_mismatch")
    return path


def _observed_scope(prepared):
    profiles = {encoded({"domain": i["claim"]["domain"], "question_ids": sorted(i["question_ids"]),
                         "required_methods": sorted(i["required_methods"])}) for i in prepared["items"]}
    return {"project_id": prepared["input"]["project_id"], "access_scope": prepared["input"]["access_scope"],
            "profiles": [strict_json(p) for p in sorted(profiles)]}


def _verify_recorded_run(prepared, journal_path):
    # Read back exact source selections, request content, raw responses and IDs.
    # Do not accept caller-authored success flags or import a semantic verdict.
    from ontology_engineering.judgment_review import read_candidates
    records = read_candidates(prepared, journal_path)
    if any({a["question_id"] for a in r["answers"]} != set(r["item"]["question_ids"]) for r in records):
        raise ValueError("compatibility_run_incomplete")
    kinds = {a["execution_kind"] for r in records for a in r["answers"]}
    if not kinds or kinds - {"live", "fixture"} or len(kinds) != 1:
        raise ValueError("compatibility_requires_one_observed_execution_kind")
    return {"execution_kind": next(iter(kinds)), "scope": _observed_scope(prepared),
            "items": len(records), "questions": sum(len(r["answers"]) for r in records),
            "candidate_ids_sha256": digest([[a["candidate_id"] for a in sorted(r["answers"], key=lambda a:a["question_id"])] for r in records])}


def record_combination(prepared, evidence_root, journal_path, output):
    """Freeze a complete run using SQLite backup and only its declared sources.

    The returned entry can support another shadow run within the observed scope.
    It supplies no independent accuracy qualification or project fact admission.
    """
    from ontology_engineering.judgment_batch import journal
    validate_lock(prepared["deployment"])
    if not Path(journal_path).is_file():
        raise ValueError("compatibility_journal_missing")
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    _write(output/"input.json", prepared["input"])
    _write(output/"deployment.json", prepared["deployment"])
    sources = output/"sources"
    sources.mkdir()
    copied = set()
    for item in prepared["items"]:
        source = item["source"]
        if source["path"] in copied:
            continue
        raw = _inside(Path(evidence_root).resolve(), source["path"]).read_bytes()
        if digest(raw) != source["sha256"]:
            raise ValueError("source_changed_before_compatibility_snapshot")
        target = sources/source["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        _write(target, raw)
        copied.add(source["path"])
    with journal(journal_path, prepared["input"]["project_id"], prepared["input"]["access_scope"]) as original:
        with sqlite3.connect(output/"journal.sqlite") as target:
            original.backup(target)
    frozen = prepare_batch(strict_json((output/"input.json").read_bytes()), sources,
                           strict_json((output/"deployment.json").read_bytes()))
    if (frozen["input_sha256"], frozen["deployment_sha256"]) != (prepared["input_sha256"], prepared["deployment_sha256"]):
        raise ValueError("compatibility_prepared_identity_mismatch")
    observed = _verify_recorded_run(frozen, output/"journal.sqlite")
    entry = {"schema": ENTRY_SCHEMA, "deployment_sha256": frozen["deployment_sha256"],
             "input_sha256": frozen["input_sha256"], "source_root": "sources",
             "input": _ref(output/"input.json", output), "deployment": _ref(output/"deployment.json", output),
             "journal": _ref(output/"journal.sqlite", output), "observed": observed,
             "allowed_use": "shadow_candidate_only", "model_quality_qualification": "not_established"}
    _write(output/"entry.json", entry)
    return entry


def inspect_entry(path):
    """Recompute what the frozen run proves; entry metadata alone proves nothing."""
    path = Path(path)
    entry = strict_json(path.read_bytes())
    _keys(entry, {"schema", "deployment_sha256", "input_sha256", "source_root", "input", "deployment", "journal",
                  "observed", "allowed_use", "model_quality_qualification"}, "combination entry")
    if (entry["schema"] != ENTRY_SCHEMA or entry["allowed_use"] != "shadow_candidate_only"
            or entry["model_quality_qualification"] != "not_established"):
        raise ValueError("unsupported_compatibility_authority")
    root = path.parent.resolve()
    if entry["source_root"] != "sources":
        raise ValueError("invalid_compatibility_source_root")
    source_root = root/"sources"
    if source_root.is_symlink() or not source_root.is_dir():
        raise ValueError("invalid_compatibility_source_root")
    raw_input = strict_json(_read(root, entry["input"]).read_bytes())
    lock = strict_json(_read(root, entry["deployment"]).read_bytes())
    db = _read(root, entry["journal"])
    prepared = prepare_batch(raw_input, source_root, lock)
    if (prepared["deployment_sha256"], prepared["input_sha256"]) != (entry["deployment_sha256"], entry["input_sha256"]):
        raise ValueError("compatibility_entry_identity_mismatch")
    observed = _verify_recorded_run(prepared, db)
    if observed != entry["observed"]:
        raise ValueError("compatibility_observation_mismatch")
    # The readback must not silently revise the saved snapshot.
    _read(root, entry["journal"])
    return entry


def check_combination(prepared, catalog, catalog_root, *, execution_kind="live"):
    """Resolve one exact combination; do not construct a cross-product of parts."""
    validate_lock(prepared["deployment"])
    if execution_kind not in {"live", "fixture"}:
        raise ValueError("invalid_compatibility_execution_kind")
    _keys(catalog, {"schema", "catalog_id", "entries"}, "compatibility catalog")
    if catalog["schema"] != CATALOG_SCHEMA or not isinstance(catalog["entries"], list):
        raise ValueError("invalid_compatibility_catalog")
    _text(catalog["catalog_id"], "catalog_id")
    wanted = digest(prepared["deployment"])
    if wanted != prepared["deployment_sha256"]:
        raise ValueError("compatibility_deployment_identity_mismatch")
    refs, seen = {}, set()
    for row in catalog["entries"]:
        _keys(row, {"deployment_sha256", "entry"}, "compatibility catalog entry")
        sha = row["deployment_sha256"]
        if not isinstance(sha, str) or len(sha) != 64 or any(c not in "0123456789abcdef" for c in sha) or sha in seen:
            raise ValueError("duplicate_or_invalid_compatibility_combination")
        seen.add(sha)
        refs[sha] = row["entry"]
    result = {"schema": "ontology-engineering.judgment-compatibility-result/v1", "deployment_sha256": wanted,
              "catalog_sha256": digest(catalog), "status": "unsupported", "reasons": [],
              "model_quality_qualification": "not_established", "fact_admission": "not_performed"}
    if wanted not in refs:
        result["reasons"] = ["whole_combination_not_listed"]
        return result
    entry = inspect_entry(_read(catalog_root, refs[wanted]))
    if entry["deployment_sha256"] != wanted:
        raise ValueError("compatibility_catalog_subject_mismatch")
    observed, requested = entry["observed"], _observed_scope(prepared)
    scope = observed["scope"]
    if observed["execution_kind"] != execution_kind:
        result["reasons"].append("recorded_execution_kind_does_not_cover_request")
    for key in ("project_id", "access_scope"):
        if scope[key] != requested[key]:
            result["reasons"].append("scope_mismatch:"+key)
    for profile in requested["profiles"]:
        covered = any(old["domain"] == profile["domain"] and
                      all(set(profile[key]).issubset(old[key]) for key in ("question_ids", "required_methods"))
                      for old in scope["profiles"])
        if not covered:
            result["reasons"].append("scope_not_covered:profile:"+profile["domain"])
    if not result["reasons"]:
        result.update(status="supported_shadow", entry_sha256=refs[wanted]["sha256"],
                      observed=deepcopy(observed), allowed_use="shadow_candidate_only")
    return result


def run_admission(prepared, *, execution_kind, experiment=False, catalog=None, catalog_root=None):
    """Explicit experiments and recorded shadow compatibility stay distinct."""
    if type(experiment) is not bool:
        raise ValueError("invalid_experiment_request")
    if experiment and catalog is not None:
        raise ValueError("experiment_and_compatibility_are_exclusive")
    if catalog is not None:
        if catalog_root is None:
            raise ValueError("compatibility_root_required")
        result = check_combination(prepared, catalog, catalog_root, execution_kind=execution_kind)
        if result["status"] != "supported_shadow":
            raise ValueError("unsupported_deployment_combination")
        return result
    if experiment or execution_kind == "fixture":
        return {"status": "explicit_experiment" if execution_kind == "live" else "fixture_experiment",
                "deployment_sha256": prepared["deployment_sha256"], "allowed_use": "shadow_candidate_only",
                "model_quality_qualification": "not_established", "fact_admission": "not_performed"}
    raise ValueError("compatibility_catalog_or_explicit_experiment_required")
