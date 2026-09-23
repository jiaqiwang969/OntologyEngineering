"""Source-bound evaluation bookkeeping, never engineering fact admission.

Reference review is an externally controlled input. A recorded reviewer identity
is not authenticated here. Model probabilities and author labels cannot create
independent ground truth. Formal engineering reviews remain in Semantica.
"""
from __future__ import annotations

from collections import Counter, defaultdict
import math
from pathlib import Path

from ontology_engineering.judgment_contracts import contracts, digest
from ontology_engineering.judgment_review import read_candidates
from ontology_engineering.jev_transport import strict_json
from ontology_engineering.method_evidence import _inside, _keys, _text

SCHEMA = "ontology-engineering.judgment-evaluation-set/v1"


def _strings(value, name, *, nonempty=True):
    if not isinstance(value, list) or (nonempty and not value) or len(set(value)) != len(value):
        raise ValueError("invalid_"+name)
    for item in value:
        _text(item, name)
    return value


def _file(root, ref):
    _keys(ref, {"path", "sha256"}, "evaluation artifact")
    raw = _inside(Path(root).resolve(), ref["path"]).read_bytes()
    if digest(raw) != ref["sha256"]:
        raise ValueError("evaluation_artifact_digest_mismatch")
    return raw


def audit_dataset(document, evidence_root, *, catalog=None):
    """Find source-group leakage before metrics, preserving unreviewed labels.

Root IDs denote declared acquisitions/projects, not statistical independence.
Also group exact source-file duplicates: renaming a lineage group cannot turn
the same file into an unseen source. Derived and paraphrased records must retain
their true root IDs; this checker cannot discover an undeclared origin.
"""
    _keys(document, {"schema", "dataset_id", "developer_ids", "heldout_domains", "items"}, "evaluation set")
    if document["schema"] != SCHEMA:
        raise ValueError("unsupported_evaluation_set")
    _text(document["dataset_id"], "dataset_id")
    developers = set(_strings(document["developer_ids"], "developer_ids"))
    heldout = set(_strings(document["heldout_domains"], "heldout_domains", nonempty=False))
    questions = (catalog or contracts()["catalog"])["questions"]
    if not isinstance(document["items"], list) or not document["items"]:
        raise ValueError("evaluation_inventory_empty")
    issues, ids, groups = [], set(), defaultdict(list)
    independent, labeled = [], []
    for item in document["items"]:
        _keys(item, {"id", "source", "claim", "domain", "split", "root_ids", "used_for_development", "reference"}, "evaluation item")
        iid = _text(item["id"], "item_id")
        if iid in ids:
            raise ValueError("duplicate_evaluation_item")
        ids.add(iid)
        if item["split"] not in {"development", "calibration", "heldout"}:
            raise ValueError("invalid_evaluation_split")
        _text(item["domain"], "domain")
        if type(item["used_for_development"]) is not bool:
            raise ValueError("invalid_development_exposure")
        _strings(item["root_ids"], "root_ids")
        _keys(item["claim"], {"id", "statement", "subject_revision", "scope"}, "evaluation claim")
        for value in item["claim"].values():
            _text(value, "claim_field")
        source = item["source"]
        _keys(source, {"path", "sha256", "selection"}, "evaluation source")
        _file(evidence_root, {k:source[k] for k in ("path", "sha256")})
        # Selection is bound exactly to the prepared runtime source below.
        if not isinstance(source["selection"], dict):
            raise ValueError("invalid_evaluation_source_selection")
        for group in ["bytes:"+source["sha256"], *["root:"+x for x in item["root_ids"]]]:
            groups[group].append((iid, item["split"]))
        if item["used_for_development"] and item["split"] != "development":
            issues.append({"code":"development_exposure", "items":[iid]})
        if item["domain"] in heldout and item["split"] != "heldout":
            issues.append({"code":"heldout_domain_exposed", "items":[x['id'] for x in document['items'] if x['domain']==item['domain']]})
        ref = item["reference"]
        _keys(ref, {"status", "origin", "reviewer_ids", "artifact", "labels"}, "reference")
        if ref["status"] not in {"pending", "author_reference", "independent_review"}:
            raise ValueError("invalid_reference_status")
        if ref["origin"] not in {"human_review", "controlled_reference", "author", "model_output", "pending"}:
            raise ValueError("invalid_reference_origin")
        reviewers = set(_strings(ref["reviewer_ids"], "reviewer_ids", nonempty=False))
        if ref["artifact"] is not None:
            _file(evidence_root, ref["artifact"])
        labels = ref["labels"]
        if not isinstance(labels, dict) or set(labels)-set(questions):
            raise ValueError("invalid_reference_questions")
        for qid, choices in labels.items():
            _strings(choices, "accepted_choices")
            if set(choices)-set(questions[qid]["criteria"]):
                raise ValueError("invalid_reference_choice")
        if ref["status"] == "pending" and labels:
            raise ValueError("pending_reference_cannot_supply_labels")
        if labels:
            labeled.append(iid)
        if ref["status"] == "independent_review":
            if not reviewers or developers & reviewers or not ref["artifact"] or ref["origin"] not in {"human_review", "controlled_reference"} or not labels:
                issues.append({"code":"independent_reference_not_established", "items":[iid]})
            else:
                independent.append(iid)
    for group, members in groups.items():
        if len({split for _, split in members}) > 1:
            issues.append({"code":"source_group_crosses_splits", "group":group, "items":sorted(i for i,_ in members)})
    affected = {i for issue in issues for i in issue["items"]}
    eligible = [i for i in independent if i not in affected]
    # Count connected declared root/file groups, not rows or questions.
    parents = {i:i for i in ids}
    def find(i):
        while parents[i] != i:
            parents[i] = parents[parents[i]]; i = parents[i]
        return i
    for members in groups.values():
        for iid,_ in members[1:]:
            parents[find(iid)] = find(members[0][0])
    components = {i:find(i) for i in ids}
    return {"schema":"ontology-engineering.evaluation-audit/v1", "dataset_sha256":digest(document),
            "evaluator_sha256":digest(Path(__file__).read_bytes()),
            "dataset_id":document["dataset_id"], "issues":issues, "items":len(ids),
            "declared_source_groups":len(set(components.values())), "group_by_item":components,
            "labeled_items":labeled, "independent_reference_items":eligible,
            "heldout_reference_items":[i["id"] for i in document["items"] if i["split"]=="heldout" and i["id"] in eligible],
            "reference_authentication":"external_not_performed", "statistical_independence":"not_established",
            "qualification":"not_assessed"}


