from pathlib import Path
import hashlib,json,shutil,subprocess
P=Path(__file__).resolve().parent;ROOT=P.parents[4];SDK=ROOT/'sdk';CRATE=SDK/'crates/openbindings-json-schema-evaluator';OLD=P.parents[1]/'boolean-composition-boundaries-review/implementation'
FILES=['semantic/mod.rs','semantic/execution/mod.rs','semantic/preparation.rs','semantic/readings.rs']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
(P/'source-hashes.json').write_text(json.dumps({'base':'f62f47757ba6682bea316492056428a665a7bea4','files':{f:sha(CRATE/'src'/f)for f in FILES}},indent=2)+'\n');(P/'production.patch').write_bytes(subprocess.check_output(['git','diff'],cwd=SDK))
for f in FILES:
 dest=P/'source'/f;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(CRATE/'src'/f,dest)
for v in ['candidate','baseline']:
 dest=P/(v+'-copy');shutil.copytree(CRATE,dest,dirs_exist_ok=True)
 if v=='baseline':
  for f in FILES:(dest/'src'/f).write_bytes(subprocess.check_output(['git','show','f62f477:crates/openbindings-json-schema-evaluator/src/'+f],cwd=SDK))
 shutil.copyfile(ROOT/'receipts/2026-11-02-preparation/original-default-performance/first-hot-path-increment-review/independent-ownership.rs',dest/'src/semantic/independent-ownership.rs')
 m=dest/'src/semantic/mod.rs';s=m.read_text()+'\n#[cfg(test)] #[path="independent-ownership.rs"] mod independent_hot_path_ownership;\n'
 e=dest/'src/semantic/execution/mod.rs';t=e.read_text();t+='\n#[cfg(test)] pub(super) fn review_frame_size()->usize {std::mem::size_of::<Frame>()}\n'
 s+='\n#[cfg(test)] #[test] fn independent_root_layout(){println!("ROOT_LAYOUT {} {} {} {}",size_of::<Run>(),size_of::<State>(),size_of::<readings::Fields>(),execution::review_frame_size());}\n'
 if v=='candidate':
  shutil.copyfile(P/'independent-root.rs',dest/'src/semantic/independent-root.rs');s+='\n#[cfg(test)] #[path="independent-root.rs"] mod independent_root;\n'
  s=s.replace('        let (mut outcome, witness) = run.evaluate_root(value, unavailable);','        let (mut outcome, witness) = run.evaluate_root(value, unavailable);\n        #[cfg(test)] independent_root::snapshot(&run);')
  needle='    fn root_leaf('; assert t.count(needle)==1;a,b=t.split(needle);b=b.replace('        if let Some(known) = self.memo.get(&key) {','        if let Some(known) = self.memo.get(&key) {\n            #[cfg(test)] super::independent_root::event(1);',1).replace('        let Beginning::Ready(state) = self.begin','        #[cfg(test)] super::independent_root::event(0);\n        let Beginning::Ready(state) = self.begin',1);t=a+needle+b
  t=t.replace('        // Plan owns the original source','        #[cfg(test)] if fields.is_leaf(){super::independent_root::event(2);}\n        // Plan owns the original source')
  t+='\n#[cfg(test)] pub(super) fn review_root_leaf(run:&mut Run, node:usize,value:JsonRef,context:Context)->Result<State,NoVerdict>{run.root_leaf(node,run.plan.fields(node),value,context)}\n'
  p=dest/'src/semantic/preparation.rs';u=p.read_text().replace('    for query in queries.values() {','    for query in queries.values() {\n        #[cfg(test)] super::independent_root::mark(false);').replace('    resources.sort_unstable();\n    control.check()?;','    resources.sort_unstable();\n    #[cfg(test)] super::independent_root::mark(true);\n    control.check()?;');p.write_text(u)
 m.write_text(s);e.write_text(t)
 lib=dest/'src/lib.rs';s=lib.read_text()
 for name in ['semantic_interruption','exact_operands','effect_limits','custom_profiles','preparation_budgets']:
  s+=f'\n#[cfg(test)] #[path="{dest}/tests/{name}.rs"] mod review_{name};\n'
 lib.write_text(s)
 for label in ['current','minimum']:
  cmd=json.loads((OLD/f'{label}-{v}-command.json').read_text());cmd[cmd.index('--test')+1]=str(dest/'src/lib.rs');cmd[cmd.index('-o')+1]=str(P/f'{label}-{v}');(P/f'{label}-{v}-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
(P/'copy-hashes.json').write_text(json.dumps({v:{str(p.relative_to(P/(v+'-copy'))):sha(p)for p in(P/(v+'-copy')/'src').rglob('*')if p.is_file()}for v in ['candidate','baseline']},indent=2)+'\n')
