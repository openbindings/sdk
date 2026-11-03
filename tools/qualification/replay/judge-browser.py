#!/usr/bin/env python3
"""Judge actual facade observations; negative controls exercise the same functions."""
import collections,copy,json,sys
from pathlib import Path
import corpus


def current_fixture(fixture):
    """Read the current reviewed suite; historical input identity must agree exactly."""
    import hashlib
    directory=Path(__file__).resolve().parents[3]/'crates/openbindings-schema-evaluator-test-support/fixtures'
    manifest=json.loads((directory/'alignment.json').read_text(encoding='utf-8'))
    for name,digest in manifest['sha256'].items():
        assert hashlib.sha256((directory/name).read_bytes()).hexdigest()==digest, 'qualification input drift: '+name
    assert fixture==json.loads((directory/'cases.json').read_text(encoding='utf-8')), 'historical fixture identity changed'
    current=json.loads((directory/'current-suite.json').read_text(encoding='utf-8'))
    assert len(current['groups'])==manifest['value_groups']
    assert sum(len(g['cases']) for g in current['groups'])==manifest['value_cases']
    return current


def pointer_exists(value,pointer):
    if pointer=='':return True
    if not isinstance(pointer,str) or not pointer.startswith('/'):return False
    for part in pointer[1:].split('/'):
        at = 0
        while at < len(part):
            if part[at] == '~':
                if at + 1 == len(part) or part[at + 1] not in '01':
                    return False
                at += 2
            else:
                at += 1
        part=part.replace('~1','/').replace('~0','~')
        if isinstance(value,dict) and part in value:value=value[part]
        elif isinstance(value,list) and part.isascii() and part.isdecimal() and (part=='0' or not part.startswith('0')) and int(part)<len(value):value=value[int(part)]
        else:return False
    return True


