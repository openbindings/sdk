from pathlib import Path
import os,json,subprocess,concurrent.futures
P=Path(__file__).resolve().parent;SDK=P.parents[3]/'sdk'
filters=['independent_budget','review_benchmark::limits_tests','evaluation_allowance_serialization_uses_only_current_names','an_evaluator_reports_the_budgets_it_was_given','semantic::explain::tests','semantic::decision::tests','review_preparation_budgets','review_semantic_interruption']
def run(label):
 cmd=json.loads((P/f'{label}-command.json').read_text());env=os.environ.copy();env['CARGO_MANIFEST_DIR']=str(P/'candidate-copy');env['CARGO_PKG_VERSION']='0.1.0';env['CARGO_PKG_NAME']='openbindings-json-schema-evaluator';result={}
 with(P/f'{label}-build.log').open('w')as f:result['build']=subprocess.call(cmd,cwd=SDK,env=env,stdout=f,stderr=subprocess.STDOUT)
 print(label,'build',result['build'],flush=True)
 if result['build']==0:
  for name in filters:
   with(P/f'{label}-{name.replace("::","-")}.log').open('w')as f:result[name]=subprocess.call([str(P/f'{label}-candidate'),name,'--test-threads=1','--nocapture'],cwd=SDK,stdout=f,stderr=subprocess.STDOUT)
   print(label,name,result[name],flush=True)
   if result[name]:break
 (P/f'{label}-results.json').write_text(json.dumps(result,indent=2)+'\n')
with concurrent.futures.ThreadPoolExecutor(max_workers=2)as ex:list(ex.map(run,['current','minimum']))
