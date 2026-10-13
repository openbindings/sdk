"""Run the pre-registered CPU pairs once, retaining every observation. Requires an
extracted consumer from verify-openapi-json-path.py --archives. No network timing.
"""
from pathlib import Path
import argparse,subprocess,json,statistics,os,hashlib,time
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=Path,required=True);p.add_argument('--edition',choices=['3.1.2','3.0.4'],default='3.1.2');a=p.parse_args()
a.output.mkdir(parents=True,exist_ok=False)
target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target'))
consumer=target/'openapi-json-path-qualification/archives/consumer'
receipt_path=consumer.parent/'receipt.json'
if not receipt_path.is_file():raise SystemExit('A successful extracted-package qualification receipt is required')
receipt=json.loads(receipt_path.read_text())
if hashlib.sha256((consumer/'Cargo.lock').read_bytes()).hexdigest()!=receipt['consumerLockSha256']:raise SystemExit('Consumer lockfile changed after qualification')
for relative,expected in receipt['testedInputs'].items():
    if hashlib.sha256((ROOT/relative).read_bytes()).hexdigest()!=expected:raise SystemExit('Qualification inputs changed: '+relative)
if hashlib.sha256((consumer/'examples/measure.rs').read_bytes()).hexdigest()!=hashlib.sha256((ROOT/'examples/openapi-json-path-consumer/examples/measure.rs').read_bytes()).hexdigest():raise SystemExit('Copied measurement source is stale')
command=['cargo','build','--release','--locked','--manifest-path',str(consumer/'Cargo.toml'),'--example','measure']
build=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(a.output/'build.log').write_text(build.stdout+build.stderr)
if build.returncode:raise SystemExit(build.returncode)
binary=target/'release/examples/measure'
raw=[]
for pair in range(3):
    for mode in (['baseline','candidate'] if pair%2==0 else ['candidate','baseline']):
        print(f'pair {pair+1} {mode}',flush=True)
        result=subprocess.run([str(binary),mode,a.edition],text=True,capture_output=True)
        (a.output/f'{pair}-{mode}.stdout').write_text(result.stdout)
        (a.output/f'{pair}-{mode}.stderr').write_text(result.stderr)
        if result.returncode:raise SystemExit(f'Failed pair {pair} {mode}: {result.returncode}')
        raw.append({'pair':pair,**json.loads(result.stdout)})
(a.output/'raw.json').write_text(json.dumps(raw,indent=2)+'\n')
rows=[]
for i,example in enumerate(raw[0]['rows']):
    b=[statistics.median(r['rows'][i]['samples_ns']) for r in raw if r['mode']=='baseline']
    c=[statistics.median(r['rows'][i]['samples_ns']) for r in raw if r['mode']=='candidate']
    bm=statistics.median(b);cm=statistics.median(c)
    if example['workload']=='stage':allowance=0
    elif example['workload']=='synthesis':allowance=100000+2000*example['n']
    elif example['workload']=='cold-setup-and-invocation':allowance=100000+10*example['source_bytes']
    else:allowance=25000+10*example['payload']
    spread=(max(b)-min(b))/bm
    row={k:v for k,v in example.items() if k!='samples_ns'}
    row.update(baseline_process_medians_ns=b,candidate_process_medians_ns=c,baseline_median_ns=bm,candidate_median_ns=cm,overhead_ns=cm-bm,ratio=cm/bm,baseline_spread=spread,ceiling_ns=2*bm+allowance,paired_overheads_ns=[cv-bv for bv,cv in zip(b,c)],median_paired_overhead_ns=statistics.median([cv-bv for bv,cv in zip(b,c)]),paired_ratios=[cv/bv for bv,cv in zip(b,c)],status='diagnostic' if example['workload']=='stage' else 'inconclusive' if spread>0.15 else 'pass' if all(cv<=2*bv+allowance for bv,cv in zip(b,c)) else 'fail')
    rows.append(row)
report={'edition':a.edition,'qualification_receipt_sha256':hashlib.sha256(receipt_path.read_bytes()).hexdigest(),'consumer_lock_sha256':receipt['consumerLockSha256'],'format':'openbindings.openapi-json-path.performance@1','time':time.time(),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256((consumer/'examples/measure.rs').read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'rows':rows,'notes':['Programmable complete delivery; no network latency or general HTTP-parity claim.','Samples include ownership destruction. Warm input/source owners are retained consistently.','validated-invocation uses the same core evaluator and input contract on both paths.','Cold row combines setup and one invocation; warm source parse is separately structurally instrumented.']}
(a.output/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'counts':{s:sum(r['status']==s for r in rows) for s in ['pass','fail','inconclusive']},'summary':str(a.output/'summary.json')}))
