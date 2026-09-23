"""Evaluation integrity on synthetic fixtures; no independent human labels here."""
from copy import deepcopy
import json

import pytest

from ontology_engineering.judgment_contracts import ROOT, digest, deployment_lock, prepare_batch
from ontology_engineering.judgment_batch import execute
from ontology_engineering.jev_transport import TransportError
from ontology_engineering.judgment_evaluation import SCHEMA, audit_dataset, evaluate, retrieval_audit, cost_report


def dataset(tmp_path):
    doc={'schema':SCHEMA,'dataset_id':'test-fixture','developer_ids':['author'],
         'heldout_domains':['cost'],'items':[]}
    artifact=tmp_path/'review.json';artifact.write_text('{"scope":"synthetic test metadata, not a real review"}')
    for n,split in enumerate(['development','calibration','heldout']):
        file=tmp_path/f'source-{n}.txt';file.write_text(f'Independent test bytes {n}')
        doc['items'].append({'id':str(n),'source':{'path':file.name,'sha256':digest(file.read_bytes()),'selection':{'start':0,'end':len(file.read_text())}},
            'claim':{'id':f'claim-{n}','statement':'synthetic claim','subject_revision':'r1','scope':'scope'},
            'domain':'cost' if n==2 else 'cad','split':split,'root_ids':[f'root-{n}'],
            'used_for_development':n==0,'reference':{'status':'independent_review','origin':'human_review',
            'reviewer_ids':['fixture-reviewer'],'artifact':{'path':artifact.name,'sha256':digest(artifact.read_bytes())},
            'labels':{'source_relation':['supports']}}})
    return doc


def test_declared_split_and_reference_audit_retains_authentication_limit(tmp_path):
    result=audit_dataset(dataset(tmp_path),tmp_path)
    assert not result['issues'] and result['declared_source_groups']==3
    assert result['heldout_reference_items']==['2']
    assert result['reference_authentication']=='external_not_performed'
    assert result['qualification']=='not_assessed'


@pytest.mark.parametrize('mutation',['root','file','reviewer','model','exposure','domain'])
def test_leakage_and_self_labeled_references_are_not_independent(tmp_path,mutation):
    doc=dataset(tmp_path)
    if mutation=='root':doc['items'][2]['root_ids']=['root-0']
    elif mutation=='file':doc['items'][2]['source']=deepcopy(doc['items'][0]['source'])
    elif mutation=='reviewer':doc['items'][2]['reference']['reviewer_ids']=['author']
    elif mutation=='model':doc['items'][2]['reference']['origin']='model_output'
    elif mutation=='exposure':doc['items'][2]['used_for_development']=True
    elif mutation=='domain':doc['items'][0]['domain']='cost'
    result=audit_dataset(doc,tmp_path)
    assert result['issues']
    assert '2' not in result['heldout_reference_items']


def test_pending_reference_has_no_fake_accuracy_denominator(tmp_path):
    doc=dataset(tmp_path)
    for item in doc['items']:
        item['reference']={'status':'pending','origin':'pending','reviewer_ids':[],'artifact':None,'labels':{}}
    result=audit_dataset(doc,tmp_path)
    assert result['labeled_items']==[] and result['heldout_reference_items']==[]
    doc['items'][0]['reference']['labels']={'source_relation':['supports']}
    with pytest.raises(ValueError,match='pending_reference'):
        audit_dataset(doc,tmp_path)


def test_reference_artifact_changes_are_rejected(tmp_path):
    doc=dataset(tmp_path);(tmp_path/'review.json').write_text('changed review')
    with pytest.raises(ValueError,match='digest_mismatch'):
        audit_dataset(doc,tmp_path)


def test_retrieval_miss_is_distinct_from_absent_expression():
    result=retrieval_audit(['P-ID','P-CLAIM','P-SCOPE'],['P-ID'],['P-ID','P-SCOPE','unknown-pattern'])
    assert result['retrieval_misses']==['P-SCOPE']
    assert result['reference_patterns_absent_from_catalogue']==['unknown-pattern']
    assert result['observed_recall']==0.5 and not result['ontology_delta_created']
    assert retrieval_audit(['P-ID'],[],[])['observed_recall'] is None


def test_unknown_human_and_tool_costs_do_not_become_zero():
    doc={'schema':'ontology-engineering.evaluation-cost/v1','currency':'USD','comparison_scope':'same batch',
         'expected_components':['jev','human','ocr'],'entries':[
        {'id':'api','component':'jev','quantity':1000,'unit':'input_token','price_per_unit':0.000000042,
         'basis':'estimated list price','artifact_sha256':'a'*64},
        {'id':'review','component':'human','quantity':None,'unit':'minute','price_per_unit':None,
         'basis':'not measured','artifact_sha256':'b'*64}]}
    result=cost_report(doc)
    assert result['total_cost'] is None and result['recorded_subtotal']==pytest.approx(0.000042)
    assert result['missing_entries']==['review'] and result['missing_components']==['ocr']
    assert result['engineering_savings']=='not_established'


def test_missing_candidate_stays_in_evaluation_denominator(tmp_path):
    root=ROOT/'examples/judgment_intake'
    batch=json.loads((root/'batch.json').read_text());batch['items']=batch['items'][:1]
    prepared=prepare_batch(batch,root,deployment_lock(model='jev-1.13.0'))
    class Failure:
        kind='fixture';identity='evaluation-fault/v1'
        def __call__(self,payload):raise TransportError('http_410')
    journal=tmp_path/'journal.sqlite';execute(prepared,journal,Failure())
    item=batch['items'][0]
    doc={'schema':SCHEMA,'dataset_id':'missing-output','developer_ids':['author'],'heldout_domains':[],
        'items':[{'id':item['id'],'source':{k:item['source'][k] for k in ['path','sha256','selection']},
            'claim':{k:item['claim'][k] for k in ['id','statement','subject_revision','scope']},
            'domain':item['claim']['domain'],'split':'development','root_ids':['pilot'],'used_for_development':True,
            'reference':{'status':'author_reference','origin':'author','reviewer_ids':['author'],
                         'artifact':None,'labels':{'source_relation':['supports']}}}]}
    result=evaluate(doc,root,prepared,journal)
    metric=next(iter(result['strata'].values()))
    assert metric['reference_questions']==1 and metric['unanswered']==1 and metric['answered']==0
    assert metric['observed_agreement']==0 and result['rows'][0]['candidate_id'] is None
    assert metric['classes_single_reference_only']['supports']['recall']==0
    doc['items'][0]['claim']['subject_revision']='another revision'
    with pytest.raises(ValueError,match='claim_identity_mismatch'):
        evaluate(doc,root,prepared,journal)
