from pathlib import Path
import json,hashlib,subprocess,sys,importlib.util
P=Path(__file__).resolve().parent;ROOT=P.parents[3];SDK=ROOT/'sdk';OLD=ROOT/'receipts/2026-11-02-preparation/original-default-performance/root-gate-order-build'
sys.path.insert(0,str(SDK/'tools/evaluator-bench'));import compare
# Identical frozen generator bytes; no request or expectation substitution.
a=P/'baseline-instrument/fixtures.py';b=P/'candidate-instrument/fixtures.py';assert a.read_bytes()==b.read_bytes()
spec=importlib.util.spec_from_file_location('frozen_review_fixtures',a);f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f);fixtures=f.fixtures();(P/'frozen-fixtures.json').write_text(json.dumps(fixtures,separators=(',',':'))+'\n')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
peer=json.loads((P/'release-peer.json').read_text());before=json.loads((OLD/'build.json').read_text());bins={'baseline':OLD/'candidate','candidate':P/'candidate'};expected={'baseline':before['binary'],'candidate':{'sha256':peer['binary_sha256'],'provenance':peer['provenance']}}
for side,binary in bins.items():assert sha(binary)==expected[side]['sha256'];assert compare.reported_sources(binary)==expected[side]['provenance']
assert compare.tree_digest(SDK/'tools/evaluator-bench',compare.INSTRUMENT_SOURCES)==peer['provenance']['instrument']
assert compare.tree_digest(SDK,compare.SDK_SOURCES)==peer['provenance']['sdk']
for name,digest in peer['root_inputs'].items():assert sha(SDK/name)==digest
for name,digest in peer['source_hashes'].items():assert sha(SDK/name)==digest
mismatches=[]
for path,digest in before['production'].items():
 name=str(Path(path).relative_to('sdk'));data=subprocess.check_output(['git','show','4e4cdfb:'+name],cwd=SDK)
 if hashlib.sha256(data).hexdigest()!=digest:mismatches.append(name)
assert not mismatches,mismatches
rows=[]
with(P/'release-observations.jsonl').open('w')as out:
 for name,fixture in fixtures.items():
  for mode in fixture['modes']:
   request={k:v for k,v in fixture.items()if k!='modes'};request.update(mode=mode,iterations=1,engine='sdk');encoded=json.dumps(request,separators=(',',':'));row={'name':name,'mode':mode,'request_sha256':hashlib.sha256(encoded.encode()).hexdigest(),'sides':{}}
   for side,binary in bins.items():
    run=subprocess.run([str(binary)],input=encoded,capture_output=True,text=True);entry={'returncode':run.returncode,'stdout':run.stdout,'stderr':run.stderr};row['sides'][side]=entry
    if run.returncode==0:entry['observation']=json.loads(run.stdout)['observation']
   row['equal']=all(v['returncode']==0 for v in row['sides'].values())and row['sides']['baseline']['observation']==row['sides']['candidate']['observation'];rows.append(row);out.write(json.dumps(row,separators=(',',':'))+'\n');out.flush()
 for side,binary in bins.items():assert sha(binary)==expected[side]['sha256']
summary={'scope':'complete observation replay, not a timing comparison','fixtures':len(fixtures),'rows':len(rows),'equal':sum(r['equal']for r in rows),'errors':[{'name':r['name'],'mode':r['mode']}for r in rows if not r['equal']],'binaries':expected,'baseline_production_equals_4e4cdfb':True,'current_manifest_and_provenance_verified':True,'fixture_bytes_unchanged':True}
(P/'release-replay-summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps({k:v for k,v in summary.items()if k!='binaries'},indent=2));assert not summary['errors']
