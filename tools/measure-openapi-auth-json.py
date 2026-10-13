"""Run registered auth/anonymous CPU gates once; preserve every raw process result.
Requires successful archive qualification and a closed-checkpoint consumer prepared
with the same auth_measure.rs and closed SDK sources (see evidence gate report).
"""
from pathlib import Path
import argparse,subprocess,os,json,statistics,hashlib,shutil,platform,gzip,time,runpy
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--baseline',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--balanced-qualified-by',type=Path,help='Completed adequate diagnostic directory; selects the preregistered balanced sampler');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target'));candidate=target/'openapi-json-path-qualification/archives/consumer';receipt=candidate.parent/'receipt.json'
if not receipt.is_file():raise SystemExit('Successful archive receipt required')
for rel,h in json.loads(receipt.read_text())['testedInputs'].items():
 if hashlib.sha256((ROOT/rel).read_bytes()).hexdigest()!=h:raise SystemExit('Stale qualification '+rel)
if (candidate/'examples/auth_measure.rs').read_bytes() != (a.baseline/'examples/auth_measure.rs').read_bytes():raise SystemExit('Measurement sources differ between closed/candidate')
balanced = a.balanced_qualified_by is not None
method_provenance = None
if balanced:
 diagnostic=json.loads((a.balanced_qualified_by/'summary.json').read_text())
 method_provenance=json.loads((a.balanced_qualified_by/'provenance.json').read_text())
 if diagnostic['status']!='method-adequate-for-one-qualification' or diagnostic['balanced_null_passes']!=12:
  raise SystemExit('Balanced method failed its preregistered adequacy guard')
 if method_provenance['measurement_sha256']!=hashlib.sha256((candidate/'examples/auth_measure.rs').read_bytes()).hexdigest():
  raise SystemExit('Diagnostic measurement source differs')
 if method_provenance['qualification_runner_sha256']!=hashlib.sha256(Path(__file__).read_bytes()).hexdigest():
  raise SystemExit('Qualification runner changed after preregistration')
 if method_provenance['receipt_sha256']!=hashlib.sha256(receipt.read_bytes()).hexdigest():
  raise SystemExit('Candidate qualification changed after diagnostic')
 if method_provenance['preregistration_sha256']!=hashlib.sha256((ROOT/'docs/evidence/openapi-auth-json/performance-investigation-plan.md').read_bytes()).hexdigest():
  raise SystemExit('Preregistration changed after diagnostic')
 environment=runpy.run_path(str(ROOT/'tools/investigate-openapi-auth-performance.py'))['environment']
for name,directory in [('candidate',candidate),('closed',a.baseline)]:
 r=subprocess.run(['cargo','build','--release','--manifest-path',str(directory/'Cargo.toml'),'--example','auth_measure'],text=True,capture_output=True);(a.output/(name+'-build.log')).write_text(r.stdout+r.stderr)
 if r.returncode:raise SystemExit('Build failed '+name)
 shutil.copyfile(target/'release/examples/auth_measure',a.output/name);(a.output/name).chmod(0o755)
raw=[]
process_environments=[]
if balanced:time.sleep(10)
for pair in range(3):
 for name in (['closed','candidate'] if pair%2==0 else ['candidate','closed']):
  command=[str(a.output/name)]+(['balanced'] if balanced else [])+(['anonymous-only'] if name=='closed' else [])+(['reverse'] if pair%2 else [])
  before=environment() if balanced else None
  print(f'pair {pair+1} {name}',flush=True);r=subprocess.run(command,text=True,capture_output=True)
  if balanced:
   with gzip.open(a.output/f'{pair}-{name}.stdout.json.gz','wb') as f:f.write(r.stdout.encode())
   process_environments.append({'pair':pair,'mode':name,'before':before,'after':environment(),'exit':r.returncode})
   (a.output/'environment.json').write_text(json.dumps(process_environments,indent=2)+'\n')
  else:(a.output/f'{pair}-{name}.stdout').write_text(r.stdout)
  (a.output/f'{pair}-{name}.stderr').write_text(r.stderr)
  if r.returncode:raise SystemExit('Measurement failed')
  observed=json.loads(r.stdout)
  if balanced:
   assert len(observed['rows'])==24
   modes=['direct-anonymous','direct-auth','adapter-anonymous','adapter-auth']
   orders=[[0,1,3,2],[1,2,0,3],[2,3,1,0],[3,0,2,1]]
   for row in observed['rows']:
    assert row['sampler']=='balanced-200ms' and not row['null_control']
    assert set(row['samples_ns'])==set(modes)
    for sample in range(7):
     blocks=[b for b in row['blocks'] if b['sample']==sample]
     assert len(blocks)%16==0
     for i,b in enumerate(blocks):
      cycle,within=divmod(i,16);order,position=divmod(within,4)
      assert (b['cycle'],b['order'],b['position'])==(cycle,order,position)
      assert b['mode']==modes[orders[order][3-position if pair%2 else position]]
      assert b['elapsed_ns']>=2_000_000 and b['calls']>0
     for mode in modes:
      matched=[b for b in blocks if b['mode']==mode];ns=sum(b['elapsed_ns'] for b in matched);calls=sum(b['calls'] for b in matched)
      assert ns>=200_000_000 and len(row['samples_ns'][mode])==7
      assert abs(row['samples_ns'][mode][sample]-ns/calls)<1e-6
  raw.append({'pair':pair,'mode':name,**observed})
