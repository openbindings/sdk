#!/usr/bin/env python3
"""Preserve the original alternating twelve-job native regression comparison."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess

p=argparse.ArgumentParser(description=__doc__)
for name in ['baseline','candidate','fixtures','output']:p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--quiet-window',required=True)
a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
if (a.output/'comparison.json').exists():raise SystemExit('Refusing to overwrite comparison')
manifest=json.loads((a.fixtures/'manifest.json').read_text())['regression'];results=[]
for name,identity in manifest.items():
    raw=(a.fixtures/(name+'.json')).read_bytes()
    assert len(raw)==identity['bytes'] and hashlib.sha256(raw).hexdigest()==identity['sha256']
    for lane in ['contract','retained'] if name=='contract-wide' else ['assess','parse-assess']:
        observations={}
        for variant in ['baseline','candidate'] if len(results)%2==0 else ['candidate','baseline']:
            command=[str(getattr(a,variant).resolve()),variant,name,lane,str(a.fixtures.resolve()/(name+'.json'))]
            run=subprocess.run(command,capture_output=True,text=True,timeout=60)
            stem=a.output/f'{name}-{lane}-{variant}'
            stem.with_suffix('.stdout').write_text(run.stdout);stem.with_suffix('.stderr').write_text(run.stderr)
            assert run.returncode==0,(variant,name,lane)
            observations[variant]=json.loads(run.stdout)
        b,c=observations['baseline'],observations['candidate'];assert b['observation']==c['observation']
        bm=statistics.median(b['samples_ns']);cm=statistics.median(c['samples_ns'])
        allowance=10000 if name in ['healthy','contract-wide'] else 1000000
        results.append({'fixture':name,'lane':lane,'baselineMedianNs':bm,'candidateMedianNs':cm,'ratio':cm/bm,'passed':cm<=bm*1.15 or cm-bm<=allowance,'observations':observations})
(a.output/'comparison.json').write_text(json.dumps({'quietWindow':a.quiet_window,'binaries':{v:hashlib.sha256(getattr(a,v).read_bytes()).hexdigest() for v in ['baseline','candidate']},'results':results},indent=2)+'\n')
assert all(row['passed'] for row in results),'Frozen regression threshold failed'