def evaluate(document, evidence_root, prepared, journal_path):
    """Measure the exact journal, retaining missing outputs in the denominator."""
    audit = audit_dataset(document, evidence_root, catalog=prepared["catalog"])
    records = read_candidates(prepared, journal_path)
    by_id = {r["item"]["id"]:r for r in records}
    if set(by_id) != {i["id"] for i in document["items"]}:
        raise ValueError("evaluation_run_inventory_mismatch")
    rows, confusion, metrics = [], defaultdict(Counter), defaultdict(Counter)
    class_counts = defaultdict(lambda:defaultdict(Counter))
    for item in document["items"]:
        actual = by_id[item["id"]]
        source = actual["item"]["source"]
        if any(source[k] != item["source"][k] for k in ("path", "sha256", "selection")):
            raise ValueError("evaluation_source_identity_mismatch")
        if item["claim"] != {k:actual["item"]["claim"][k] for k in item["claim"]} or item["domain"] != actual["item"]["claim"]["domain"]:
            raise ValueError("evaluation_claim_identity_mismatch")
        answers = {a["question_id"]:a for a in actual["answers"]}
        for qid, accepted in item["reference"]["labels"].items():
            if qid not in actual["item"]["question_ids"]:
                raise ValueError("reference_question_not_scheduled")
            candidate = answers.get(qid)
            choice = candidate["answer"]["choice"] if candidate else None
            hit = choice in accepted if choice is not None else False
            stratum = '|'.join([item["split"], item["domain"], qid, item["reference"]["status"]])
            count = metrics[stratum]
            count['reference_questions'] += 1
            count['answered'] += candidate is not None
            count['matches_reference'] += hit
            count['unanswered'] += candidate is None
            count['unknown_answers'] += choice in prepared['catalog'].get('unknown_choices', [])
            confusion[stratum][','.join(accepted)+' -> '+str(choice)] += 1
            if len(accepted)==1:
                for label in prepared['catalog']['questions'][qid]['criteria']:
                    counts=class_counts[stratum][label]
                    counts['true_positive'] += choice==label and accepted[0]==label
                    counts['false_positive'] += choice==label and accepted[0]!=label
                    counts['false_negative'] += choice!=label and accepted[0]==label
            rows.append({'item_id':item['id'], 'question_id':qid, 'accepted_choices':accepted,
                'choice':choice, 'matches_reference':hit, 'reference_status':item['reference']['status'],
                'independent_reference_eligible':item['id'] in audit['independent_reference_items'],
                'candidate_id':candidate['candidate_id'] if candidate else None,
                'execution_kind':candidate['execution_kind'] if candidate else 'not_completed'})
    output = {}
    for key, count in metrics.items():
        n = count['reference_questions']
        output[key] = {**count, 'observed_agreement':count['matches_reference']/n if n else None,
                       'confusion':dict(confusion[key])}
        output[key]['classes_single_reference_only']={}
        for label,c in class_counts[key].items():
            tp,fp,fn=c['true_positive'],c['false_positive'],c['false_negative']
            output[key]['classes_single_reference_only'][label]={**c,
                'precision':tp/(tp+fp) if tp+fp else None,'recall':tp/(tp+fn) if tp+fn else None}
    return {'schema':'ontology-engineering.judgment-evaluation/v1', 'dataset_audit':audit,
            'evaluator_sha256':digest(Path(__file__).read_bytes()),
            'deployment_sha256':prepared['deployment_sha256'], 'input_sha256':prepared['input_sha256'],
            'rows':rows, 'strata':output, 'qualification':'not_assessed',
            'meaning':'Observed agreement at the recorded reference quality. No automatic compatibility, fact admission or accuracy guarantee.'}


