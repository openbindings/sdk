#!/usr/bin/env python3
import hashlib,json,shutil,subprocess
from pathlib import Path
P=Path(__file__).resolve().parent;ROOT=P.parents[4]
deps=ROOT/'target/alignment/wasm32-unknown-unknown/release/deps'
compiler=Path('/Users/matt/Code/ob-pj/design/openbindings-rust-sdk-migration-2026-10-08/work/toolchains/1.99.0/bin/rustc')
# Resolve the exact direct dependency identities from the current Cargo fingerprint.
fp=ROOT/'target/alignment/wasm32-unknown-unknown/release/.fingerprint'
parent=fp/'openbindings-json-schema-evaluator-44c71430763ad993/lib-openbindings_json_schema_evaluator.json'
ident=json.loads(parent.read_text());libraries={}
for _,name,_,expected in ident['deps']:
 matches=[p for p in fp.glob('*/lib-'+name) if int.from_bytes(bytes.fromhex(p.read_text().strip()),'little')==expected]
 assert len(matches)==1,(name,matches)
 suffix=matches[0].parent.name.rsplit('-',1)[1]
 libraries[name]=deps/('lib'+name+'-'+suffix+'.rlib')
 assert libraries[name].is_file()
(P/'wasm-layout-parent-fingerprint.json').write_text(parent.read_text())
for variant in ['candidate','baseline']:
 dest=P/('layout-'+variant);shutil.copytree(P/(variant+'-copy'),dest,dirs_exist_ok=True)
 with (dest/'src/semantic/execution/mod.rs').open('a') as out:
  out.write('\n// Intentional target-layout diagnostics in a private review copy only.\nconst REVIEW_FRAME_LAYOUT_MISMATCH:[();0]=[();std::mem::size_of::<Frame<\'static>>()];\nconst REVIEW_STATE_LAYOUT_MISMATCH:[();0]=[();std::mem::size_of::<State>()];\n')
 with (dest/'src/semantic/mod.rs').open('a') as out:
  out.write('\nconst REVIEW_RUN_LAYOUT_MISMATCH:[();0]=[();std::mem::size_of::<Run>()];\nconst REVIEW_FIELDS_LAYOUT_MISMATCH:[();0]=[();std::mem::size_of::<readings::Fields>()];\n')
 cmd=[str(compiler),'--edition=2024','--crate-type','rlib','--emit=metadata','--target','wasm32-unknown-unknown',str(dest/'src/lib.rs'),'-L','dependency='+str(deps),'-L','dependency='+str(ROOT/'target/alignment/release/deps'),'-o',str(P/('layout-'+variant+'.rmeta'))]
 for name,path in libraries.items():cmd+=['--extern',name+'='+str(path)]
 (P/('wasm-layout-'+variant+'-command.json')).write_text(json.dumps(cmd,indent=2)+'\n')
 result=subprocess.run(cmd,cwd=ROOT/'sdk',stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (P/('wasm-layout-'+variant+'.log')).write_text(result.stdout)
 print(variant,result.returncode)
 print(result.stdout[-5000:])
(P/'wasm-layout-dependency-hashes.json').write_text(json.dumps({str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in libraries.values()},indent=2)+'\n')
