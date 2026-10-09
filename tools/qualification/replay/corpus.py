"""Independent corpus loading and judgment; never imports an SDK implementation."""
from pathlib import Path
import base64
import collections
import json

OUT = Path(__file__).resolve().parent.parent
CORPUS = OUT / 'inputs/spec/conformance'
PROFILE = {
    'recursive-references': True, 'document-resource-dynamic-scope': True,
    'supplied-resources': True, 'repeated-member-detection': True,
    'exact-numbers': True, 'ecma262-unicode-property-escapes': False,
    'draft-07-dialect': False, 'exact-lone-surrogate-strings': False,
}

class Number(str):
    """A JSON number token; it must never pass through a binary float."""

def read(path):
    return json.loads(Path(path).read_bytes(), parse_int=Number, parse_float=Number)

def exact_text(value):
    if isinstance(value, Number): return str(value)
    if value is None: return 'null'
    if value is True: return 'true'
    if value is False: return 'false'
    if isinstance(value, str): return json.dumps(value, ensure_ascii=True)
    if isinstance(value, list): return '['+','.join(map(exact_text,value))+']'
    if isinstance(value, dict): return '{'+','.join(json.dumps(k)+':'+exact_text(v) for k,v in value.items())+'}'
    raise TypeError(type(value))

def carriage(given):
    present=[k for k in ('document','documentText','documentBase64') if k in given]
    if len(present)!=1: raise ValueError('Exactly one document carriage is required')
    kind=present[0]
    if kind=='document': return exact_text(given[kind]).encode()
    if kind=='documentText': return given[kind].encode('utf-8')
    return base64.b64decode(given[kind],validate=True)

def load():
    manifest=read(CORPUS/'manifest.json'); cases=[]; counted={}
    for file in manifest['files']:
        data=read(CORPUS/file['path']); counted[file['path']]=len(data['tests'])
        if len(data['tests'])!=int(file['tests']): raise ValueError('Manifest test count mismatch')
        for index,test in enumerate(data['tests']):
            expected={'outcome':'conformant' if test['valid'] else 'non-conformant',
                      'violates':test.get('violates',[]),'notViolated':test.get('notViolated',[])}
            cases.append({'id':f"{file['path']}#/tests/{index}",'file':file['path'],'action':'validate-document','given':test,'expected':expected})
    for file in manifest['scenarioFiles']:
        data=read(CORPUS/file['path']); counted[file['path']]=len(data['scenarios'])
        if data['format']!='openbindings.core-scenarios@3' or len(data['scenarios'])!=int(file['scenarios']): raise ValueError('Scenario format/count mismatch')
        for case in data['scenarios']: cases.append(dict(case,file=file['path']))
    actual=sorted(c['id'] for c in cases); expected=read(OUT/'inputs/blueprint/case-inventory.json')['ids']
    if actual!=expected or len(set(actual))!=len(actual): raise ValueError('Frozen case inventory mismatch')
    disk={str(p.relative_to(CORPUS)) for d in ('document','scenarios') for p in (CORPUS/d).glob('*.json')}
    if disk!=set(counted): raise ValueError('Uncounted fixture file')
    return cases

def request(case):
    given=case['given']; data={'documentBase64':base64.b64encode(carriage(given)).decode()}
    for field in ('name','operation','side','dependency','kind','binding'):
        if field in given: data[field]=given[field]
    if 'values' in given: data['valuesJson']=[exact_text(v) for v in given['values']]
    if 'resources' in given: data['resources']=[{'uri':r['uri'],'documentJson':exact_text(r['document'])} for r in given['resources']]
    return {'id':case['id'],'action':case['action'],'given':data}

def lacking(features,profile):
    unknown=set(features)-profile.keys()
    if unknown: raise ValueError('Undeclared features: '+str(unknown))
    return any(not profile[f] for f in features)

def judge_value(expected,actual,features,profile):
    form={'result':expected} if isinstance(expected,str) else expected
    dependent=form.get('dependsOn',features); unsupported=lacking(dependent,profile)
    want=form['result']; result=actual['outcome']
    if result=='no-contract': return want=='no-contract'
    if result=='no-verdict':
        reason=actual.get('detail',{}).get('reason')
        return reason in ('conservative-preparation','resource-unavailable','unsupported-capability') and (want in ('undefined','external') or form.get('orNoVerdict',False) or unsupported)
    if result=='mismatch' and (not actual.get('problems') or actual.get('problems_complete',actual.get('problemsComplete')) is not True):return False
    return result=={'satisfies':'satisfies','fails':'mismatch'}.get(want)

