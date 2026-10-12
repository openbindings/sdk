"""Interleaved public-consumer timing with pre-registered first-slice gates.

Run verify-openapi-inspection.py --archives first. Output must be a new directory;
failed/inconclusive results are retained. This command never retries measurements.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess

ROOT=Path(__file__).resolve().parents[1]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--consumer',type=Path)
    args=parser.parse_args()
    target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()
    consumer=(args.consumer or target/'openapi-inspection-qualification/archives/consumer').resolve()
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=False)
    command=['cargo','build','--release','--locked','--example','measure']
    build=subprocess.run(command,cwd=consumer,text=True,capture_output=True)
    (output/'build.log').write_text(build.stdout+build.stderr)
    if build.returncode: raise SystemExit(f'Build failed: {output / "build.log"}')
    binary=target/'release/examples/measure'
    metadata={'platform':platform.platform(),'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'binarySha256':digest(binary),'consumerLockSha256':digest(consumer/'Cargo.lock'),'harnessSha256':digest(consumer/'examples/measure.rs'),'consumer':str(consumer),'commands':[]}
    for name,key in [('machdep.cpu.brand_string','cpu'),('hw.memsize','memoryBytes')]:
        read=subprocess.run(['sysctl','-n',name],capture_output=True,text=True)
        metadata[key]=read.stdout.strip() if read.returncode==0 else 'unavailable in this environment'
    runs=[]
    for pair in range(3):
        result={}
        for mode in (['baseline','adapter'] if pair%2==0 else ['adapter','baseline']):
            print(f'Measuring pair {pair+1}: {mode}',flush=True)
            command=[str(binary),mode]
            process=subprocess.run(command,cwd=consumer,text=True,capture_output=True)
            (output/f'{pair+1}-{mode}.json').write_text(process.stdout)
            (output/f'{pair+1}-{mode}.stderr').write_text(process.stderr)
            metadata['commands'].append({'argv':command,'exit':process.returncode})
            (output/'metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
            if process.returncode: raise SystemExit('Measurement process failed; retained raw evidence')
            result[mode]=json.loads(process.stdout)
        runs.append(result)
    results=[];stable=True;passed=True
    for index,count in enumerate([1,32,512]):
        pairs=[];base={stage:[] for stage in ['cold','warm']}
        for run in runs:
            pair={}
            for stage in ['cold','warm']:
                b=statistics.median(s[stage+'Ns'] for s in run['baseline']['workloads'][index]['samples'])
                a=statistics.median(s[stage+'Ns'] for s in run['adapter']['workloads'][index]['samples'])
                allowance=(50_000+500*count) if stage=='cold' else (25_000+1_000*count)
                ceiling=2*b+allowance
                pair[stage]={'baselineNs':b,'adapterNs':a,'ceilingNs':ceiling,'overheadNs':a-b,'marginNs':ceiling-a}
                base[stage].append(b)
            pairs.append(pair)
        spreads={stage:(max(values)-min(values))/statistics.median(values) for stage,values in base.items()}
        margins={stage:statistics.median(p[stage]['marginNs'] for p in pairs) for stage in ['cold','warm']}
        stable &= all(v<=.15 for v in spreads.values())
        passed &= all(v>=0 for v in margins.values())
        results.append({'operations':count,'pairs':pairs,'baselineProcessSpread':spreads,'medianMarginNs':margins})
    status='inconclusive' if not stable else ('passed' if passed else 'investigate')
    summary={'status':status,'budgets':'cold:2*B+50000+500*N; warm:2*B+25000+1000*N ns; baseline spread<=15%','workloads':results,'scope':'native release, exact same generated documents; adapter adds carrier admission, target translation, SDK result checks; logical owner counts do not measure heap or RSS','rawFiles':[p.name for p in output.glob('*-*.json')]}
    (output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(f'{status}: {output / "summary.json"}',flush=True)
    if status!='passed':raise SystemExit(1)

if __name__=='__main__':main()
