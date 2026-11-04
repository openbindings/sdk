from pathlib import Path
import subprocess,json,concurrent.futures
P=Path(__file__).resolve().parent
filters=['independent_boolean','independent_hot_path_ownership','semantic::decision::tests','semantic::counts::tests','semantic::explain::tests','semantic::execution::tests','review_semantic_interruption','review_effect_limits','review_exact_operands','review_custom_profiles','review_preparation_budgets','recursive_support_keeps_foreign_leaf']
def run(label,variant):
 results={}
 for f in filters if variant=='candidate' else ['independent_hot_path_ownership']:
  with(P/f'{label}-{variant}-{f.replace("::","-")}.log').open('w')as out:results[f]=subprocess.call([str(P/f'{label}-{variant}'),f,'--test-threads=1','--nocapture'],cwd=P.parents[4]/'sdk',stdout=out,stderr=subprocess.STDOUT)
  print(label,variant,f,results[f],flush=True)
 (P/f'{label}-{variant}-statuses.json').write_text(json.dumps(results,indent=2)+'\n')
with concurrent.futures.ThreadPoolExecutor(max_workers=4)as pool:list(pool.map(lambda x:run(*x),[(l,v)for l in ['current','minimum']for v in ['candidate','baseline']]))
