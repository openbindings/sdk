"""Compare internal forks with exact downloaded upstream crate archives."""
from pathlib import Path
import argparse,difflib,hashlib,json,tarfile
p=argparse.ArgumentParser();p.add_argument('--upstream-dir',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
root=Path(__file__).resolve().parents[1];a.output.mkdir(parents=True,exist_ok=True);records=[]
for name,version in [('jsonschema','0.58.6'),('jsonschema-value','0.58.6'),('regress','0.12.0')]:
 archive=a.upstream_dir/f'{name}-{version}.crate';modified=root/'vendor'/('openbindings-internal-'+name)
 with tarfile.open(archive) as tar:original={m.name.split('/',1)[1]:tar.extractfile(m).read() for m in tar.getmembers() if m.isfile()}
 current={str(f.relative_to(modified)):f.read_bytes() for f in modified.rglob('*') if f.is_file() and 'target' not in f.parts and '.git' not in f.parts};changes=[];patch=[]
 for file in sorted(original.keys()|current.keys()):
  before,after=original.get(file),current.get(file)
  if before==after:continue
  changes.append({'file':file,'upstream_sha256':hashlib.sha256(before).hexdigest() if before is not None else None,'current_sha256':hashlib.sha256(after).hexdigest() if after is not None else None})
  try:patch.extend(difflib.unified_diff((before or b'').decode().splitlines(True),(after or b'').decode().splitlines(True),fromfile='a/'+file,tofile='b/'+file))
  except UnicodeError:patch.append('Binary difference: '+file+'\n')
 (a.output/(name+'.patch')).write_text(''.join(patch));records.append({'upstream':name,'version':version,'upstream_archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'vcs':json.loads(original.get('.cargo_vcs_info.json',b'{}')),'modified_package':'openbindings-internal-'+name,'changes':changes,'patch':name+'.patch'})
(a.output/'manifest.json').write_text(json.dumps({'records':records},indent=2)+'\n')
print(json.dumps([{'name':r['upstream'],'changed_files':len(r['changes'])} for r in records]))
