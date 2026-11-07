from pathlib import Path
import json,subprocess,concurrent.futures,os
P=Path(__file__).resolve().parent
SDK=P.parents[4]/'sdk'
def run(label):
 c=json.loads((P/f'{label}-command.json').read_text());env=os.environ.copy();env['CARGO_MANIFEST_DIR']=str(P/'candidate-copy');env['CARGO_PKG_VERSION']='0.1.0';env['CARGO_PKG_NAME']='openbindings-json-schema-evaluator'
 with (P/f'{label}-build.log').open('w')as f:r=subprocess.call(c,cwd=SDK,env=env,stdout=f,stderr=subprocess.STDOUT)
 print(label,r,flush=True)
 return label,r
with concurrent.futures.ThreadPoolExecutor(max_workers=2)as e:r=dict(e.map(run,['current','minimum']))
(P/'build-results.json').write_text(json.dumps(r,indent=2)+'\n')
