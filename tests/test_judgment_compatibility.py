"""Exact shadow compatibility on synthetic runs, never model qualification."""
from copy import deepcopy
import json
import sqlite3

import pytest

from ontology_engineering.judgment_batch import execute
from ontology_engineering.judgment_contracts import ROOT, deployment_lock, digest, prepare_batch
from ontology_engineering.judgment_compatibility import CATALOG_SCHEMA, check_combination, inspect_entry, record_combination

EXAMPLES=ROOT/'examples/judgment_intake'


class Fixture:
    kind='fixture'
    identity='compatibility-synthetic-fixture/v1'
    def __init__(self): self.calls=0
    def __call__(self,payload):
        self.calls+=1
        answers={}
        for qid,q in payload['questions'].items():
            choice=next(iter(q['criteria']))
            answers[qid]={'type':'choice','choice':choice,'confidence':1.0,
                         'probabilities':{c:float(c==choice) for c in q['criteria']}}
        return {'model':payload['model'],'answers':answers,'usage':{'input_tokens':0,'output_tokens':0}}


def prepared(**kwargs):
    document=json.loads((EXAMPLES/'batch.json').read_text())
    document['items']=document['items'][:1]
    return prepare_batch(document,EXAMPLES,deployment_lock(model='jev-1.13.0',**kwargs))


def snapshot(tmp_path,name='baseline',**kwargs):
    p=prepared(**kwargs)
    db=tmp_path/(name+'.sqlite');execute(p,db,Fixture())
    output=tmp_path/name
    record_combination(p,EXAMPLES,db,output)
    return p,{'deployment_sha256':p['deployment_sha256'],
              'entry':{'path':name+'/entry.json','sha256':digest((output/'entry.json').read_bytes())}}


def catalog(*entries):
    return {'schema':CATALOG_SCHEMA,'catalog_id':'synthetic-combinations','entries':list(entries)}


def test_exact_combination_recomputed_with_scope_and_execution_boundary(tmp_path):
    p,entry=snapshot(tmp_path)
    result=check_combination(p,catalog(entry),tmp_path,execution_kind='fixture')
    assert result['status']=='supported_shadow'
    assert result['model_quality_qualification']=='not_established'
    assert result['observed']['questions']==13
    live=check_combination(p,catalog(entry),tmp_path)
    assert live['status']=='unsupported'
    assert live['reasons']==['recorded_execution_kind_does_not_cover_request']
    assert inspect_entry(tmp_path/'baseline/entry.json')['allowed_use']=='shadow_candidate_only'


def test_individually_observed_parts_do_not_support_unlisted_combination(tmp_path):
    _,a=snapshot(tmp_path,'a',strategy='batched',workers=2)
    _,b=snapshot(tmp_path,'b',strategy='single',workers=1)
    combination=prepared(strategy='batched',workers=1)
    result=check_combination(combination,catalog(a,b),tmp_path,execution_kind='fixture')
    assert result['status']=='unsupported' and result['reasons']==['whole_combination_not_listed']
    transport=Fixture()
    with pytest.raises(ValueError,match='unsupported_deployment_combination'):
        execute(combination,tmp_path/'unlisted.sqlite',transport,compatibility_catalog=catalog(a,b),compatibility_root=tmp_path)
    assert transport.calls==0 and not (tmp_path/'unlisted.sqlite').exists()


@pytest.mark.parametrize('mutation,error',[
    ('input','evidence_digest_mismatch'),('journal','candidate_raw_response_mismatch'),
    ('source','source_hash_mismatch'),('metadata','observation_mismatch'),
    ('authority','unsupported_compatibility_authority'),('catalog_subject','entry_identity_mismatch')])
