"""Frozen protocol/validator shared by the R and Q runners; no timing calibration."""
from pathlib import Path
import gzip
import hashlib
import json
import math
import statistics

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / 'docs/evidence/openapi-auth-json'
EXECUTION = EVIDENCE / 'measurement-resolution'
PLAN = EVIDENCE / 'measurement-resolution-design/synthesized-plan.md'
SOURCE = ROOT / 'examples/openapi-json-path-consumer/examples/auth_resolution.rs'
SAMPLER = 'resolution-fixed-cyclic@1'
SLOTS = [0,0,1,0,2,0,3,1,1,2,1,3,2,2,3,3]
WORK = [('bearer-get','get',1024,64,'bearer'), ('key-post','post',1024,64,'key'),
        ('and-post','post',65536,64,'and'), ('large-token','get',1024,4096,'bearer'),
        ('anonymous-get-large','get',65536,0,'none'), ('anonymous-post-large','post',65536,0,'none')]
R_MAPS = [[0,0,2,2],[0,1,2,3],[1,0,3,2],[4,5,6,7]]
Q_MAPS = [[0,1,2,3],[4,5,6,7],[5,4,7,6]]
R_IDS = ['R-alias','R-common-normal','R-common-swapped','R-separate-normal']
Q_IDS = ['Q-real','Q-null-normal','Q-null-swapped']

def require(ok, why):
    if not ok:
        raise ValueError(why)

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')

def rotate(xs, n):
    n %= len(xs)
    return xs[n:]+xs[:n]

def cycle(s,r,p,c,h):
    x=rotate(SLOTS, (s+5*r+3*p+7*c+11*h)%16)
    return x[::-1] if (s+p+h+c)%2 else x

def specs(phase):
    return [(v,*w,l) for v in ['3.0.4','3.1.2'] for w in WORK
            if phase=='Q' or w[0]=='and-post' for l in ['binding','operation']]

def layout(phase,p,r,anon):
    spec=specs(phase)[r]
    extra=phase=='R' or (not anon and spec[5]!='none')
    hs=[0,1] if phase=='R' else [(p+r)%2]
    construction=hs[::-1] if phase=='R' and (p+r)%2 else hs[:]
    nc=4 if phase=='R' else 3 if extra else 1
    schedules=[]
    for s in range(7):
        for h in (hs[::-1] if phase=='R' and (s+p+r)%2 else hs):
            cs=rotate(list(range(nc)), (s+2*p+r+h) if phase=='R' else (s+p+r))
            if (s+p+(h if phase=='R' else r))%2: cs.reverse()
            for c in cs: schedules.append({'sample':s,'cohort':h,'configuration':c,'slots':cycle(s,r,p,c,h)})
    return hs,construction,nc,8 if extra else 4,schedules

def schedule_manifest():
    return [{'phase':phase,'process':p,'row':r,'anonymous_only':anon,
             'fixture':spec,'construction_order':layout(phase,p,r,anon)[1],
             'visits':layout(phase,p,r,anon)[4]}
            for phase in ['R','Q'] for anon in ([False] if phase=='R' else [False,True])
            for p in range(3) for r,spec in enumerate(specs(phase))]

def fixture_expected(spec, auth):
    v,w,method,size,tok,mech,layer=spec
    names=(['Key','Token'] if mech=='and' else ['Key'] if mech=='key' else ['Token']) if auth else []
    body='"'+'x'*(size-2)+'"'
    return names,body

def deltas(m):
    d,c=m[1]-m[0],m[3]-m[2]
    return {'D':d,'C':c,'R':c-d,'U':abs(d)+abs(c),'adequate':all(abs(x)<=1000 for x in [d,c,c-d])}

