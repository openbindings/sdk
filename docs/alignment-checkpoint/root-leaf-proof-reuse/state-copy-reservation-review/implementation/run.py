from pathlib import Path
import subprocess,json,concurrent.futures
P=Path(__file__).resolve().parent
filters=['independent_state_copy','independent_root','independent_hot_path_ownership','semantic::explain::tests','review_semantic_interruption','review_preparation_budgets']
def run(label):
 results={}
 for name in filters:
  with (P/f'{label}-{name.replace("::","-")}.log').open('w')as f:results[name]=subprocess.call([str(P/f'{label}-candidate'),name,'--test-threads=1','--nocapture'],cwd=P.parents[4]/'sdk',stdout=f,stderr=subprocess.STDOUT)
  print(label,name,results[name],flush=True)
  if results[name]!=0: break
 (P/f'{label}-results.json').write_text(json.dumps(results,indent=2)+'\n')
with concurrent.futures.ThreadPoolExecutor(max_workers=2)as e:list(e.map(run,['current','minimum']))
