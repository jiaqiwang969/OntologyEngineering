"""Bind explicit pattern migration reviews to exact frozen native history.

Only identity, asset hashes and record projection are handled here. Semantica
evaluates migration obligations. A clear record does not establish logical
equivalence or automatically migrate any project application.
"""
from pathlib import Path

from ontology_engineering.judgment_contracts import digest
from ontology_engineering.judgment_review import _binding, _generated_record, execute_record, write_new
from ontology_engineering.jev_transport import strict_json
from ontology_engineering.method_bootstrap import load_capsule
from ontology_engineering.method_evidence import _inside, _keys, _text

SCHEMA = "ontology-engineering.pattern-migration-review/v1"
REVIEW_SCOPE = "review-pattern-migration"


def _definitions(payload, step):
    manifest = strict_json(payload[step["manifest_path"]])
    base = step["manifest_path"].rsplit("/", 1)[0] + "/"
    assets = {a["asset_id"]: a for a in manifest["assets"]}
    def read(aid):
        raw = payload[base + assets[aid]["path"]]
        if digest(raw) != assets[aid]["sha256"]:
            raise ValueError("pattern_history_asset_hash_mismatch")
        return strict_json(raw)
    profiles = read("input-profiles")["profiles"]
    catalogue = read("pattern-catalog")["patterns"]
    if "pattern-evolution-catalog" in assets:
        catalogue += read("pattern-evolution-catalog")["variants"]
    result = {}
    for pattern in catalogue:
        pid, method = pattern["id"], pattern["method"]
        if pid in result:
            raise ValueError("duplicate_pattern_history_identity")
        query, shape = "findings-" + method, "shape-" + method
        result[pid] = {"catalogue_entry": pattern, "input_profile": profiles[method],
            "query_asset_id": query, "query_sha256": assets[query]["sha256"],
            "shape_asset_id": shape, "shape_sha256": assets[shape]["sha256"]}
    return result


def migration_subject(mapping, binding_path):
    """Prepare the exact review subject, retaining asset-level old/new evidence."""
    _keys(mapping, {"schema", "mapping_id", "project_id", "source_version", "target_version", "kind", "relation",
                    "sources", "targets", "review_decision"}, "pattern migration")
    if mapping["schema"] != SCHEMA:
        raise ValueError("unsupported_pattern_migration_schema")
    for key in ("mapping_id", "project_id", "source_version", "target_version", "kind", "relation"):
        _text(mapping[key], key)
    for key in ("sources", "targets"):
        values = mapping[key]
        if (not isinstance(values, list) or not values or any(not isinstance(v, str) or not v.strip() for v in values) or
            len(values) != len(set(values))):
            raise ValueError("invalid_pattern_migration_inventory")
    spec, payload, capsule = load_capsule()
    binding = _binding(binding_path, mapping["project_id"], spec)
    if mapping["target_version"] != spec["package_version"]:
        raise ValueError("migration_target_must_be_current_bound_package")
    steps = {s["version"]: s for s in capsule["steps"]}
    if mapping["source_version"] not in steps:
        raise ValueError("migration_source_history_missing")
    old_step, new_step = steps[mapping["source_version"]], steps[mapping["target_version"]]
    old, new = _definitions(payload, old_step), _definitions(payload, new_step)
    if set(mapping["sources"]) - set(old) or set(mapping["targets"]) - set(new):
        raise ValueError("migration_pattern_definition_missing")
    # Byte/profile retention is a factual asset comparison, not equivalence proof.
    retained_ids = all(pid in new for pid in old)
    retained_definitions = retained_ids and all(old[pid] == new[pid] for pid in old)
    subject = {"mapping": {k: v for k, v in mapping.items() if k != "review_decision"},
        "binding_sha256": digest(Path(binding_path).read_bytes()), "capsule_sha256": spec["sha256"],
        "source_package_sha256": old_step["package_sha256"], "target_package_sha256": new_step["package_sha256"],
        "source_definitions": {pid: old[pid] for pid in mapping["sources"]},
        "target_definitions": {pid: new[pid] for pid in mapping["targets"]},
        "retained_definition_fingerprints": {pid: digest(value) for pid, value in old.items() if pid in new and value == new[pid]},
        "old_identity_retained": retained_ids, "old_definition_retained": retained_definitions}
    return subject, binding


