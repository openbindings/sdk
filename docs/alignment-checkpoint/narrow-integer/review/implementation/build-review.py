#!/usr/bin/env python3
import hashlib,json,shutil,subprocess,sys
from pathlib import Path
P=Path(__file__).resolve().parent;ROOT=P.parents[4];SDK=ROOT/'sdk';OLD=P.parents[1]/'mask-keyword-inline-review/implementation'
FILES=['src/semantic/value_predicates.rs','src/semantic/primitive.rs']
def setup():
 dest=P/'candidate-copy';shutil.copytree(OLD/'baseline-copy',dest,dirs_exist_ok=True)
 hashes={}
 for f in FILES:
  src=SDK/'crates/openbindings-json-schema-evaluator'/f;shutil.copyfile(src,dest/f);hashes[f]=hashlib.sha256(src.read_bytes()).hexdigest()
 assert (dest/'src/semantic/readings.rs').read_bytes()==(SDK/'crates/openbindings-json-schema-evaluator/src/semantic/readings.rs').read_bytes()
 shutil.copyfile(P/'independent-integer.rs',dest/'src/semantic/independent-integer.rs');shutil.copyfile(P.parent/'design/controls.json',dest/'src/semantic/integer-controls.json')
 m=dest/'src/semantic/mod.rs';m.write_text(m.read_text()+'\n#[cfg(test)] #[path="independent-integer.rs"] mod independent_integer;\n')
 (P/'source-hashes.json').write_text(json.dumps({'base':'5cf704eb6cbd8f0d36b916ba53c1def4ebb20d0a','files':hashes,'unchanged_readings':hashlib.sha256((dest/'src/semantic/readings.rs').read_bytes()).hexdigest()},indent=2)+'\n')
 (P/'production.patch').write_bytes(subprocess.check_output(['git','diff','--',*[str(Path('crates/openbindings-json-schema-evaluator')/f)for f in FILES]],cwd=SDK))
 (P/'copy-hashes.json').write_text(json.dumps({str(f.relative_to(dest)):hashlib.sha256(f.read_bytes()).hexdigest()for f in(dest/'src').rglob('*')if f.is_file()},indent=2)+'\n')
def build(label):
 cmd=json.loads((OLD/f'{label}-baseline-command.json').read_text());cmd[cmd.index('--test')+1]=str(P/'candidate-copy/src/lib.rs');cmd[cmd.index('-o')+1]=str(P/f'{label}-candidate')
 (P/f'{label}-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
 with(P/f'{label}-build.log').open('w')as out:return subprocess.call(cmd,cwd=SDK,stdout=out,stderr=subprocess.STDOUT)
def run(label):
 filters=['independent_integer','integer_predicate_preserves_numeric_admission_and_exact_answers','independent_hot_path_ownership','review_exact_operands','a_permissive_custom_meta_can_admit_an_empty_type_union','frame_backing_growth_has_an_exact_requested_byte_gate']
 results={}
 for f in filters:
  with(P/f'{label}-{f}.log').open('w')as out:results[f]=subprocess.call([str(P/f'{label}-candidate'),f,'--test-threads=1','--nocapture'],cwd=SDK,stdout=out,stderr=subprocess.STDOUT)
 (P/f'{label}-statuses.json').write_text(json.dumps(results,indent=2)+'\n');return int(any(results.values()))
if __name__=='__main__':
 if sys.argv[1]=='setup':setup()
 elif sys.argv[1]=='build':sys.exit(build(sys.argv[2]))
 else:sys.exit(run(sys.argv[2]))
