from pathlib import Path
import json,subprocess,concurrent.futures
P=Path(__file__).resolve().parent
filters=['independent_root','independent_hot_path_ownership','semantic::readings::tests','semantic::execution::tests','semantic::explain::tests','review_semantic_interruption','review_effect_limits','review_exact_operands','review_custom_profiles','review_preparation_budgets','recursive_support_keeps_foreign_leaf']
def go(x):
 l,v=x;statuses={}
 for f in filters if v=='candidate'else ['independent_root_layout','independent_hot_path_ownership']:
  with(P/f'{l}-{v}-{f.replace("::","-")}.log').open('w')as log:statuses[f]=subprocess.call([str(P/f'{l}-{v}'),f,'--test-threads=1','--nocapture'],cwd=P.parents[4]/'sdk',stdout=log,stderr=subprocess.STDOUT)
  print(l,v,f,statuses[f],flush=True)
 (P/f'{l}-{v}-statuses.json').write_text(json.dumps(statuses,indent=2)+'\n')
with concurrent.futures.ThreadPoolExecutor(max_workers=4)as pool:list(pool.map(go,[(l,v)for l in ['current','minimum']for v in ['candidate','baseline']]))
