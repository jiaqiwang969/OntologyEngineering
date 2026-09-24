"""Transport contracts and source binding for Semantica-owned Jev questions.

This module exports registered meanings; it never implements ontology reasoning.
"""
from __future__ import annotations
from copy import deepcopy
import hashlib
import json
from pathlib import Path

from ontology_engineering.jev_transport import ADAPTER_VERSION, pinned_model, strict_json
from ontology_engineering.method_evidence import _inside, _keys, _text, pointer_value
from scripts.semantic_bundle_transport import load_bundle

ROOT = Path(__file__).resolve().parents[1]
BATCH_SCHEMA = "ontology-engineering.judgment-batch/v1"
LOCK_SCHEMA = "ontology-engineering.judgment-deployment/v1"


def encoded(value):
    # Preserve all array, option and question ordering: changes can affect inference.
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(value if isinstance(value, bytes) else encoded(value)).hexdigest()


def contracts(skill_root=ROOT):
    spec, payload, _ = load_bundle("engineering-judgment-intake", root=Path(skill_root))
    manifest = strict_json(payload[spec["manifest"]])
    def asset(aid):
        entry = next(a for a in manifest["assets"] if a["asset_id"] == aid)
        return strict_json(payload[entry["path"]]), entry["sha256"]
    catalog, question_sha = asset("question-catalog")
    patterns, pattern_sha = asset("pattern-catalog")
    profiles, profile_sha = asset("input-profiles")
    if catalog["allowed_use"] != "candidate_only" or catalog["fact_authority"] is not False:
        raise ValueError("unsupported_question_authority")
    identity = {k:spec[k] for k in ("package_id", "package_version", "package_sha256", "sha256", "runtime")}
    identity.update(question_sha256=question_sha, pattern_sha256=pattern_sha, profile_sha256=profile_sha)
    return {"identity":identity, "catalog":catalog, "patterns":patterns, "profiles":profiles}


def adapter_identity(skill_root=ROOT):
    names = ["ontology_engineering/jev_transport.py", "ontology_engineering/judgment_contracts.py",
             "ontology_engineering/judgment_batch.py", "ontology_engineering/method_evidence.py",
             "ontology_engineering/semantica_runtime.py", "scripts/semantic_bundle_transport.py",
             "ontology_engineering/judgment_review.py", "scripts/judgment_batch.py", "scripts/judgment_review.py"]
    names += ["ontology_engineering/judgment_compatibility.py", "scripts/judgment_compatibility.py"]
    names += ["ontology_engineering/judgment_admission.py", "scripts/judgment_admission.py"]
    names += ["ontology_engineering/judgment_evolution.py", "scripts/judgment_evolution.py"]
    return {"version":ADAPTER_VERSION, "files":{n:digest((Path(skill_root)/n).read_bytes()) for n in names}}


def deployment_lock(*, model, strategy="batched", workers=2, max_attempts=3, max_state_chars=24000, skill_root=ROOT):
    if strategy not in ("batched", "single"):
        raise ValueError("unsupported_batch_strategy")
    for value, low, high in ((workers,1,16),(max_attempts,1,5),(max_state_chars,100,100000)):
        if type(value) is not int or not low <= value <= high:
            raise ValueError("invalid_execution_limit")
    return {"schema":LOCK_SCHEMA,"mode":"shadow_candidate_only", "compatibility_status":"unvalidated_experiment", "model":pinned_model(model),
            "semantic_assets":contracts(skill_root)["identity"],"adapter":adapter_identity(skill_root),
            "strategy":strategy,"workers":workers,"max_attempts":max_attempts,"max_state_chars":max_state_chars,
            "source_selection":"explicit-json-pointer-or-text-range/v1", "routing":"retain-obligations/v1",
            "sampling":"all-errors-unknowns-and-stable-stratified-audit/v1", "calibration":"not_established"}


def validate_lock(lock, *, skill_root=ROOT):
    expected = deployment_lock(model=lock.get("model"),strategy=lock.get("strategy"), workers=lock.get("workers"),
                               max_attempts=lock.get("max_attempts"),max_state_chars=lock.get("max_state_chars"),skill_root=skill_root)
    if lock != expected:
        raise ValueError("deployment_combination_or_adapter_mismatch")


