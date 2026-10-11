#!/usr/bin/env python3
"""Judge actual facade observations; negative controls exercise the same functions."""
import collections,copy,json,sys
from pathlib import Path
import corpus


def current_fixture(fixture):
    """Apply recorded admission changes only to the exact historical inputs."""
    provenance=Path(__file__).resolve().parents[3]/'crates/openbindings-schema-evaluator-test-support/fixtures/provenance.json'
    translated=copy.deepcopy(fixture)
    for change in json.loads(provenance.read_text())['runtime_policy_translations']:
        selected=[(g,c) for g in translated['groups'] for c in g['cases'] if c['id']==change['case']]
        assert len(selected)==1, 'policy translation must identify one existing case'
        group,case=selected[0]
        assert group['document']==change['original_document']
        assert [translated['resources'][r] for r in group['resources']]==change['original_resources']
        assert case['value']==change['original_value']
        assert case['expected']==change['from_expected']
        assert case['refusal_kind']==change['from_refusal_kind']
        assert change['to_expected']=='satisfies'
        case.update(expected=change['to_expected'],refusal_kind=None,refusal_reason='')
    return translated


def pointer_exists(value,pointer):
    if pointer=='':return True
    if not isinstance(pointer,str) or not pointer.startswith('/'):return False
    for part in pointer[1:].split('/'):
        part=part.replace('~1','/').replace('~0','~')
        if isinstance(value,dict) and part in value:value=value[part]
        elif isinstance(value,list) and part.isdecimal() and (part=='0' or not part.startswith('0')) and int(part)<len(value):value=value[int(part)]
        else:return False
    return True


def judge_value(case,result,sources):
    failures=[];tag=result.get('outcome');detail=result.get('detail',{})
    permitted=case['optional_capability'] is not None and tag=='no-verdict' and detail.get('reason')=='unsupported-capability'
    if tag!=('fails' if case['expected']=='mismatch' else case['expected']) and not permitted:failures.append('wrong verdict')
    if tag=='no-verdict':
        if detail.get('reason')!=case.get('refusal_kind'):failures.append('wrong refusal distinction')
        if not detail.get('code','').strip() or not detail.get('message','').strip():failures.append('unexplained refusal')
        if detail.get('reason') not in ['unsupported-capability','conservative-preparation','resource-unavailable']:failures.append('unjustified refusal reason')
    if tag=='fails':
        problems=result.get('problems',[])
        if not problems:failures.append('missing problems')
        if result.get('problemsComplete') is not True:failures.append('unexpected diagnostic truncation')
        paths=sorted({p.get('instancePointer','<absent>') for p in problems})
        if case['acceptable_paths'] is not None and paths not in [sorted(set(p)) for p in case['acceptable_paths']]:failures.append('wrong instance paths')
        value=json.loads(case['value'])
        if any(not pointer_exists(value,p) for p in paths):failures.append('nonexistent instance location')
        for problem in problems:
            location=problem.get('schemaLocation')
            if not location:continue
            resource=location.get('resource');pointer=location.get('pointer')
            if resource is not None and resource.startswith('https://sdk-program.openbindings.invalid/'):failures.append('generated identity leaked')
            if resource not in sources or not pointer_exists(sources[resource],pointer):failures.append('nonexistent original schema location')
    return failures


def judge_evaluator(fixture,observed):
    expected_ids={g['id'] for g in fixture['groups']};counts=collections.Counter(g.get('id') for g in observed)
    accounting=[f'evaluator group {i}: observed {counts[i]} times' for i in sorted(expected_ids) if counts[i]!=1]
    accounting += ['unknown evaluator group '+str(i) for i in counts.keys()-expected_ids]
    groups={g.get('id'):g for g in observed};rows=[]
    for group in fixture['groups']:
        found=groups.get(group['id'],{});got=found.get('values',[])
        if found.get('executed') is not True:accounting.append('unexecuted evaluator group '+group['id'])
        if len(got)!=len(group['cases']):accounting.append('wrong evaluator value count '+group['id'])
        sources={None:json.loads(group['document'])}
        sources.update({uri:json.loads(text) for uri,text in fixture.get('builtin_resources',{}).items()})
        sources.update({fixture['resources'][r]['uri']:json.loads(fixture['resources'][r]['documentJson']) for r in group['resources']})
        for i,case in enumerate(group['cases']):
            result=got[i] if i<len(got) else {};failures=judge_value(case,result,sources)
            rows.append({'id':case['id'],'status':'fail' if failures else 'pass','expected':case['expected'],'actual':result,'failures':failures})
    return rows,accounting


