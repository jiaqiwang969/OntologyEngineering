#!/usr/bin/env python3
"""Audit evaluation sources and report exact candidate-journal comparisons."""
import argparse
import json
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
from ontology_engineering.jev_transport import strict_json
from ontology_engineering.judgment_contracts import prepare_batch
from ontology_engineering.judgment_evaluation import audit_dataset, evaluate, cost_report, reference_subjects
from ontology_engineering.judgment_review import write_new


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode',choices=['audit','evaluate','cost','reference-subjects'])
    p.add_argument('--input',type=Path,required=True)
    p.add_argument('--evidence-root',type=Path)
    p.add_argument('--batch',type=Path)
    p.add_argument('--lock',type=Path)
    p.add_argument('--journal',type=Path)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args()
    doc=strict_json(a.input.read_bytes())
    if a.mode=='cost':
        result=cost_report(doc)
    else:
        if not a.evidence_root:p.error('source evaluation requires --evidence-root')
        if a.mode=='audit':result=audit_dataset(doc,a.evidence_root)
        elif a.mode=='reference-subjects':result=reference_subjects(doc,a.evidence_root)
        else:
            if not all([a.batch,a.lock,a.journal]):p.error('evaluate requires batch, lock and journal')
            prepared=prepare_batch(strict_json(a.batch.read_bytes()),a.evidence_root,strict_json(a.lock.read_bytes()))
            result=evaluate(doc,a.evidence_root,prepared,a.journal)
    a.output.parent.mkdir(parents=True,exist_ok=True)
    write_new(a.output,result)
    print(json.dumps({'mode':a.mode,'output':str(a.output),'qualification':result.get('qualification','not_assessed')}))


if __name__=='__main__':main()