def validate_row(raw, phase,p,r,anon=False,preflight=False,frozen_fixtures=None):
    spec=specs(phase)[r];v,w,method,size,tok,mech,layer=spec
    for k,val in {'sampler':SAMPLER,'phase':phase,'process':p,'row':r,'anonymous_only':anon,'preflight_only':preflight,
                  'version':v,'workload':w,'bytes':size,'token_bytes':tok,'layer':layer,'error':None}.items():
        require(raw.get(k)==val, f'row identity {k}: {raw.get(k)} != {val}')
    _,expected_body=fixture_expected(spec,False)
    require(raw.get('fixture_response')==expected_body,'finite response fixture')
    require(raw.get('fixture_input')==('{\"body\":'+expected_body+'}' if method=='post' else '{}'),'input fixture')
    hs,construction,nc,no,schedule=layout(phase,p,r,anon)
    require(raw['construction_order']==construction, 'cohort construction order')
    require([h['cohort'] for h in raw['cohorts']]==hs,'cohort set/order')
    chronology=[];summaries=[]
    target=800_000_000 if w=='and-post' else 200_000_000
    k=64 if size==65536 else 256
    for h in raw['cohorts']:
        hi=h['cohort'];objects=h['objects']
        require([o['id'] for o in objects]==list(range(no)),'physical object set')
        require(len({o['object_pointer'] for o in objects})==no,'physical objects alias')
        for o in objects:
            i=o['id'];shared=phase!='R' or i<4
            auth=phase=='Q' and not anon and mech!='none' and i in [1,3]
            require(o['constructor_rank']==(i if hi==0 else no-1-i),'constructor rank')
            require(o['fixture_id']==(0 if shared else i),'fixture id')
            require(o['direct']==(i%4<2) and o['authenticated']==auth,'object work mode')
            names,body=fixture_expected(spec,auth)
            source=json.loads(o['source']);context=json.loads(o['context'])
            if frozen_fixtures is not None:
                expected=frozen_fixtures['|'.join([v,w,str(auth)])]
                for key,val in {'source':o['source'],'context':o['context'],'input':raw['fixture_input'],'response':raw['fixture_response']}.items():
                    require(hashlib.sha256(val.encode()).hexdigest()==expected[key],'frozen fixture mismatch '+key)
            require(source['openapi']==v and source['servers']==[{'url':'https://measure.invalid'}], 'source edition/server')
            require(source['security']==([{n:[] for n in names}] if names else []),'source security')
            require(list(source['paths'])==['/p0'] and list(source['paths']['/p0'])==[method], 'source operation')
            require(context['credentials']=={n:'a'*tok for n in names}, 'attempt credentials')
            require(list(context['configuration'])==[('local.openapi30-json-probe.r1' if v=='3.0.4' else 'local.openapi-json-probe.r1')+'.security'], 'configuration kind key')
            require(next(iter(context['configuration'].values()))=={'schemes':names,'origins':{n:['https://measure.invalid'] for n in names}},'context selection/grants')
            require(o['context_bytes']==(len(o['context'].encode()) if auth else 0),'context byte count')
            if auth and mech=='and':require(o['context_bytes']==(334 if v=='3.0.4' else 332),'AND context size')
            for other in objects:
                same=phase!='R' or (i<4 and other['id']<4) or i==other['id']
                for ptr in ['input_pointer','response_pointer']:
                    require((o[ptr]==other[ptr])==same,'fixture allocation relation '+ptr)
        configs=h['configurations'];require(len(configs)==nc,'configuration count')
        for ci,c in enumerate(configs):
            mapping=(R_MAPS if phase=='R' else Q_MAPS)[ci]
            require(c['id']==(R_IDS if phase=='R' else Q_IDS)[ci] and c['configuration']==ci,'configuration namespace')
            require(c['map']==mapping,'label mapping')
            blocks=c['blocks'];require(c['capacity']==16384 and len(blocks)<=16359,'bounded records')
            if preflight:
                require(not blocks and c['samples_ns']==[[],[],[],[]], 'preflight timed data')
                continue
            require(len(c['samples_ns'])==4 and all(len(x)==7 for x in c['samples_ns']),'sample dimensions')
            recomputed=[[] for _ in range(4)]
            offset=0
            for s in range(7):
                bs=[b for b in blocks if b['sample']==s]
                require(bs and len(bs)%16==1 and 17<=len(bs)<=2337, 'prefix/cycle count')
                require(blocks[offset:offset+len(bs)]==bs,'config sample chronology');offset+=len(bs)
                slots=cycle(s,r,p,ci,hi)
                elapsed=[0]*4;calls=[0]*4
                for bi,b in enumerate(bs):
                    prefix=bi==0;cy,pos=divmod(max(0,bi-1),16);slot=slots[-1] if prefix else slots[pos]
                    require(b['prefix']==prefix and b['cycle']==cy and b['position']==pos,'block tags')
                    require(b['slot']==slot and b['predecessor']==(slot if prefix else slots[(pos-1)%16]),'cyclic predecessor')
                    require(b['calls']==k and b['object_id']==mapping[slot] and b['fixture_id']==objects[mapping[slot]]['fixture_id'],'block work/identity')
                    require(type(b['elapsed_ns']) is int and (b['elapsed_ns']==0 if prefix else b['elapsed_ns']>0), 'block elapsed')
                    chronology.append((b['ordinal'],s,hi,ci,bi))
                    if not prefix:
                        elapsed[slot]+=b['elapsed_ns'];calls[slot]+=k
                        if pos==15 and bi!=len(bs)-1:
                            require(any(x<target for x in elapsed),'extra cycle after target')
                require(all(x>=target for x in elapsed),'duration target')
                for slot in range(4):
                    measured=elapsed[slot]/calls[slot];reported=c['samples_ns'][slot][s]
                    require(math.isfinite(reported) and abs(reported-measured)<1e-6,'sample arithmetic')
                    recomputed[slot].append(measured)
            require(offset==len(blocks),'unexpected sample')
            m=list(map(statistics.median,recomputed))
            summaries.append({'phase':phase,'process':p,'row':r,'cohort':hi,'configuration':ci,'id':c['id'],'medians_ns':m,
                              'context_bytes':max(objects[x]['context_bytes'] for x in mapping),**deltas(m)})
    if not preflight:
        chronology.sort();require([x[0] for x in chronology]==list(range(len(chronology))),'global block chronology')
        visits=[]
        for _,s,h,c,b in chronology:
            if b==0:visits.append({'sample':s,'cohort':h,'configuration':c,'slots':cycle(s,r,p,c,h)})
        require(visits==schedule,'macro schedule')
        # A configuration sample is uninterrupted until the next registered visit.
        previous=None;closed=set()
        for _,s,h,c,b in chronology:
            key=(s,h,c)
            if key!=previous:
                require(key not in closed,'interleaved config blocks')
                if previous is not None:closed.add(previous)
                previous=key
    return summaries

