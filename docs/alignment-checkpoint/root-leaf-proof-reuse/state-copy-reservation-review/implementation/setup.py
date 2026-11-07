from pathlib import Path
import hashlib,json,shutil,subprocess
P=Path(__file__).resolve().parent; ROOT=P.parents[4];SDK=ROOT/'sdk';OLD=P.parents[1]/'root-alias-leaf-memo-review/implementation';C=SDK/'crates/openbindings-json-schema-evaluator';D=P/'candidate-copy'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
shutil.copytree(OLD/'candidate-copy',D,dirs_exist_ok=True)
live=(C/'src/semantic/mod.rs').read_text();m=D/'src/semantic/mod.rs';s=m.read_text()
a='    fn copied(';b='    fn truth('
new=live[live.index(a):live.index(b)];old=s[s.index(a):s.index(b)]
(P/'old-copied.rs').write_text(old);(P/'new-copied.rs').write_text(new)
s=s[:s.index(a)]+new+s[s.index(b):]
s+='\n#[cfg(test)] #[path="independent-state-copy.rs"] mod independent_state_copy;\n';m.write_text(s)
shutil.copyfile(P/'independent-state-copy.rs',D/'src/semantic/independent-state-copy.rs')
p=D/'src/semantic/decision.rs';s=p.read_text();s=s.replace('pub(super) fn work(&mut self, units: usize) -> Result<(), NoVerdict> {','pub(super) fn work(&mut self, units: usize) -> Result<(), NoVerdict> {\n        super::independent_state_copy::event("work",units);');s=s.replace('pub(super) fn charge(&mut self, bytes: usize) -> Result<(), NoVerdict> {','pub(super) fn charge(&mut self, bytes: usize) -> Result<(), NoVerdict> {\n        super::independent_state_copy::event("charge",bytes);');p.write_text(s)
p=D/'src/semantic/explain.rs';p.write_text(p.read_text()+'\n#[cfg(test)] pub(super) fn review_seed()->Arc<WitnessSeed>{Arc::new(WitnessSeed{schema:7,keyword:Some("type"),instance:11})}\n')
shutil.copyfile(SDK/'tools/evaluator-bench/src/allocations.rs',D/'src/review_allocations.rs')
p=D/'src/lib.rs';s=p.read_text().replace('#![forbid(unsafe_code)]','// Review-only allocator instrumentation.');s+='\nmod review_allocations;\n#[global_allocator] static REVIEW_ALLOCATOR:review_allocations::Counter=review_allocations::Counter;\n';p.write_text(s)
shutil.copyfile(SDK/'tools/evaluator-bench/src/sdk.rs',D/'src/review_benchmark.rs')
s=p.read_text()+'\nmod review_benchmark;\n';p.write_text(s)
w=json.loads((P.parents[1]/'root-alias-leaf-memo-small-reservation/witness.json').read_text());request=w['rows']['baseline']['request'];assert {k:v for k,v in request.items() if k!='expected'}=={k:v for k,v in w['rows']['candidate']['request'].items() if k!='expected'}
(P/'64k-request.json').write_text(json.dumps(request,separators=(',',':'))+'\n');shutil.copyfile(P/'64k-request.json',D/'src/semantic/64k-request.json')
files=['semantic/mod.rs','semantic/execution/mod.rs','semantic/preparation.rs','semantic/readings.rs'];h={}
for f in files:
 q=P/'source'/f;q.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(C/'src'/f,q);h[f]=sha(q)
(P/'source-hashes.json').write_text(json.dumps({'baseline':'f62f47757ba6682bea316492056428a665a7bea4','files':h,'request':sha(P/'64k-request.json'),'benchmark_adapter':sha(D/'src/review_benchmark.rs')},indent=2)+'\n')
(P/'production.patch').write_bytes(subprocess.check_output(['git','diff'],cwd=SDK))
for label in ['current','minimum']:
 cmd=json.loads((OLD/f'{label}-candidate-command.json').read_text());cmd[cmd.index('--test')+1]=str(D/'src/lib.rs');cmd[cmd.index('-o')+1]=str(P/f'{label}-candidate');(P/f'{label}-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
(P/'probe-hashes.json').write_text(json.dumps({str(p.relative_to(D)):sha(p) for p in (D/'src').rglob('*') if p.is_file()},indent=2)+'\n')