def controls():
    case={'id':'control','value':'{"a":7}','expected':'fails','acceptable_paths':[['/a']],'optional_capability':None,'refusal_kind':None}
    valid={'outcome':'fails','problemsComplete':True,'problems':[{'instancePointer':'/a','schemaLocation':{'resource':None,'pointer':'/type'}}]}
    sources={None:{'type':'string'}}
    assert not judge_value(case,valid,sources)
    bad=[]
    bad.append({**valid,'outcome':'mismatch'}) # obsolete public result tag is not a synonym
    bad.append({'outcome':'satisfies'})
    for field,value in [('instancePointer','/missing'),('schemaLocation',{'resource':None,'pointer':'/missing'}),('schemaLocation',{'resource':'https://sdk-program.openbindings.invalid/private','pointer':''})]:
        x=copy.deepcopy(valid);x['problems'][0][field]=value;bad.append(x)
    x=copy.deepcopy(valid);x['problemsComplete']=False;bad.append(x)
    for result in bad:assert judge_value(case,result,sources)
    optional={**case,'optional_capability':'unicode','refusal_kind':'unsupported-capability'}
    for reason in ['cancelled','limit-exceeded','evaluator-failure','undefined','resource-unavailable']:
        assert judge_value(optional,{'outcome':'no-verdict','detail':{'reason':reason,'code':'x','message':'x'}},sources)
    fixture={'resources':{},'groups':[{'id':'group','document':'{"type":"string"}','resources':[],'cases':[case]}]}
    group={'id':'group','executed':True,'values':[valid]}
    assert not judge_evaluator(fixture,[group])[1]
    for mutation in [[],[group,group],[{**group,'id':'unknown'}],[{**group,'executed':False}],[{**group,'values':[]}],[{**group,'values':[valid,valid]}]]:
        assert judge_evaluator(fixture,mutation)[1]
    raw=json.loads((Path(__file__).resolve().parents[3]/'crates/openbindings-schema-evaluator-test-support/fixtures/cases.json').read_text())
    changed=current_fixture(raw)
    target='adversarial/resource-uri-names-another-id'
    raw_group=next(g for g in raw['groups'] if g['id']==target)
    new_case=next(g for g in changed['groups'] if g['id']==target)['cases'][0]
    assert raw_group['cases'][0]['expected']=='no-verdict' and new_case['expected']=='satisfies'
    assert not judge_value(new_case,{'outcome':'satisfies'}, {})
    assert judge_value(new_case,{'outcome':'no-verdict','detail':{'reason':'conservative-preparation','code':'old','message':'old policy'}}, {})
    for field in ['id','expected','refusal_kind','value','document','resource_uri','resource_document']:
        altered=copy.deepcopy(raw)
        g=next(g for g in altered['groups'] if g['id']==target)
        if field=='document':g['document']='{}'
        elif field.startswith('resource_'):
            altered['resources'][g['resources'][0]]['uri' if field=='resource_uri' else 'documentJson']='changed'
        else:g['cases'][0][field]='changed'
        try:current_fixture(altered)
        except AssertionError:pass
        else:raise AssertionError('changed input inherited policy translation: '+field)
    return {'status':'pass','wrong_results_rejected':len(bad)+6,'accounting_mutations_rejected':6,'policy_input_mutations_rejected':7}


def main():
    if sys.argv[1]=='--controls':print(json.dumps(controls()));return
    folder=Path(sys.argv[1]);cases=corpus.load();actual=json.loads((folder/'core.json').read_text());by={x['id']:x for x in actual}
    failures=[c['id'] for c in cases if c['id'] not in by or not corpus.judge(c,by[c['id']])];accounting=corpus.reconcile(cases,actual)
    fixture=current_fixture(json.loads((Path(__file__).resolve().parents[3]/'crates/openbindings-schema-evaluator-test-support/fixtures/cases.json').read_text()))
    observed=json.loads((folder/'evaluator.json').read_text());rows,other=judge_evaluator(fixture,observed);accounting+=other
    report={'core':len(cases),'core_passed':len(cases)-len(failures),'core_failures':failures,'evaluator_cases':len(rows),'evaluator_passed':sum(r['status']=='pass' for r in rows),'evaluator_failures':[r for r in rows if r['status']!='pass'],'reconciliation':accounting,'controls':controls()}
    (folder/'judged-evaluator.json').write_text(json.dumps(rows,indent=2)+'\n');(folder/'judgment.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2));raise SystemExit(bool(failures or accounting or report['evaluator_failures']))

if __name__=='__main__':main()
