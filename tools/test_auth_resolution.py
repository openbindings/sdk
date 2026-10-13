"""Deterministic protocol and adversarial receipt tests; no timed samples."""
import copy
import itertools
import json
import unittest
import auth_resolution as a


def synthetic_row(phase='R',p=0,r=0,anon=False):
    spec=a.specs(phase)[r];v,w,method,size,tok,mech,layer=spec
    hs,construction,nc,no,schedule=a.layout(phase,p,r,anon)
    row={'sampler':a.SAMPLER,'phase':phase,'process':p,'row':r,'anonymous_only':anon,'preflight_only':False,'version':v,'workload':w,'bytes':size,'token_bytes':tok,'layer':layer,'error':None,'construction_order':construction,'cohorts':[]}
    _,body=a.fixture_expected(spec,False)
    row.update(fixture_response=body,fixture_input=('{\"body\":'+body+'}' if method=='post' else '{}'))
    for hi in hs:
        objects=[]
        for i in range(no):
            auth=phase=='Q' and not anon and mech!='none' and i in [1,3]
            names,_=a.fixture_expected(spec,auth)
            kind='local.openapi30-json-probe.r1' if v=='3.0.4' else 'local.openapi-json-probe.r1'
            context=json.dumps({'credentials':{n:'a'*tok for n in names},'configuration':{kind+'.security':{'schemes':names,'origins':{n:['https://measure.invalid'] for n in names}}}},separators=(',',':'))
            shared=phase!='R' or i<4
            objects.append({'id':i,'constructor_rank':i if hi==0 else no-1-i,'fixture_id':0 if shared else i,'input_pointer':10 if shared else 10+i,'response_pointer':20 if shared else 20+i,'object_pointer':30+i,'direct':i%4<2,'authenticated':auth,'context_bytes':len(context) if auth else 0,'context':context,'source':json.dumps({'openapi':v,'servers':[{'url':'https://measure.invalid'}],'security':[{n:[] for n in names}] if names else [],'paths':{'/p0':{method:{}}}})})
        row['cohorts'].append({'cohort':hi,'objects':objects,'configurations':[{'id':(a.R_IDS if phase=='R' else a.Q_IDS)[c],'configuration':c,'map':(a.R_MAPS if phase=='R' else a.Q_MAPS)[c],'capacity':16384,'blocks':[],'samples_ns':[[] for _ in range(4)]} for c in range(nc)]})
    ordinal=0;k=64 if size==65536 else 256;target=800_000_000 if w=='and-post' else 200_000_000
    for visit in schedule:
        s,h,c,slots=visit['sample'],visit['cohort'],visit['configuration'],visit['slots']
        cohort=next(x for x in row['cohorts'] if x['cohort']==h);cfg=cohort['configurations'][c]
        for bi in range(17):
            prefix=bi==0;pos=max(0,bi-1);slot=slots[-1] if prefix else slots[pos];obj=cfg['map'][slot]
            cfg['blocks'].append({'sample':s,'cycle':0,'position':pos,'slot':slot,'predecessor':slot if prefix else slots[(pos-1)%16],'elapsed_ns':0 if prefix else target//4,'calls':k,'prefix':prefix,'ordinal':ordinal,'object_id':obj,'fixture_id':cohort['objects'][obj]['fixture_id']});ordinal+=1
        for slot in range(4):cfg['samples_ns'][slot].append(target/(4*k))
    return row


class Protocol(unittest.TestCase):
    def test_schedule_edges_and_counts(self):
        schedules=a.schedule_manifest()
        self.assertEqual(len(schedules),156)
        for entry in schedules:
            for visit in entry['visits']:
                slots=visit['slots'];self.assertEqual(sorted(zip(slots[-1:]+slots[:-1],slots)),list(itertools.product(range(4),repeat=2)))
        self.assertEqual(sum(len(x['visits']) for x in schedules if x['phase']=='R'),672)
        self.assertEqual(7*(1+146*16),16359)
    def test_known_arithmetic_and_all_mappings(self):
        rows=[]
        for p in range(3):
            for r in range(4):rows+=a.validate_row(synthetic_row(p=p,r=r),'R',p,r)
        result=a.summarize_r(rows)
        self.assertEqual(result['component_passes'],216)
        self.assertEqual(result['eligible_passes'],72)
        self.assertEqual(a.deltas([10,13,20,24]),{'D':3,'C':4,'R':1,'U':7,'adequate':True})
        self.assertEqual(__import__('statistics').median([30,5,80,10,20,90,4]),20)
    def test_rejects_corruption(self):
        base=synthetic_row()
        corruptions=[lambda x:x.update(phase='Q'),lambda x:x.update(error='cap'),
          lambda x:x['cohorts'][0]['configurations'][0].update(id='Q-real'),
          lambda x:x['cohorts'][0]['configurations'][0]['map'].__setitem__(1,1),
          lambda x:x['cohorts'][0]['objects'][1].update(input_pointer=11),
          lambda x:x['cohorts'][0]['objects'][0].update(constructor_rank=1),
          lambda x:x['cohorts'][0]['configurations'][0]['samples_ns'][0].__setitem__(0,1),
          lambda x:x['cohorts'][0]['configurations'][0]['blocks'][0].update(prefix=False),
          lambda x:x['cohorts'][0]['configurations'][0]['blocks'][0].update(elapsed_ns=100),
          lambda x:x['cohorts'][0]['configurations'][0]['blocks'][1].update(calls=63),
          lambda x:x['cohorts'][0]['configurations'][0]['blocks'][1].update(predecessor=99),
          lambda x:x['cohorts'][0]['configurations'][0]['blocks'][1].update(object_id=99),
          lambda x:x['cohorts'][0]['configurations'][0]['blocks'][1].update(ordinal=99999),
          lambda x:x['cohorts'][0]['configurations'][0]['blocks'].pop()]
        corruptions += [lambda x,e=e:x.update(error=e) for e in ['record-cap','cycle-cap','wall-time-cap','interrupted']]
        def wrong_key(x):
            obj=x['cohorts'][0]['objects'][0];context=json.loads(obj['context']);context['configuration']={'wrong.security':next(iter(context['configuration'].values()))};obj['context']=json.dumps(context)
        corruptions.append(wrong_key)
        for change in corruptions:
            x=copy.deepcopy(base);change(x)
            with self.assertRaises(ValueError):a.validate_row(x,'R',0,0)
    def test_exact_fixture_hashes(self):
        import hashlib
        raw=synthetic_row();obj=raw['cohorts'][0]['objects'][0]
        values={'source':obj['source'],'context':obj['context'],'input':raw['fixture_input'],'response':raw['fixture_response']}
        fixtures={'3.0.4|and-post|False':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in values.items()}}
        a.validate_row(raw,'R',0,0,frozen_fixtures=fixtures)
        for key in values:
            corrupt=copy.deepcopy(fixtures);corrupt['3.0.4|and-post|False'][key]='0'*64
            with self.assertRaises(ValueError):a.validate_row(raw,'R',0,0,frozen_fixtures=corrupt)
    def test_known_q_gates_and_uncertainty(self):
        rows=[]
        for p in range(3):
            for r,spec in enumerate(a.specs('Q')):
                for binary in ['closed','candidate']:
                    auth=binary=='candidate' and spec[5]!='none'
                    for c in range(3 if auth else 1):
                        m=[10000,10500,11000,12000] if auth and c==0 else [10000,10000,11000,11000]
                        rows.append({'process':p,'row':r,'binary':binary,'configuration':c,'medians_ns':m,'context_bytes':334 if auth and c==0 else 0,**a.deltas(m)})
        rs={'U_R':{str(i):0 for i in range(4)}}
        result=a.summarize_q(rows,rs);self.assertEqual(len(result['gates']),40)
        self.assertTrue(all(g['status']=='pass' for g in result['gates']))
        first=next(g for g in result['gates'] if g['gate']=='incremental-auth')['pairs'][0]
        self.assertEqual(first['auth_ratio'],12000/10500)
        self.assertEqual(first['allowance_used'],1000/(500+5000+668))
        edge=copy.deepcopy(rs);edge['U_R']['0']=first['slack_ns']
        self.assertEqual(next(g for g in a.summarize_q(rows,edge)['gates'] if g['row']==4 and g['gate']=='incremental-auth')['status'],'inconclusive')
        spread_rows=copy.deepcopy(rows)
        next(x for x in spread_rows if x['row']==0 and x['binary']=='closed' and x['process']==2)['medians_ns'][2]=20000
        self.assertFalse(a.summarize_q(spread_rows,rs)['gates'][0]['valid'])
        changed=copy.deepcopy(rows)
        next(x for x in changed if x['row']==4 and x['binary']=='candidate' and x['configuration']==1)['adequate']=False
        result=a.summarize_q(changed,rs);self.assertEqual(result['status'],'inconclusive-stop')
        rs['U_R']['0']=10000
        self.assertEqual(a.summarize_q(rows,rs)['status'],'inconclusive-stop')
        bad=copy.deepcopy(rows);x=next(x for x in bad if x['row']==4 and x['binary']=='candidate' and x['configuration']==0);x['C']=20000
        self.assertEqual(next(g for g in a.summarize_q(bad,rs)['gates'] if g['row']==4 and g['gate']=='incremental-auth')['status'],'fail')
        self.assertEqual(a.summarize_q(bad,rs)['status'],'failed')

class Checkpoints(unittest.TestCase):
    def fake(self, code, validator, error=None, seconds=3):
        import subprocess,sys,tempfile,time
        from run_auth_resolution import stream_checkpoints,stop_and_drain
        with tempfile.TemporaryFile() as raw:
            child=subprocess.Popen([sys.executable,'-c',code],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                if error:
                    with self.assertRaises(error):stream_checkpoints(child,raw,time.monotonic()+seconds,validator)
                else:self.assertEqual(stream_checkpoints(child,raw,time.monotonic()+seconds,validator),2)
            finally:
                stop_and_drain(child,raw)
                raw.seek(0);saved=raw.read()
            return saved
    def test_good_barrier(self):
        seen=[]
        saved=self.fake("import sys;print('0',flush=True);assert sys.stdin.readline()=='next\\n';print('1',flush=True);assert sys.stdin.readline()=='next\\n'",lambda line,index:seen.append((line,index)))
        self.assertEqual(seen,[(b'0',0),(b'1',1)]);self.assertEqual(saved,b'0\n1\n')
    def test_corrupt_and_truncated_are_retained(self):
        def reject(line,index):raise ValueError('corrupt fixture')
        saved=self.fake("import sys;print('bad',flush=True);sys.stdin.readline();print('should-not-run',flush=True)",reject,ValueError)
        self.assertEqual(saved,b'bad\n')
        saved=self.fake("import sys;sys.stdout.write('partial');sys.stdout.flush()",lambda *_:None,ValueError)
        self.assertEqual(saved,b'partial')
    def test_partial_row_after_cooperative_signal_is_retained(self):
        def interrupt(*_):raise KeyboardInterrupt()
        saved=self.fake("import signal,sys;signal.signal(signal.SIGTERM,lambda *_:(print('partial-row',flush=True),sys.exit(2)));print('ready',flush=True);sys.stdin.readline()",interrupt,KeyboardInterrupt)
        self.assertEqual(saved,b'ready\npartial-row\n')
    def test_deadline_and_interrupt(self):
        saved=self.fake("import time;print('0',flush=True);time.sleep(3)",lambda *_:None,ValueError,.1)
        self.assertEqual(saved,b'0\n')
        def interrupt(*_):raise KeyboardInterrupt()
        saved=self.fake("import sys;print('0',flush=True);sys.stdin.readline()",interrupt,KeyboardInterrupt)
        self.assertEqual(saved,b'0\n')

if __name__=='__main__':unittest.main()