def review_migration(mapping_path, evidence_root, binding_path, workspace, actor, output):
    raw = Path(mapping_path).read_bytes()
    mapping = strict_json(raw)
    subject, binding = migration_subject(mapping, binding_path)
    subject_sha = digest(subject)
    decision, decision_sha = None, None
    if mapping["review_decision"] is not None:
        ref = mapping["review_decision"]
        _keys(ref, {"path", "sha256"}, "mapping review reference")
        decision_raw = _inside(Path(evidence_root).resolve(), ref["path"]).read_bytes()
        decision_sha = digest(decision_raw)
        if decision_sha != ref["sha256"]:
            raise ValueError("mapping_review_hash_mismatch")
        decision = strict_json(decision_raw)
        _keys(decision, {"schema", "review_id", "subject_sha256", "actor_id", "authority_id", "action", "state", "reason", "issued_at"},
              "pattern mapping decision")
        authority = binding["authority"]["decision"]
        if (decision["schema"] != "ontology-engineering.pattern-migration-decision/v1" or
            decision["subject_sha256"] != subject_sha):
            raise ValueError("mapping_review_subject_mismatch")
        if (decision["actor_id"] != actor or actor != authority["authority_id"] or
            decision["authority_id"] != authority["authority_id"] or decision["action"] != REVIEW_SCOPE or
            REVIEW_SCOPE not in authority["scope"]):
            raise ValueError("mapping_review_authority_mismatch")
        for key in ("review_id", "reason", "issued_at"):
            _text(decision[key], key)
    out = Path(output); out.mkdir(parents=True, exist_ok=False)
    write_new(out / "mapping.json", raw)
    write_new(out / "subject.json", subject)
    if decision is not None:
        write_new(out / "decision.json", decision)
    claim = {"id": mapping["mapping_id"], "statement": "Review this exact non-equivalent pattern migration mapping",
             "subject_revision": mapping["target_version"], "scope": "pattern-mapping:" + mapping["mapping_id"]}
    fields = {"source_pattern_ids": mapping["sources"], "target_pattern_ids": mapping["targets"],
        "mapping_kind": mapping["kind"], "mapping_relation": mapping["relation"],
        "old_identity_retained": subject["old_identity_retained"], "old_definition_retained": subject["old_definition_retained"],
        "mapping_review_state": decision["state"] if decision else "proposed",
        "mapping_review_ref": decision["review_id"] if decision else "not_reviewed",
        "mapping_sha256": subject_sha}
    if decision is not None:
        fields["reviewed_mapping_sha256"] = decision["subject_sha256"]
    record = _generated_record(out, "mapping-review", claim, "pattern-evolution-review", fields,
        {"subject_sha256": subject_sha, "decision_sha256": decision_sha,
         "history_origin": "Exact hash-locked native bootstrap history; old source assets retained, current project review executed natively.",
         "logical_equivalence": "not_established"})
    check = execute_record(record, out, binding_path, workspace, actor, out / "native-review", project_id=mapping["project_id"])
    result = {"schema": "ontology-engineering.pattern-migration-result/v1", "subject_sha256": subject_sha,
        "mapping_sha256": digest(raw), "review": check, "status": check["status"],
        "old_identity_retained": subject["old_identity_retained"], "old_definition_retained": subject["old_definition_retained"],
        "logical_equivalence": "not_established", "project_applications_migrated": False, "tbox_mutated": False}
    write_new(out / "summary.json", result)
    return result