def judge_value(case,result,sources):
    failures=[];tag=result.get('status');detail=result.get('detail',{})
    # Capability refusal remains an observed gap, not a semantic pass.
    if tag!=case['expected']:failures.append('expected '+case['expected']+', observed '+str(tag))
    if tag=='no-verdict':
        if detail.get('reason')!=case.get('refusal_kind'):failures.append('wrong refusal distinction')
        if not detail.get('code','').strip() or not detail.get('message','').strip():failures.append('unexplained refusal')
        if detail.get('reason') not in ['unsupported-capability','conservative-preparation','resource-unavailable']:failures.append('unjustified refusal reason')
    if tag=='undefined':
        if not detail.get('code','').strip() or not detail.get('message','').strip():failures.append('unexplained undefined result')
        location=detail.get('location')
        if location:
            resource=location.get('sourceUri');pointer=location.get('pointer')
            if 'sourceUri' not in location:failures.append('missing original source coordinate')
            if resource is not None and resource.startswith('https://sdk-program.openbindings.invalid/'):failures.append('generated identity leaked')
            if resource not in sources or not pointer_exists(sources[resource],pointer):failures.append('nonexistent original schema location')
    if tag=='fails':
        problems=result.get('problems',[])
        if not problems and result.get('problemsComplete') is True:failures.append('complete diagnostics lack problems')
        paths=sorted({p['instance'] if isinstance(p.get('instance'),str) else '<absent>' for p in problems})
        if result.get('problemsComplete') is True and case['acceptable_paths'] is not None and paths not in [sorted(set(p)) for p in case['acceptable_paths']]:failures.append('wrong instance paths')
        value=json.loads(case['value'])
        if any(not pointer_exists(value,p) for p in paths):failures.append('nonexistent instance location')
        for problem in problems:
            location=problem.get('schema')
            if not location:
                failures.append('problem lacks a schema location');continue
            resource=location.get('sourceUri');pointer=location.get('pointer')
            if 'sourceUri' not in location:failures.append('missing original source coordinate')
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
    assert not pointer_exists({'a~2': 1}, '/a~2')
    assert pointer_exists({'a~2': 1}, '/a~02')
    assert not pointer_exists([1], '/٠')
    assert pointer_exists({'٠': 1}, '/٠')
    case={'id':'control','value':'{"a":7}','expected':'fails','acceptable_paths':[['/a']],'optional_capability':None,'refusal_kind':None}
    valid={'status':'fails','problemsComplete':True,'problems':[{'instance':'/a','schema':{'sourceUri':None,'pointer':'/type'}}]}
    sources={None:{'type':'string'}}
    assert not judge_value(case,valid,sources)
    bad=[]
    bad.append({**valid,'status':'mismatch'}) # obsolete public result tag is not a synonym
    bad.append({'status':'satisfies'})
    bad.append({k:v for k,v in valid.items() if k!='status'}|{'outcome':'fails'}) # the former discriminant is not read
    for field,value in [('instance','/missing'),('instance',None),('schema',None),('schema',{'resource':None,'pointer':'/type'}),('schema',{'sourceUri':None,'pointer':'/missing'}),('schema',{'sourceUri':'https://sdk-program.openbindings.invalid/private','pointer':''})]:
        x=copy.deepcopy(valid);x['problems'][0][field]=value;bad.append(x)
    x=copy.deepcopy(valid);x['problemsComplete']=False;assert not judge_value(case,x,sources)
    for result in bad:assert judge_value(case,result,sources)
    optional={**case,'optional_capability':'unicode','refusal_kind':'unsupported-capability'}
    for reason in ['cancelled','limit-exceeded','evaluator-failure','undefined','resource-unavailable']:
        assert judge_value(optional,{'status':'no-verdict','detail':{'reason':reason,'code':'x','message':'x'}},sources)
    # An undefined result an evaluator declines gives the stated refusal only.
    declined={**case,'expected':'undefined','optional_capability':'undefined-results','refusal_kind':'conservative-preparation'}
    assert judge_value(declined,{'status':'no-verdict','detail':{'reason':'conservative-preparation','code':'x','message':'x'}},sources)
    assert judge_value(declined,{'status':'no-verdict','detail':{'reason':'unsupported-capability','code':'x','message':'x'}},sources)
    undefined={'status':'undefined','detail':{'code':'proof','message':'established undefined result','location':{'sourceUri':None,'pointer':'/type'}}}
    semantic={**case,'expected':'undefined','refusal_kind':None}
    assert not judge_value(semantic,undefined,sources)
    assert judge_value(optional,undefined,sources)
    assert judge_value(semantic,{'status':'no-verdict','detail':{'reason':'undefined','code':'proof','message':'old representation'}},sources)
    assert judge_value(semantic,{'status':'undefined','detail':{}},sources)
    for location in [{'sourceUri':None,'pointer':'/missing'}, {'sourceUri':'https://sdk-program.openbindings.invalid/private','pointer':''}]:
        assert judge_value(semantic,{**undefined,'detail':{**undefined['detail'],'location':location}},sources)
    fixture={'resources':{},'groups':[{'id':'group','document':'{"type":"string"}','resources':[],'cases':[case]}]}
    group={'id':'group','executed':True,'values':[valid]}
    assert not judge_evaluator(fixture,[group])[1]
    for mutation in [[],[group,group],[{**group,'id':'unknown'}],[{**group,'executed':False}],[{**group,'values':[]}],[{**group,'values':[valid,valid]}]]:
        assert judge_evaluator(fixture,mutation)[1]
    raw=json.loads((Path(__file__).resolve().parents[3]/'crates/openbindings-schema-evaluator-test-support/fixtures/cases.json').read_text(encoding='utf-8'))
    changed=current_fixture(raw)
    target='adversarial/resource-uri-names-another-id'
    raw_group=next(g for g in raw['groups'] if g['id']==target)
    new_case=next(g for g in changed['groups'] if g['id']==target)['cases'][0]
    assert raw_group['cases'][0]['expected']=='no-verdict' and new_case['expected']=='satisfies'
    assert not judge_value(new_case,{'status':'satisfies'}, {})
    assert judge_value(new_case,{'status':'no-verdict','detail':{'reason':'conservative-preparation','code':'old','message':'old policy'}}, {})
    # A schema in another dialect expects its semantic verdict; a truthful refusal is still a gap.
    dialect=next(c for g in changed['groups'] for c in g['cases'] if c['id'].startswith('suite/draft2020-12/optional/cross-draft.json#0/0 '))
    refusal={'status':'no-verdict','detail':{'reason':'unsupported-capability','code':'schema-dialect','message':'dialect not read'}}
    assert dialect['expected']=='satisfies' and dialect['optional_capability'] is None
    assert not judge_value(dialect,{'status':'satisfies'},{})
    assert judge_value(dialect,refusal,{})
    assert judge_value(dialect,{'status':'fails','problems':[]},{})
    assert judge_value(dialect,{**refusal,'detail':{**refusal['detail'],'reason':'conservative-preparation'}},{})
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
    return {'status':'pass','wrong_results_rejected':len(bad)+14,'accounting_mutations_rejected':6,'policy_input_mutations_rejected':7}


def main():
    if sys.argv[1]=='--controls':print(json.dumps(controls()));return
    folder=Path(sys.argv[1]);cases=corpus.load();actual=json.loads((folder/'core.json').read_text(encoding='utf-8'));by={x['id']:x for x in actual}
    failures=[c['id'] for c in cases if c['id'] not in by or not corpus.judge(c,by[c['id']])];accounting=corpus.reconcile(cases,actual)
    fixture=current_fixture(json.loads((Path(__file__).resolve().parents[3]/'crates/openbindings-schema-evaluator-test-support/fixtures/cases.json').read_text(encoding='utf-8')))
    observed=json.loads((folder/'evaluator.json').read_text(encoding='utf-8'));rows,other=judge_evaluator(fixture,observed);accounting+=other
    report={'core':len(cases),'core_passed':len(cases)-len(failures),'core_failures':failures,'evaluator_cases':len(rows),'evaluator_passed':sum(r['status']=='pass' for r in rows),'evaluator_failures':[r for r in rows if r['status']!='pass'],'reconciliation':accounting,'controls':controls()}
    (folder/'judged-evaluator.json').write_text(json.dumps(rows,indent=2)+'\n');(folder/'judgment.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2));raise SystemExit(bool(failures or accounting or report['evaluator_failures']))

if __name__=='__main__':main()
