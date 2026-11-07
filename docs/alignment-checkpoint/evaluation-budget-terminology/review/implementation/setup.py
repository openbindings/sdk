from pathlib import Path
import json,shutil,hashlib
P=Path(__file__).resolve().parent;ROOT=P.parents[3];SDK=ROOT/'sdk';D=P/'candidate-copy';shutil.copytree(SDK/'crates/openbindings-json-schema-evaluator',D,dirs_exist_ok=True)
shutil.copyfile(P/'independent-budget.rs',D/'src/independent-budget.rs');shutil.copyfile(SDK/'tools/evaluator-bench/src/sdk.rs',D/'src/review_benchmark.rs');p=D/'src/lib.rs';s=p.read_text()+'\n#[cfg(test)] #[path="independent-budget.rs"] mod independent_budget;\n#[cfg(test)] mod review_benchmark;\n'
for name in ['evaluation_budgets','preparation_budgets','semantic_interruption']:s+=f'\n#[cfg(test)] #[path="{D}/tests/{name}.rs"] mod review_{name};\n'
p.write_text(s)
old=ROOT/'receipts/2026-11-02-preparation/original-default-performance/state-copy-reservation-review/implementation'
for label in['current','minimum']:
 cmd=json.loads((old/f'{label}-command.json').read_text());cmd[cmd.index('--test')+1]=str(D/'src/lib.rs');cmd[cmd.index('-o')+1]=str(P/f'{label}-candidate');(P/f'{label}-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
(P/'probe-hashes.json').write_text(json.dumps({str(p.relative_to(D)):hashlib.sha256(p.read_bytes()).hexdigest()for p in(D/'src').rglob('*')if p.is_file()},indent=2)+'\n')