def retrieval_audit(full_catalogue, selected, required):
    """Operational recall accounting; relevance references remain externally reviewed."""
    for value, name in [(full_catalogue,'full_catalogue'), (selected,'selected'), (required,'required')]:
        _strings(value, name, nonempty=name=='full_catalogue')
    if set(selected)-set(full_catalogue):
        raise ValueError('retrieval_returned_unknown_pattern')
    expressible = set(required) & set(full_catalogue)
    missing = sorted(expressible-set(selected))
    return {'retrieval_misses':missing, 'reference_patterns_absent_from_catalogue':sorted(set(required)-set(full_catalogue)),
            'required_in_catalogue':len(expressible), 'retrieved_required':len(expressible & set(selected)),
            'observed_recall':len(expressible & set(selected))/len(expressible) if expressible else None,
            'suggested_expansion':missing, 'ontology_delta_created':False}


def cost_report(document):
    """Account for known and missing costs; unknown rates never become zero."""
    _keys(document, {'schema','currency','comparison_scope','expected_components','entries'}, 'cost ledger')
    if document['schema'] != 'ontology-engineering.evaluation-cost/v1':
        raise ValueError('unsupported_evaluation_cost')
    _text(document['currency'],'currency'); _text(document['comparison_scope'],'comparison_scope')
    expected = set(_strings(document['expected_components'], 'expected_components'))
    if not isinstance(document['entries'],list):
        raise ValueError('invalid_cost_entries')
    total, missing, observed, seen = 0.0, [], [], set()
    for entry in document['entries']:
        _keys(entry, {'id','component','quantity','unit','price_per_unit','basis','artifact_sha256'}, 'cost entry')
        if entry['id'] in seen or entry['component'] not in expected:
            raise ValueError('invalid_cost_identity')
        seen.add(entry['id'])
        for k in ('id','unit','basis','artifact_sha256'):
            _text(entry[k],k)
        for k in ('quantity','price_per_unit'):
            v=entry[k]
            if v is not None and (type(v) not in (int,float) or not math.isfinite(v) or v<0):
                raise ValueError('invalid_cost_quantity_or_rate')
        if entry['quantity'] is None or entry['price_per_unit'] is None:
            missing.append(entry['id'])
        else:
            amount=entry['quantity']*entry['price_per_unit']; total+=amount
            observed.append({'id':entry['id'],'amount':amount,'basis':entry['basis']})
    absent = sorted(expected-{e['component'] for e in document['entries']})
    return {'currency':document['currency'],'comparison_scope':document['comparison_scope'],
            'recorded_subtotal':total, 'total_cost':None if missing or absent else total,
            'missing_entries':missing, 'missing_components':absent, 'priced_entries':observed,
            'engineering_savings':'not_established', 'billing_reconciliation':'not_performed'}
