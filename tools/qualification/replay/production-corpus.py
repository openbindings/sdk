#!/usr/bin/env python3
"""Run independently decoded pinned cases through the production observer."""
import argparse,json,subprocess,time,uuid
from pathlib import Path
import corpus
p=argparse.ArgumentParser();p.add_argument('--stage',choices=['m2','full'],default='full');p.add_argument('--binary',required=True);args=p.parse_args()
cases=corpus.load();selected=[c for c in cases if args.stage=='full' or c['action'] in ('validate-document','resolve-operation','check-dependency-kind')]
requests=''.join(json.dumps(corpus.request(c))+'\n' for c in selected)
start=time.monotonic();run=subprocess.run([args.binary],input=requests,text=True,capture_output=True,timeout=120)
observed=[json.loads(line) for line in run.stdout.splitlines()];by_id={r['id']:r for r in observed}
accounting=corpus.reconcile(selected,observed);failures=[]
for c in selected:
 a=by_id.get(c['id'])
 if a is None or not corpus.judge(c,a):failures.append({'id':c['id'],'expected':c['expected'],'actual':a})
out=corpus.OUT/'corpus'/str(uuid.uuid4());out.mkdir(parents=True)
request_snapshot=corpus.OUT/'corpus'/f'{args.stage}-requests.jsonl'
if request_snapshot.exists():assert request_snapshot.read_text()==requests,'frozen request stream changed'
else:request_snapshot.write_text(requests)
(out/f'{args.stage}-requests.jsonl').write_text(requests);(out/f'{args.stage}-actual.jsonl').write_text(run.stdout)
report={'evidence':str(out),'stage':args.stage,'total_inventory':len(cases),'selected':len(selected),'observed':len(observed),'passed':len(selected)-len(failures),'accounting_errors':accounting,'failures':failures,'exit_code':run.returncode,'stderr':run.stderr,'elapsed_seconds':time.monotonic()-start}
(out/f'{args.stage}-report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k not in ('failures','stderr')},indent=2));print(json.dumps([{'id':f['id'],'expected':f['expected'],'outcome':(f['actual'] or {}).get('outcome'),'findings':(f['actual'] or {}).get('findings',[])[:3]} for f in failures[:8]],indent=2))
if run.returncode or failures or accounting:raise SystemExit(1)