def read_rows(path, phase,p,anon=False,preflight=False):
    opener=gzip.open if str(path).endswith('.gz') else open
    with opener(path,'rt') as f: raw=[json.loads(line) for line in f]
    require(len(raw)==len(specs(phase)), 'incomplete process row count')
    fixtures=json.loads((EXECUTION/'fixtures.json').read_text()) if (EXECUTION/'freeze.json').exists() else None
    return [item for r,row in enumerate(raw) for item in validate_row(row,phase,p,r,anon,preflight,fixtures)]

def summarize_r(rows):
    require(len(rows)==96,'R configuration total')
    eligible=[r for r in rows if r['configuration']<3]
    require(len(eligible)==72,'R eligible total')
    okay=all(x['adequate'] for x in eligible)
    return {'status':'method-adequate-for-one-qualification' if okay else 'inconclusive-stop',
            'product_qualified_complete':False,'observations':96,'eligible':72,'eligible_passes':sum(x['adequate'] for x in eligible),
            'component_passes':sum(abs(x[k])<=1000 for x in eligible for k in ['D','C','R']), 'component_total':216,
            'U_R':{str(r):max(x['U'] for x in eligible if x['row']==r) for r in range(4)},'rows':rows}

def summarize_q(rows, r_summary):
    gates=[];valid=True
    def get(p,r,name,c=0):
        matched=[x for x in rows if x['process']==p and x['row']==r and x['binary']==name and x['configuration']==c]
        require(len(matched)==1,'Q observation identity');return matched[0]
    require(len(rows)==240,'Q configuration total') # candidate 56 + closed 24, three pairs
    for r,spec in enumerate(specs('Q')):
        closed=[get(p,r,'closed')['medians_ns'][2] for p in range(3)]
        candidate=[get(p,r,'candidate') for p in range(3)]
        c=[x['medians_ns'][2] for x in candidate]
        spread=(max(closed)-min(closed))/statistics.median(closed)
        numeric=[y<=1.10*x+1000 for x,y in zip(closed,c)]
        gates.append({'row':r,'fixture':spec,'gate':'unsecured-regression','closed_ns':closed,'candidate_ns':c,'ceilings_ns':[1.10*x+1000 for x in closed],
                      'numeric_pass':numeric,'spread':spread,'valid':spread<=.15,'status':'fail' if not all(numeric) else 'pass' if spread<=.15 else 'inconclusive'})
        if spec[5]=='none':continue
        pair_results=[]
        da=[x['medians_ns'][1] for x in candidate];d0=[x['medians_ns'][0] for x in candidate]
        auth_spread=max((max(x)-min(x))/statistics.median(x) for x in [da,d0])
        for p,x in enumerate(candidate):
            nulls=[get(p,r,'candidate',i) for i in [1,2]]
            u=max(n['U'] for n in nulls)
            if r in [4,5,16,17]:u=max(u,r_summary['U_R'][str([4,5,16,17].index(r))])
            ceiling=max(0,x['D'])+5000+2*x['context_bytes'];slack=ceiling-x['C']
            okay=all(n['adequate'] for n in nulls) and slack>u and auth_spread<=.15
            pair_results.append({'pair':p,'medians_ns':x['medians_ns'],'context_bytes':x['context_bytes'],'ceiling_ns':ceiling,'delta_ns':x['C'],'allowance_used':x['C']/ceiling,'auth_ratio':x['medians_ns'][3]/x['medians_ns'][1],'slack_ns':slack,'U':u,'nulls':nulls,'numeric_pass':x['C']<=ceiling,'valid':okay})
        numeric=all(x['numeric_pass'] for x in pair_results);okay=all(x['valid'] for x in pair_results)
        gates.append({'row':r,'fixture':spec,'gate':'incremental-auth','pairs':pair_results,'spread':auth_spread,'valid':okay,'status':'fail' if not numeric else 'pass' if okay else 'inconclusive'})
    require(len(gates)==40,'Q gate total')
    complete=all(g['status']=='pass' for g in gates)
    return {'status':'failed' if any(g['status']=='fail' for g in gates) else 'performance-qualified-pending-independent-review' if complete else 'inconclusive-stop','product_qualified_complete':False,'gates':gates,'rows':rows}
