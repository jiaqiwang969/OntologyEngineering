"""Evaluation integrity on synthetic fixtures; no independent human labels here."""
from copy import deepcopy
import json
import subprocess
import sys

import pytest

from ontology_engineering.judgment_contracts import ROOT, digest, deployment_lock, prepare_batch
from ontology_engineering.judgment_batch import execute
from ontology_engineering.jev_transport import TransportError
from ontology_engineering.judgment_evaluation import SCHEMA, REFERENCE_SCHEMA, audit_dataset, evaluate, retrieval_audit, cost_report, reference_subject


def seal_fixture_review(item, tmp_path):
    """Synthetic controlled records for tests; never an actual human review."""
    ref = item['reference']
    artifact = tmp_path / f"review-{item['id']}.json"
    artifact.write_text(json.dumps({'schema':REFERENCE_SCHEMA, 'review_id':'fixture-review-'+item['id'],
        'subject_sha256':digest(reference_subject(item)), 'reviewer_ids':ref['reviewer_ids'],
        'origin':ref['origin'], 'labels':ref['labels'],
        'rationale':'Synthetic schema fixture only; no actual independent human review.',
        'completed_at':'2026-09-23T00:00:00Z'}))
    ref['artifact'] = {'path':artifact.name, 'sha256':digest(artifact.read_bytes())}


def dataset(tmp_path):
    doc={'schema':SCHEMA,'dataset_id':'test-fixture','developer_ids':['author'],
         'heldout_domains':['cost'],'items':[]}
    for n,split in enumerate(['development','calibration','heldout']):
        file=tmp_path/f'source-{n}.txt';file.write_text(f'Independent test bytes {n}')
        doc['items'].append({'id':str(n),'source':{'path':file.name,'sha256':digest(file.read_bytes()),'selection':{'start':0,'end':len(file.read_text())}},
            'claim':{'id':f'claim-{n}','statement':'synthetic claim','subject_revision':'r1','scope':'scope'},
            'domain':'cost' if n==2 else 'cad','split':split,'root_ids':[f'root-{n}'],
            'used_for_development':n==0,'reference':{'status':'independent_review','origin':'human_review',
            'reviewer_ids':['fixture-reviewer'],'artifact':None,
            'labels':{'source_relation':['supports']}}})
        seal_fixture_review(doc['items'][-1],tmp_path)
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
    doc=dataset(tmp_path);(tmp_path/'review-0.json').write_text('changed review')
    with pytest.raises(ValueError,match='digest_mismatch'):
        audit_dataset(doc,tmp_path)


@pytest.mark.parametrize('mutation',[
    'arbitrary-attachment','another-item-review','labels','reviewer','origin',
    'revision','claim','selection','source-bytes','question-definition','empty-rationale'])
def test_hash_valid_but_wrong_reference_cannot_enter_independent_set(tmp_path,mutation):
    doc=dataset(tmp_path);item=doc['items'][2];ref=item['reference']
    catalog=deepcopy(prepare_catalog())
    if mutation=='arbitrary-attachment':
        artifact=tmp_path/'unrelated.json';artifact.write_text('{"scope":"not a review"}')
        ref['artifact']={'path':artifact.name,'sha256':digest(artifact.read_bytes())}
    elif mutation=='another-item-review':ref['artifact']=deepcopy(doc['items'][1]['reference']['artifact'])
    elif mutation=='labels':ref['labels']['source_relation']=['contradicts']
    elif mutation=='reviewer':ref['reviewer_ids']=['different-declared-reviewer']
    elif mutation=='origin':ref['origin']='controlled_reference'
    elif mutation=='revision':item['claim']['subject_revision']='r2'
    elif mutation=='claim':item['claim']['statement']='a different claim'
    elif mutation=='selection':item['source']['selection']['start']=1
    elif mutation=='source-bytes':
        path=tmp_path/item['source']['path'];path.write_text('changed source bytes')
        item['source']['sha256']=digest(path.read_bytes())
    elif mutation=='question-definition':catalog['questions']['source_relation']['instructions']+=' Changed meaning.'
    elif mutation=='empty-rationale':
        path=tmp_path/ref['artifact']['path'];review=json.loads(path.read_text());review['rationale']=''
        path.write_text(json.dumps(review));ref['artifact']['sha256']=digest(path.read_bytes())
    result=audit_dataset(doc,tmp_path,catalog=catalog)
    assert '2' not in result['independent_reference_items']
    assert '2' not in result['heldout_reference_items']
    assert any(issue['code'].startswith('reference_artifact_') and '2' in issue['items'] for issue in result['issues'])
    assert result['qualification']=='not_assessed'


def prepare_catalog():
    from ontology_engineering.judgment_contracts import contracts
    return contracts()['catalog']


def test_cli_exports_pending_subjects_without_inventing_references(tmp_path):
    doc=dataset(tmp_path)
    for item in doc['items']:
        item['reference']={'status':'pending','origin':'pending','reviewer_ids':[],'artifact':None,'labels':{}}
    path=tmp_path/'dataset.json';path.write_text(json.dumps(doc));output=tmp_path/'subjects.json'
    run=subprocess.run([sys.executable,str(ROOT/'scripts/judgment_evaluation.py'),'reference-subjects',
        '--input',str(path),'--evidence-root',str(tmp_path),'--output',str(output)],capture_output=True,text=True)
    assert run.returncode==0,run.stderr
    result=json.loads(output.read_text())
    assert len(result['subjects'])==3 and result['questions']==prepare_catalog()['questions']
    assert result['reference_review']=='not_performed' and result['qualification']=='not_assessed'
    assert result['dataset_audit']['independent_reference_items']==[]
    for item,subject in zip(doc['items'],result['subjects']):
        assert subject['subject_sha256']==digest(reference_subject(item))


def test_unbound_reference_metrics_are_not_labeled_independent(tmp_path):
    root=ROOT/'examples/judgment_intake'
    batch=json.loads((root/'batch.json').read_text());batch['items']=batch['items'][:1]
    (tmp_path/'sources.json').write_bytes((root/'sources.json').read_bytes())
    prepared=prepare_batch(batch,tmp_path,deployment_lock(model='jev-1.13.0'))
    class Failure:
        kind='fixture';identity='reference-metric-fault/v1'
        def __call__(self,payload):raise TransportError('http_410')
    journal=tmp_path/'journal.sqlite';execute(prepared,journal,Failure())
    item=batch['items'][0]
    artifact=tmp_path/'unrelated.json';artifact.write_text('{"scope":"not a review"}')
    doc={'schema':SCHEMA,'dataset_id':'unbound-reference','developer_ids':['author'],'heldout_domains':[],
        'items':[{'id':item['id'],'source':{k:item['source'][k] for k in ['path','sha256','selection']},
            'claim':{k:item['claim'][k] for k in ['id','statement','subject_revision','scope']},
            'domain':item['claim']['domain'],'split':'development','root_ids':['fixture'],'used_for_development':True,
            'reference':{'status':'independent_review','origin':'human_review','reviewer_ids':['fixture-reviewer'],
                'artifact':{'path':artifact.name,'sha256':digest(artifact.read_bytes())},
                'labels':{'source_relation':['supports']}}}]}
    result=evaluate(doc,tmp_path,prepared,journal)
    assert all(key.endswith('|unverified_reference') for key in result['strata'])
    assert result['rows'][0]['reference_quality']=='unverified_reference'
    assert result['rows'][0]['independent_reference_eligible'] is False
    assert result['qualification']=='not_assessed'


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