def judge(case,actual,profile=PROFILE):
    if actual.get('id')!=case['id'] or actual.get('executed') is not True: return False
    want=case['expected']; action=case['action']
    unsupported=lacking(want.get('dependsOn',[]),profile)
    if action=='validate-document':
        result=actual['outcome']; required=want.get('violates',[]); forbidden=want.get('notViolated',[])
        if want['outcome']=='conformant': return result=='conformant' or result=='undetermined' and unsupported
        return result=='non-conformant' and all(actual['rules'].get(r)=='violated' for r in required) and all(actual['rules'].get(r)!='violated' for r in forbidden)
    if action=='resolve-operation':
        return actual['outcome']==want['outcome'] and (want['outcome']=='not-found' or actual['operationKey']==want['operationKey'] and sorted(actual['bindingKeys'])==sorted(want.get('bindingKeys',[])))
    if action=='check-dependency-kind': return actual['outcome']==want['outcome']
    if action=='validate-operation-values':
        return len(actual['values'])==len(want['results']) and all(judge_value(e,a,want.get('dependsOn',[]),profile) for e,a in zip(want['results'],actual['values']))
    if action=='check-examples':
        claims={'true':'satisfies','false':'fails','no-claim':'no-contract','external':'external','undefined':'undefined'}
        expected={(k,s):v for k,example in want['examples'].items() for s,v in example.items()}
        observed={(a['example'],a['side']):a for a in actual['examples']}
        return len(observed)==len(actual['examples']) and expected.keys()==observed.keys() and all(judge_value(claims[e],observed[k],[],profile) for k,e in expected.items())
    raise ValueError('Unknown action '+action)

def reconcile(cases,results):
    wanted={c['id'] for c in cases}; counted=collections.Counter(r['id'] for r in results)
    errors=[]
    if set(counted)!=wanted: errors.append('missing or unexpected case')
    if any(n!=1 for n in counted.values()): errors.append('duplicate case')
    if any(r.get('executed') is not True for r in results): errors.append('unexecuted case')
    return errors

def controls():
    cases=load(); valid=[{'id':c['id'],'executed':True} for c in cases]
    checks={'all_435_accounted':not reconcile(cases,valid),
            'reject_missing':bool(reconcile(cases,valid[1:])),
            'reject_duplicate':bool(reconcile(cases,valid+[valid[0]])),
            'reject_unexpected':bool(reconcile(cases,valid+[{'id':'fabricated','executed':True}])),
            'reject_unexecuted':bool(reconcile(cases,[dict(r,executed=False) if i==0 else r for i,r in enumerate(valid)])),
            'exact_integer':exact_text(read_text('{"n":9007199254740993}'))=='{"n":9007199254740993}',
            'reject_wrong_verdict':not judge_value('satisfies',{'outcome':'mismatch'},[],PROFILE),
            'reject_no_contract_for_undefined':not judge_value('undefined',{'outcome':'no-contract'},[],PROFILE),
            'reject_no_verdict_for_absence':not judge_value('no-contract',{'outcome':'no-verdict','detail':{'reason':'resource-unavailable'}},[],PROFILE),
            'reject_unsupported_misuse':not judge_value('satisfies',{'outcome':'no-verdict'},['exact-numbers'],PROFILE)}
    for n in range(1,14):
        rule=f'OBI-{n:02}'; case=next(c for c in cases if rule in c['expected'].get('violates',[]))
        result={'id':case['id'],'executed':True,'outcome':'non-conformant','rules':{r:'violated' for r in case['expected']['violates']}}
        checks[rule+'_positive_control']=judge(case,result)
        result['rules'][rule]='satisfied';checks[rule+'_wrong_rule_detected']=not judge(case,result)
    return {'scope':'Independent harness controls; synthetic responses are not Rust SDK evidence','count':len(cases),'checks':checks,'passed':all(checks.values())}

def read_text(text): return json.loads(text,parse_int=Number,parse_float=Number)

if __name__=='__main__':
    result=controls();print(json.dumps(result,indent=2));raise SystemExit(not result['passed'])
