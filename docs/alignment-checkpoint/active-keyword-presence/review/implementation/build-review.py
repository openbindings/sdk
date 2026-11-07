#!/usr/bin/env python3
import hashlib,json,shutil,subprocess,sys
from pathlib import Path
P=Path(__file__).resolve().parent
ROOT=P.parents[4];SDK=ROOT/'sdk';CRATE=SDK/'crates/openbindings-json-schema-evaluator'
FILES=['src/semantic/mod.rs','src/semantic/preparation.rs','src/semantic/execution/mod.rs','src/semantic/refinement.rs','src/semantic/readings.rs']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def setup():
 (P/'source-hashes.json').write_text(json.dumps({'base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=SDK,text=True).strip(),'files':{str((CRATE/f).relative_to(ROOT)):sha(CRATE/f) for f in FILES}},indent=2)+'\n')
 (P/'production.patch').write_bytes(subprocess.check_output(['git','diff','--',*[str((CRATE/f).relative_to(SDK)) for f in FILES]],cwd=SDK))
 old=ROOT/'receipts/2026-11-02-preparation/original-default-performance/first-hot-path-increment-review'
 for variant in ['candidate','original']:
  dest=P/(variant+'-copy');shutil.copytree(CRATE,dest,dirs_exist_ok=True)
  if variant=='original':
   for f in FILES[:-1]:(dest/f).write_bytes(subprocess.check_output(['git','show','432cf15:crates/openbindings-json-schema-evaluator/'+f],cwd=SDK))
  shutil.copyfile(old/'independent-ownership.rs',dest/'src/semantic/independent-ownership.rs')
  with (dest/'src/semantic/mod.rs').open('a') as out:out.write('\n#[cfg(test)]\n#[path="independent-ownership.rs"]\nmod independent_hot_path_ownership;\n')
  if variant=='candidate':
   shutil.copyfile(P/'independent-masks.rs',dest/'src/semantic/independent-masks.rs')
   m=dest/'src/semantic/mod.rs';s=m.read_text();needle='if self.fields.may_read(keyword) {';assert s.count(needle)==1;s=s.replace(needle,needle+'\n            #[cfg(test)] review_raw_read();')
   s+='\n#[cfg(test)] thread_local! { static REVIEW_RAW_READS:std::cell::Cell<usize>=const {std::cell::Cell::new(0)}; }\n#[cfg(test)] fn review_raw_read(){REVIEW_RAW_READS.with(|c|c.set(c.get()+1));}\n#[cfg(test)] fn review_raw_reads_reset(){REVIEW_RAW_READS.with(|c|c.set(0));}\n#[cfg(test)] fn review_raw_reads()->usize{REVIEW_RAW_READS.with(|c|c.get())}\n#[cfg(test)] #[path="independent-masks.rs"] mod independent_masks;\n';m.write_text(s)
  lib=dest/'src/lib.rs';s=lib.read_text().replace('#![forbid(unsafe_code)]','// Review-only counting allocator; production retains forbid(unsafe_code).')
  shutil.copyfile(SDK/'tools/evaluator-bench/src/allocations.rs',dest/'src/review_allocations.rs')
  s+='\n#[cfg(test)] mod review_allocations;\n#[cfg(test)] #[global_allocator] static REVIEW_COUNTER:review_allocations::Counter=review_allocations::Counter;\n'
  for test in ['evaluation_budgets','preparation_scaling','pattern_memory','preparation_budgets','custom_profiles','literal_storage','exact_operands']:
   s+='\n#[cfg(test)] #[path="'+str(dest/'tests'/f'{test}.rs')+'"] mod review_'+test+';\n'
  lib.write_text(s)
 (P/'copied-source-hashes.json').write_text(json.dumps({v:{str(p.relative_to(P/(v+'-copy'))):sha(p) for p in (P/(v+'-copy')/'src').rglob('*') if p.is_file()} for v in ['candidate','original']},indent=2)+'\n')
def build(label,variant):
 old=ROOT/'receipts/2026-11-02-preparation/borrowed-literal-values-review/implementation'
 cmd=json.loads((old/(label+'-private-command.json')).read_text());cmd[cmd.index('--test')+1]=str(P/(variant+'-copy')/'src/lib.rs');cmd[cmd.index('-o')+1]=str(P/(label+'-'+variant))
 (P/(label+'-'+variant+'-command.json')).write_text(json.dumps(cmd,indent=2)+'\n')
 with (P/(label+'-'+variant+'-build.log')).open('w') as out:code=subprocess.call(cmd,cwd=SDK,stdout=out,stderr=subprocess.STDOUT)
 return code
def run(label,variant):
 binary=str(P/(label+'-'+variant));filters=['independent_hot_path_ownership','frame_backing_growth_has_an_exact_requested_byte_gate']
 if variant=='candidate':filters+=['independent_masks','prepared_operand_','empty_custom_applicators','recursive_support_keeps_foreign_leaf','review_custom_profiles','review_literal_storage','review_exact_operands','uncapped_evaluation_decides_large_values','admitted_large_documents_and_resources_keep_required_capabilities','run_matchers_hold_about_their_text_of_the_pattern_memory','original_meta_matching_obeys_each_attempt_and_retries_without_poisoning']
 statuses={}
 for filt in filters:
  with (P/(label+'-'+variant+'-'+filt+'.log')).open('w') as out:code=subprocess.call([binary,filt,'--nocapture','--test-threads=1'],cwd=SDK,stdout=out,stderr=subprocess.STDOUT)
  statuses[filt]=code
 (P/(label+'-'+variant+'-statuses.json')).write_text(json.dumps(statuses,indent=2)+'\n');return int(any(statuses.values()))
if __name__=='__main__':
 if sys.argv[1]=='setup':setup()
 elif sys.argv[1]=='build':sys.exit(build(*sys.argv[2:]))
 else:sys.exit(run(*sys.argv[2:]))
