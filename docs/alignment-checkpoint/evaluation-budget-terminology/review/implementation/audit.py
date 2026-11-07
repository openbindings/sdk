from pathlib import Path
import subprocess,json,hashlib,difflib,importlib.util,sys
P=Path(__file__).resolve().parent;ROOT=P.parents[3];SDK=ROOT/'sdk';BASE='4e4cdfb'
def git(*args):return subprocess.check_output(['git',*args],cwd=SDK)
def norm(s):
 for a,b in [('evaluation_sdk_work_units','evaluation_steps'),('evaluation_sdk_reservation_bytes','program_bytes'),('evaluationSdkWorkUnits','evaluationSteps'),('evaluationSdkReservationBytes','programBytes')]:s=s.replace(a,b)
 return s
files=git('diff','--name-only',BASE).decode().splitlines();rows=[];extra=[]
for name in files:
 b=git('show',BASE+':'+name);a=(SDK/name).read_bytes();out=P/'source'/name;out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(a)
 pure=norm(a.decode())==b.decode();row={'path':name,'before_sha256':hashlib.sha256(b).hexdigest(),'after_sha256':hashlib.sha256(a).hexdigest(),'field_only_exact':pure}
 if name.endswith('.json'):
  def keys(v):
   if isinstance(v,dict):return{norm(k):keys(val)for k,val in v.items()}
   if isinstance(v,list):return[keys(x)for x in v]
   return v
  row['json_keys_only']=keys(json.loads(a))==json.loads(b)
  assert json.loads(norm(a.decode()))==json.loads(b),name
  row['json_spelling_only']=True
 if not pure:extra.extend(difflib.unified_diff(b.decode().splitlines(True),norm(a.decode()).splitlines(True),fromfile=name+':before',tofile=name+':normalized-after'))
 rows.append(row)
(P/'normalized-extra.diff').write_text(''.join(extra));(P/'source-hashes.json').write_text(json.dumps({'base':git('rev-parse',BASE).decode().strip(),'files':rows},indent=2)+'\n');(P/'production.patch').write_bytes(git('diff',BASE))
for side in['baseline-instrument','candidate-instrument']:
 for name in git('ls-files','tools/evaluator-bench').decode().splitlines():
  out=P/side/Path(name).relative_to('tools/evaluator-bench');out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(git('show',BASE+':'+name)if side.startswith('baseline')else(SDK/name).read_bytes());out.chmod((SDK/name).stat().st_mode&0o777)
sys.path.insert(0,str(SDK/'tools/evaluator-bench'));import compare
names=compare.sdk_crates(SDK);audit=compare.instrument_diff(P/'baseline-instrument',P/'candidate-instrument',names)
assert audit['confined'] and audit['changed']==['src/sdk.rs'] and not audit['forbidden'],audit
(P/'instrument-audit.json').write_text(json.dumps(audit,indent=2)+'\n')
(P/'current-untracked-instrument.txt').write_bytes(git('ls-files','--others','--exclude-standard','tools/evaluator-bench'))
(P/'remaining-old-inventory.txt').write_bytes(subprocess.run(['rg','-n','evaluation_steps|program_bytes|evaluationSteps|programBytes','crates','packages','tools','docs','README.md'],cwd=SDK,stdout=subprocess.PIPE).stdout)
print(json.dumps({'files':len(rows),'field_only_exact':sum(r['field_only_exact']for r in rows),'json_keys_only':sum(r.get('json_keys_only',False)for r in rows),'adapter_changed':audit['changed']},indent=2))
