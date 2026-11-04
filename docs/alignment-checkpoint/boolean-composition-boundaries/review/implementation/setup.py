from pathlib import Path
import json,hashlib,shutil,subprocess
P=Path(__file__).resolve().parent; ROOT=P.parents[4]; SDK=ROOT/'sdk'; CRATE=SDK/'crates/openbindings-json-schema-evaluator'; OLD=P.parents[1]/'narrow-integer-review/implementation'
FILES=['semantic/decision.rs','semantic/counts.rs','semantic/mod.rs','semantic/execution/mod.rs']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
(P/'source-hashes.json').write_text(json.dumps({'commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=SDK,text=True).strip(),'files':{f:sha(CRATE/'src'/f)for f in FILES}},indent=2)+'\n')
(P/'production.patch').write_bytes(subprocess.check_output(['git','diff'],cwd=SDK))
for variant in ['candidate','baseline']:
 dest=P/(variant+'-copy');shutil.copytree(CRATE,dest,dirs_exist_ok=True)
 if variant=='baseline':
  for f in FILES:(dest/'src'/f).write_bytes(subprocess.check_output(['git','show','0142eaa:crates/openbindings-json-schema-evaluator/src/'+f],cwd=SDK))
 ownership=ROOT/'receipts/2026-11-02-preparation/original-default-performance/first-hot-path-increment-review/independent-ownership.rs'
 shutil.copyfile(ownership,dest/'src/semantic/independent-ownership.rs')
 m=dest/'src/semantic/mod.rs';s=m.read_text()+'\n#[cfg(test)] #[path="independent-ownership.rs"] mod independent_hot_path_ownership;\n'
 if variant=='candidate':
  shutil.copyfile(P/'independent-boolean.rs',dest/'src/semantic/independent-boolean.rs')
  s+='\n#[cfg(test)] #[path="independent-boolean.rs"] mod independent_boolean;\n'
  s=s.replace('for (name, &formula) in &source.properties {','for (name, &formula) in &source.properties {\n            #[cfg(test)] independent_boolean::loop_visit(1);')
  s=s.replace('for (&index, &formula) in &source.items {','for (&index, &formula) in &source.items {\n            #[cfg(test)] independent_boolean::loop_visit(2);')
  e=dest/'src/semantic/execution/mod.rs';t=e.read_text();needle='            *annotation = self.graph.and(result.valid, *annotation)?;'; assert t.count(needle)==1
  t=t.replace('            self.graph.tick()?;\n'+needle,'            #[cfg(test)] super::independent_boolean::loop_visit(0);\n            self.graph.tick()?;\n'+needle)
  t+='''
#[cfg(test)] pub(super) fn review_complete(run:&mut Run<'_>, value:JsonRef<'_>, state:State)->Result<State,NoVerdict>{
 let visit=Visit{schema:run.plan.entry,instance:0,context:run.empty_context.clone()};
 run.active.push(visit.clone(),(),1,&mut run.graph)?;
 run.complete_application(Frame{visit,value,fields:run.plan.fields(run.plan.entry),state,phase:Phase::Done})
}
''';e.write_text(t)
  c=dest/'src/semantic/counts.rs';t=c.read_text().replace('for (count, &formula) in self.uncertain.iter().enumerate() {\n            graph.tick()?;\n            let same', 'for (count, &formula) in self.uncertain.iter().enumerate() {\n            #[cfg(test)] super::independent_boolean::loop_visit(3);\n            graph.tick()?;\n            let same')
  t+='\n#[cfg(test)] impl Matches {pub(super) fn review_seed(v:Vec<Formula>)->Self{Self{concrete:0,uncertain:v}} pub(super) fn review_values(&self)->&[Formula]{&self.uncertain}}\n';c.write_text(t)
  d=dest/'src/semantic/decision.rs';d.write_text(d.read_text()+'''\n#[cfg(test)] impl Graph<'_>{pub(super) fn review_evaluate(&self,f:Formula,v:&[bool])->bool{self.evaluate(f,v)} pub(super) fn review_state(&self)->(usize,usize,usize,usize,usize){(self.nodes.len(),self.unique.len(),self.applied.len(),self.bytes,self.steps)} pub(super) fn review_apply(&mut self,and:bool,a:Formula,b:Formula)->Result<Formula,NoVerdict>{self.apply(if and{Op::And}else{Op::Or},a,b)}}\n''')
 m.write_text(s)
 lib=dest/'src/lib.rs';s=lib.read_text()
 for name in ['semantic_interruption','exact_operands','effect_limits','custom_profiles','preparation_budgets']:
  s+=f'\n#[cfg(test)] #[path="{dest}/tests/{name}.rs"] mod review_{name};\n'
 lib.write_text(s)
 for label in ['current','minimum']:
  cmd=json.loads((OLD/f'{label}-command.json').read_text());cmd[cmd.index('--test')+1]=str(dest/'src/lib.rs');cmd[cmd.index('-o')+1]=str(P/f'{label}-{variant}')
  (P/f'{label}-{variant}-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
(P/'copy-hashes.json').write_text(json.dumps({v:{str(p.relative_to(P/(v+'-copy'))):sha(p) for p in (P/(v+'-copy')/'src').rglob('*')if p.is_file()}for v in ['candidate','baseline']},indent=2)+'\n')
