"""Source selection, complete configuration identity and mandatory question boundaries."""
from copy import deepcopy
from pathlib import Path
import pytest

from ontology_engineering.judgment_contracts import BATCH_SCHEMA,contracts,deployment_lock,digest,prepare_batch


def batch(tmp_path):
    text='计算报告：只在给定支撑条件下成立；实际支撑状态尚未验证。'
    path=tmp_path/'source.txt';path.write_text(text)
    return {'schema':BATCH_SCHEMA,'batch_id':'source-check','project_id':'synthetic','access_scope':'local-test',
            'items':[{'id':'one','source':{'id':'source-1','path':'source.txt','sha256':digest(path.read_bytes()),
                                         'media_type':'text/plain','selection':{'start':0,'end':len(text)},
                                         'lineage_group':'synthetic-origin','context_status':'complete_for_question'},
                      'claim':{'id':'c1','statement':'实际装配件满足要求。','subject_id':'assembly','subject_revision':'R2',
                               'scope':'real-assembly','domain':'cad','cq':'支持条件是否成立？'},
                      'question_ids':['source_relation','scope_relation'],'required_methods':['pattern-scope','pattern-claim']}]}


def test_source_range_and_conditions_are_preserved(tmp_path):
    raw=batch(tmp_path);lock=deployment_lock(model='jev-1.13.0')
    p=prepare_batch(raw,tmp_path,lock)
    assert '实际支撑状态尚未验证' in p['items'][0]['state']['source_text']
    assert p['items'][0]['source']['selection']==raw['items'][0]['source']['selection']
    assert p['deployment']['compatibility_status']=='unvalidated_experiment'


@pytest.mark.parametrize('change',['hash','path','range','question','obligation','extra_field','too_long'])
def test_invalid_intake_cannot_be_silently_repaired(tmp_path,change):
    raw=batch(tmp_path);item=raw['items'][0];lock=deployment_lock(model='jev-1.13.0')
    if change=='hash':item['source']['sha256']='0'*64
    if change=='path':item['source']['path']='../source.txt'
    if change=='range':item['source']['selection']['end']=100000
    if change=='question':item['question_ids']=['source_relation']
    if change=='obligation':item['required_methods']=[]
    if change=='extra_field':item['accepted_fact']=True
    if change=='too_long':lock=deployment_lock(model='jev-1.13.0',max_state_chars=100)
    with pytest.raises(ValueError):prepare_batch(raw,tmp_path,lock)


def test_source_symlink_is_not_followed(tmp_path):
    raw=batch(tmp_path)
    (tmp_path/'link.txt').symlink_to(tmp_path/'source.txt');raw['items'][0]['source']['path']='link.txt'
    with pytest.raises(ValueError,match='symlink'):prepare_batch(raw,tmp_path,deployment_lock(model='jev-1.13.0'))


def test_lock_changes_require_new_evaluation_identity(tmp_path):
    raw=batch(tmp_path);lock=deployment_lock(model='jev-1.13.0')
    altered=deepcopy(lock);altered['adapter']['version']='unreviewed'
    with pytest.raises(ValueError,match='combination_or_adapter'):prepare_batch(raw,tmp_path,altered)
    with pytest.raises(ValueError,match='exact_model'):deployment_lock(model='jev-latest')


def test_questions_and_patterns_are_semantica_owned():
    c=contracts()
    assert {p['id'] for p in c['patterns']['patterns']}=={'P-ID','P-CLAIM','P-SCOPE','P-REALIZE','P-CHANGE'}
    assert len(c['catalog']['questions'])==13 and c['catalog']['fact_authority'] is False
    assert c['identity']['package_id']=='semantica.engineering.judgment-intake'