def question_stages(selected, dependencies):
    """Topologically stage questions; a missing dependency is never inferred."""
    pending = list(selected)
    complete, stages = set(), []
    for qid in selected:
        if set(dependencies.get(qid, [])) - set(selected):
            raise ValueError("question_dependency_missing")
    while pending:
        ready = [q for q in pending if set(dependencies.get(q, [])).issubset(complete)]
        if not ready:
            raise ValueError("question_dependency_cycle")
        stages.append(ready)
        complete.update(ready)
        pending = [q for q in pending if q not in complete]
    return stages


def source_selection(source, evidence_root):
    """Verify one exact source selection without evaluating its meaning."""
    _keys(source, {"id", "path", "sha256", "media_type", "selection", "lineage_group", "context_status"}, "source")
    for key in ("id", "lineage_group"):
        _text(source[key], "source." + key)
    if source["context_status"] not in ("complete_for_question", "partial", "unknown"):
        raise ValueError("invalid_context_status")
    raw = _inside(Path(evidence_root).resolve(), source["path"]).read_bytes()
    if digest(raw) != source["sha256"]:
        raise ValueError("source_hash_mismatch")
    selection = source["selection"]
    if source["media_type"] == "application/json":
        _keys(selection, {"pointer"}, "selection")
        text = pointer_value(strict_json(raw), selection["pointer"])
    elif source["media_type"] == "text/plain":
        _keys(selection, {"start", "end"}, "selection")
        decoded = raw.decode("utf-8")
        start, end = selection["start"], selection["end"]
        if type(start) is not int or type(end) is not int or not 0 <= start < end <= len(decoded):
            raise ValueError("invalid_source_range")
        text = decoded[start:end]
    else:
        raise ValueError("native_or_visual_source_requires_prior_extraction")
    _text(text, "source_text")
    if "apikey_" in text:
        raise ValueError("credential_material_in_source")
    return text


def prepare_batch(document, evidence_root, lock, *, skill_root=ROOT):
    """Read only declared local selections and preserve their exact source lineage."""
    validate_lock(lock, skill_root=skill_root)
    _keys(document, {"schema","batch_id","project_id","access_scope","items"}, "batch")
    if document["schema"] != BATCH_SCHEMA:
        raise ValueError("unsupported_batch_schema")
    for key in ("batch_id", "project_id", "access_scope"):
        _text(document[key], key)
    if not isinstance(document["items"], list) or not document["items"]:
        raise ValueError("empty_source_inventory")
    loaded = contracts(skill_root)
    catalog = loaded["catalog"]
    root, prepared, ids = Path(evidence_root).resolve(), [], set()
    for item in document["items"]:
        _keys(item,{"id","source","claim","question_ids","required_methods"}, "item")
        iid = _text(item["id"], "item.id")
        if iid in ids:
            raise ValueError("duplicate_item_id")
        ids.add(iid)
        source, claim = item["source"], item["claim"]
        text = source_selection(source, root)
        _keys(claim,{"id","statement","subject_id","subject_revision","scope","domain","cq"},"claim")
        for key, value in claim.items():
            _text(value,"claim."+key)
        questions, methods = item["question_ids"],item["required_methods"]
        if not isinstance(questions,list) or not questions or len(questions)!=len(set(questions)) or set(questions)-set(catalog["questions"]):
            raise ValueError("invalid_question_inventory")
        if not set(catalog["required_questions"]).issubset(questions):
            raise ValueError("mandatory_question_omitted")
        if not isinstance(methods,list) or not methods or len(methods)!=len(set(methods)) or set(methods)-set(loaded["profiles"]["profiles"]):
            raise ValueError("invalid_obligation_inventory")
        state = {"source_text":text,"claim":deepcopy(claim),"context_status":source["context_status"]}
        if b"apikey_" in encoded({"item":item,"state":state}):
            raise ValueError("credential_material_in_batch")
        if len(encoded(state).decode()) > lock["max_state_chars"]:
            raise ValueError("context_limit_requires_explicit_reselection")
        stages = question_stages(questions,catalog.get("dependencies",{}))
        prepared.append({"id":iid,"source":deepcopy(source),"claim":deepcopy(claim),"state":state,
                         "question_ids":list(questions),"stages":stages,"required_methods":list(methods)})
    return {"input":deepcopy(document),"items":prepared,"input_sha256":digest(document),"deployment":deepcopy(lock),
            "deployment_sha256":digest(lock),"catalog":catalog,"catalog_identity":loaded["identity"]}