def test_changed_artifacts_cannot_import_a_compatibility_pass(tmp_path,mutation,error):
    p,ref=snapshot(tmp_path)
    output=tmp_path/'baseline';entry_path=output/'entry.json'
    data=json.loads(entry_path.read_text())
    if mutation=='input': (output/'input.json').write_text('{}')
    elif mutation=='journal':
        with sqlite3.connect(output/'journal.sqlite') as db:
            db.execute("UPDATE candidates SET candidate_id='changed'")
        data['journal']['sha256']=digest((output/'journal.sqlite').read_bytes())
    elif mutation=='source':
        (output/'sources'/p['items'][0]['source']['path']).write_text('{}')
    elif mutation=='metadata': data['observed']['questions']=9999
    elif mutation=='authority': data['model_quality_qualification']='qualified'
    elif mutation=='catalog_subject': data['deployment_sha256']='f'*64
    entry_path.write_text(json.dumps(data))
    ref['entry']['sha256']=digest(entry_path.read_bytes())
    with pytest.raises(ValueError,match=error): check_combination(p,catalog(ref),tmp_path,execution_kind='fixture')


@pytest.mark.parametrize('field',['project_id','access_scope','domain','questions'])
def test_recorded_scope_cannot_be_silently_broadened(tmp_path,field):
    p,entry=snapshot(tmp_path)
    request=deepcopy(p)
    if field in {'project_id','access_scope'}: request['input'][field]='different'
    elif field=='domain': request['items'][0]['claim']['domain']='unobserved-domain'
    else: request['items'][0]['question_ids'].append('unobserved-question')
    result=check_combination(request,catalog(entry),tmp_path,execution_kind='fixture')
    assert result['status']=='unsupported' and result['reasons']


def test_domain_and_question_scopes_cannot_be_cross_combined(tmp_path):
    document=json.loads((EXAMPLES/'batch.json').read_text())
    document['items']=document['items'][:2]
    baseline=prepared();required=baseline['catalog']['required_questions']
    extras=[q for q in baseline['catalog']['questions'] if q not in required][:2]
    for i,item in enumerate(document['items']):
        item['claim']['domain']='domain-'+str(i)
        item['question_ids']=[*required,extras[i]]
    p=prepare_batch(document,EXAMPLES,baseline['deployment'])
    db=tmp_path/'run.sqlite';execute(p,db,Fixture())
    record_combination(p,EXAMPLES,db,tmp_path/'observed')
    ref={'deployment_sha256':p['deployment_sha256'],
         'entry':{'path':'observed/entry.json','sha256':digest((tmp_path/'observed/entry.json').read_bytes())}}
    changed=deepcopy(p);changed['items'][0]['question_ids']=[*required,extras[1]]
    result=check_combination(changed,catalog(ref),tmp_path,execution_kind='fixture')
    assert result['status']=='unsupported'
    assert result['reasons']==['scope_not_covered:profile:domain-0']


def test_partial_run_and_replay_do_not_establish_observed_combination(tmp_path):
    p=prepared();db=tmp_path/'run.sqlite';execute(p,db,Fixture())
    replay=deepcopy(p);replay['input']['batch_id']='replay'
    replay['input_sha256']=digest(replay['input'])
    execute(replay,db,Fixture())
    with pytest.raises(ValueError,match='observed_execution_kind'):
        record_combination(replay,EXAMPLES,db,tmp_path/'replayed')
    with sqlite3.connect(db) as con:
        con.execute('DELETE FROM candidates WHERE rowid=(SELECT MIN(rowid) FROM candidates)')
    with pytest.raises(ValueError,match='run_incomplete'):
        record_combination(p,EXAMPLES,db,tmp_path/'partial')


def test_live_transport_requires_explicit_experiment_before_any_call(tmp_path):
    class NeverCalled:
        kind='live';identity='no-network-test'
        def __call__(self,payload): raise AssertionError('must not run')
    with pytest.raises(ValueError,match='catalog_or_explicit_experiment'):
        execute(prepared(),tmp_path/'denied.sqlite',NeverCalled())
    assert not (tmp_path/'denied.sqlite').exists()


def test_catalog_scope_is_retained_on_resume_and_cannot_change_silently(tmp_path):
    p,entry=snapshot(tmp_path)
    c=catalog(entry);db=tmp_path/'accepted.sqlite'
    first=execute(p,db,Fixture(),compatibility_catalog=c,compatibility_root=tmp_path)
    assert first['run_admission']['status']=='supported_shadow'
    with pytest.raises(ValueError,match='resume_identity'):
        execute(p,db,Fixture())
    second=execute(p,db,Fixture(),compatibility_catalog=c,compatibility_root=tmp_path)
    assert first['run_admission']==second['run_admission']
