#!/usr/bin/env python3
"""Run one serial source/artifact-bound campaign; caller arranges quiet window."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tarfile
import time

HERE = Path(__file__).resolve().parent

def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def run(command, out, timeout=120):
    started = time.monotonic()
    result = subprocess.run([str(x) for x in command], capture_output=True, text=True, timeout=timeout)
    out.with_suffix('.stdout').write_text(result.stdout)
    out.with_suffix('.stderr').write_text(result.stderr)
    out.with_suffix('.exit').write_text(str(result.returncode) + '\n')
    if result.returncode: raise RuntimeError(f'{out.name} failed; see saved stdout/stderr')
    return result.stdout, (time.monotonic()-started)*1000

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ['package', 'archive', 'fixtures', 'output', 'native', 'regression', 'source']:
        parser.add_argument('--'+arg, required=True, type=Path)
    parser.add_argument('--mode', choices=['check','measure'], default='check')
    parser.add_argument('--quiet-window')
    args = parser.parse_args()
    if args.mode == 'measure' and not args.quiet_window: parser.error('measure requires an owner-confirmed --quiet-window ID')
    args.output.mkdir(parents=True, exist_ok=True)
    receipt = args.output/'campaign.json'
    if receipt.exists(): raise RuntimeError('Refusing to overwrite a prior campaign; use a fresh output directory')
    source = subprocess.check_output(['git','rev-parse','HEAD'],cwd=args.source,text=True).strip()
    assets = {str(p.relative_to(HERE)):sha(p) for p in HERE.rglob('*') if p.is_file() and not any(x in p.parts for x in ['target','__pycache__']) and 'out' not in p.parts}
    status = subprocess.check_output(['git','status','--short'],cwd=args.source,text=True)
    # Binding a tarball hash is useful only if the consumed files match it.
    with tarfile.open(args.archive, 'r:gz') as archive:
        for member in archive.getmembers():
            if not member.isfile(): continue
            relative = Path(member.name).relative_to('package')
            if '..' in relative.parts: raise ValueError('unsafe archive member')
            if (args.package/relative).read_bytes() != archive.extractfile(member).read():
                raise ValueError('unpacked package differs from archive: '+str(relative))
    record = {'source':source,'sourceStatus':status,'protocolSha256':sha(HERE/'protocol.json'),'harnessFiles':assets,'fixtureManifestSha256':sha(args.fixtures/'manifest.json'),
              'nativeSha256':sha(args.native),'regressionSha256':sha(args.regression),'archiveSha256':sha(args.archive),'archive':str(args.archive.resolve()),
              'mode':args.mode,'quietWindow':args.quiet_window,'host':platform.platform(),'commands':[], 'status':'started',
              'nodeVersion':subprocess.check_output(['node','--version'],text=True).strip(),
              'rustcVersion':subprocess.check_output([os.environ.get('RUSTC','rustc'),'--version','--verbose'],text=True).strip()}
    receipt.write_text(json.dumps(record,indent=2)+'\n')
    if args.mode == 'measure':
        snapshot = subprocess.check_output(['ps','-axo','pid,pcpu,comm'],text=True)
        (args.output/'processes-before.txt').write_text(snapshot)
        for line in snapshot.splitlines()[1:]:
            fields=line.split(maxsplit=2)
            if len(fields)==3 and float(fields[1])>2 and any(s in fields[2] for s in ['rustc','cargo','clang','wasm-opt']):
                raise RuntimeError('Active compiler invalidates quiet window: '+line)
    try:
        command=[args.native,args.fixtures,args.mode]
        record['commands'].append([str(x) for x in command])
        command_timed=['/usr/bin/time','-l',*command] if sys.platform=='darwin' else command
        raw,_=run(command_timed,args.output/'native-run')
        native=json.loads(raw);native['cold']=[]
        if args.mode=='measure':
            for n in range(7):
                raw,ms=run([args.native,args.fixtures,'cold'],args.output/f'native-cold-{n}')
                native['cold'].append({'processElapsedMs':ms,'observation':json.loads(raw)})
        (args.output/'native.json').write_text(json.dumps(native,indent=2)+'\n')
        regressions=[]
        manifest=json.loads((args.fixtures/'manifest.json').read_text())
        for name in manifest['regression']:
            for lane in ['contract','retained'] if name=='contract-wide' else ['assess','parse-assess']:
                command=[args.regression,'current',name,lane,args.fixtures/(name+'.json')]
                if args.mode=='check':command.append('--check')
                record['commands'].append([str(x) for x in command])
                raw,_=run(command,args.output/f'regression-{name}-{lane}',timeout=60)
                regressions.append(json.loads(raw))
        (args.output/'regression.json').write_text(json.dumps(regressions,indent=2)+'\n')
        for host in ['node','chromium','webkit','workerd']:
            runner='browser' if host in ['chromium','webkit'] else host
            command=['node',HERE/(runner+'.mjs'),args.package,args.fixtures,args.output/(host+'.json')]
            if runner=='browser':command.append(host)
            command.append(args.mode)
            record['commands'].append([str(x) for x in command])
            run(command,args.output/(host+'-run'),timeout=120)
            print(host+' '+args.mode+' complete',flush=True)
        record['status']='complete'
    except Exception as error:
        record['status']='failed';record['error']=str(error);raise
    finally:
        receipt.write_text(json.dumps(record,indent=2)+'\n')

if __name__=='__main__':main()