if balanced:
 with gzip.open(a.output/'raw.json.gz','wb') as f:f.write(json.dumps(raw,separators=(',',':')).encode())
else:(a.output/'raw.json').write_text(json.dumps(raw,indent=2)+'\n')
rows=[]
for i,example in enumerate(raw[0]['rows']):
 selected=[r for r in raw if r['mode']=='candidate'];closed=[r for r in raw if r['mode']=='closed'];med=lambda r,m:statistics.median(r['rows'][i]['samples_ns'][m]);b=[med(r,'adapter-anonymous') for r in closed];c=[med(r,'adapter-anonymous') for r in selected];spread=(max(b)-min(b))/statistics.median(b)
 gate={'version':example['version'],'workload':example['workload'],'layer':example['layer'],'gate':'unsecured-regression','closed_ns':b,'candidate_ns':c,'spread':spread,'ceilings_ns':[1.10*x+1000 for x in b],'status':'inconclusive' if spread>0.15 else 'pass' if all(y<=1.10*x+1000 for x,y in zip(b,c)) else 'fail'};rows.append(gate)
 if not example['workload'].startswith('anonymous-'):
  da=[med(r,'direct-auth') for r in selected];d0=[med(r,'direct-anonymous') for r in selected];ca=[med(r,'adapter-auth') for r in selected];c0=c;size=selected[0]['rows'][i]['context_bytes'];allowance=5000+2*size;ceil=[max(0,x-y)+allowance for x,y in zip(da,d0)];delta=[x-y for x,y in zip(ca,c0)];spread=max((max(x)-min(x))/statistics.median(x) for x in [da,d0]);rows.append({'version':example['version'],'workload':example['workload'],'layer':example['layer'],'gate':'incremental-auth','context_bytes':size,'direct_auth_ns':da,'direct_anonymous_ns':d0,'candidate_auth_ns':ca,'candidate_anonymous_ns':c0,'delta_ns':delta,'allowance_used':[d/c for d,c in zip(delta,ceil)],'ceilings_ns':ceil,'auth_ratios':[x/y for x,y in zip(ca,da)],'spread':spread,'status':'inconclusive' if spread>0.15 else 'pass' if all(x<=y for x,y in zip(delta,ceil)) else 'fail'})
summary={'format':'openbindings.auth-json.performance@1','receipt_sha256':hashlib.sha256(receipt.read_bytes()).hexdigest(),'binary_sha256':{n:hashlib.sha256((a.output/n).read_bytes()).hexdigest() for n in ['candidate','closed']},'rows':rows,'notes':['Three fresh pairs, seven >=50ms interleaved samples; raw failures retained.','Programmable host CPU work; no RSS/heap/network/TLS/browser claim.','Direct path includes input conversion, per-attempt credential ownership, request prepare, host response, exact result conversion and destruction.','Auth source synthesis scalability measured separately; no cheaper reference denominator claimed equivalent.']}
if balanced:summary['notes'][0]='Three fresh pairs, seven >=200ms samples per condition in balanced2ms blocks; all raw blocks retained.'
summary.update(sampler='balanced-200ms' if balanced else 'legacy-50ms',method_provenance=method_provenance,runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),host=platform.platform(),rustc=subprocess.check_output(['rustc','--version'],text=True).strip(),measurement_source_sha256=hashlib.sha256((candidate/'examples/auth_measure.rs').read_bytes()).hexdigest())
(a.output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps({s:sum(r['status']==s for r in rows) for s in ['pass','fail','inconclusive']}))
raise SystemExit(1 if any(r['status']!='pass' for r in rows) else 0)
