"""Prepare/freeze the instrument or execute exactly one phase. No retries."""
import argparse
import gzip
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import selectors
import time
import tomllib
import auth_resolution as a

TARGET=Path(os.environ.get('CARGO_TARGET_DIR',a.ROOT/'target'))
CANDIDATE=TARGET/'openapi-json-path-qualification/archives/consumer'
CLOSED=Path('/tmp/ob-auth-baseline/consumer')
BINARIES=TARGET/'openapi-auth-measurement-resolution/20261013'
SOURCES=[a.SOURCE,a.PLAN,Path(__file__),a.ROOT/'tools/auth_resolution.py',a.ROOT/'tools/resolve-openapi-auth-performance.py',a.ROOT/'tools/qualify-openapi-auth-performance.py',a.ROOT/'tools/test_auth_resolution.py']

def environment():
    def read(cmd):
        r=subprocess.run(cmd,capture_output=True,text=True)
        return {'exit':r.returncode,'stdout':r.stdout,'stderr':r.stderr}
    return {'time_ns':time.time_ns(),'load_average':os.getloadavg(),'platform':platform.platform(),
            'cpu':read(['sysctl','-n','machdep.cpu.brand_string']),
            'performance_levels':read(['sysctl','hw.nperflevels','hw.perflevel0.physicalcpu','hw.perflevel1.physicalcpu']),
            'thermal':read(['pmset','-g','therm'])}

def tracked_files(directory):
    return {str(p.relative_to(directory)):a.sha(p) for p in sorted(directory.rglob('*')) if p.is_file() and 'target' not in p.relative_to(directory).parts and '.git' not in p.relative_to(directory).parts}

def prepare():
    a.EXECUTION.mkdir(exist_ok=True)
    BINARIES.mkdir(parents=True,exist_ok=True)
    prior=a.EVIDENCE/'qualification-investigation/receipt.json'; receipt=json.loads(prior.read_text())
    for rel,h in receipt['testedInputs'].items():a.require(a.sha(a.ROOT/rel)==h,'changed prior functional input '+rel)
    for rel,h in receipt['consumerFiles'].items():
        if rel not in ['Cargo.toml','wasm-probe.ts']:a.require(a.sha(CANDIDATE/rel)==h,'changed qualified consumer '+rel)
    manifest=(CANDIDATE/'Cargo.toml').read_text()
    a.require(manifest.split('\n[patch.crates-io]')[0]==(a.ROOT/'examples/openapi-json-path-consumer/Cargo.toml').read_text(),'consumer manifest base')
    patches=tomllib.loads(manifest)['patch']['crates-io']
    expected={}
    for filename in receipt['archives']:
        directory=CANDIDATE.parent/'cargo'/filename.removesuffix('.crate')
        expected[tomllib.loads((directory/'Cargo.toml').read_text())['package']['name']]={'path':str(directory)}
    expected['dynamic-openapi-client']={'git':'https://github.com/openbindings/openapi-client','rev':'fe09522a9b99a6c6b4918e3de0467b5a638afa5e'}
    a.require(patches==expected,'consumer archive patches')
    a.require(a.sha(CANDIDATE/'Cargo.lock')==receipt['consumerLockSha256'],'consumer lock changed')
    archives=CANDIDATE.parent/'cargo'
    archive_inventory={}
    for name,h in receipt['archives'].items():
        path=archives/name
        if not path.exists():path=TARGET/'package'/name
        a.require(a.sha(path)==h,'archive changed '+name)
        archive_inventory[str(path)]=h
        with tarfile.open(path) as t:
            for member in t.getmembers():
                if member.isfile():
                    actual=archives/member.name
                    a.require(actual.is_file() and a.sha(actual)==__import__('hashlib').sha256(t.extractfile(member).read()).hexdigest(),'extracted package differs '+member.name)
                    archive_inventory[str(actual)]=a.sha(actual)
    baseline=json.loads((a.EVIDENCE/'closed-snapshot.json').read_text())
    for rel,h in baseline['files'].items():a.require(a.sha(CLOSED.parent/rel)==h,'closed source changed '+rel)
    a.require((CLOSED/'Cargo.toml').read_text()==baseline['consumerManifest'],'closed consumer manifest changed')
    a.require(tomllib.loads((CLOSED/'Cargo.lock').read_text())['package']==tomllib.loads((CANDIDATE/'Cargo.lock').read_text())['package'],'closed/candidate locked packages differ')
    original_lib=subprocess.check_output(['git','show',baseline['base']+':examples/openapi-json-path-consumer/src/lib.rs'],cwd=a.ROOT)
    a.require((CLOSED/'src/lib.rs').read_bytes()==original_lib,'closed consumer library changed')
    commands=[];artifacts={}
    instrument_before={str(p.relative_to(a.ROOT)):a.sha(p) for p in SOURCES}
    for name,consumer in [('candidate',CANDIDATE),('closed',CLOSED)]:
        shutil.copyfile(a.SOURCE,consumer/'examples/auth_resolution.rs')
        for step in ['test','build']:
            cmd=['cargo',step,'--locked','--release','--target-dir',str(TARGET),'--manifest-path',str(consumer/'Cargo.toml'),'--example','auth_resolution']
            if step=='build':cmd.append('--message-format=json')
            run=subprocess.run(cmd,capture_output=True,text=True)
            (a.EXECUTION/f'{name}-{step}.log').write_text(run.stdout+run.stderr)
            commands.append({'command':cmd,'exit':run.returncode})
            a.require(run.returncode==0,'harness '+name+' '+step+' failed')
            if step=='build':
                emitted=[json.loads(line) for line in run.stdout.splitlines()]
                outputs=[x['executable'] for x in emitted if x.get('reason')=='compiler-artifact' and x.get('target',{}).get('name')=='auth_resolution' and x.get('executable')]
                a.require(len(outputs)==1 and Path(outputs[0])==TARGET/'release/examples/auth_resolution','unexpected Cargo executable artifact')
                artifacts[name]=outputs[0]
        shutil.copyfile(artifacts[name],BINARIES/name);(BINARIES/name).chmod(0o755)
        # Readiness-only, no sample timer is entered. Final phase first launches
        # are separately retained and counted in each phase cap.
        for phase in (['R','Q'] if name=='candidate' else ['Q']):
            cmd=[str(BINARIES/name),'phase='+phase,'process=0','preflight-only','remaining-seconds=2700']+(['anonymous-only'] if name=='closed' else [])
            run=subprocess.run(cmd,capture_output=True,text=True)
            file=a.EXECUTION/f'{name}-{phase}-readiness.jsonl'
            file.write_text(run.stdout);file.with_suffix('.stderr').write_text(run.stderr)
            commands.append({'command':cmd,'exit':run.returncode})
            a.require(run.returncode==0,'readiness preflight failed')
            a.read_rows(file,phase,0,name=='closed',True)
    a.require(a.sha(BINARIES/'candidate')!=a.sha(BINARIES/'closed'),'candidate and closed binaries unexpectedly identical')
    a.require(instrument_before=={str(p.relative_to(a.ROOT)):a.sha(p) for p in SOURCES},'instrument changed during preparation')
    extension={'format':'harness-only-receipt-extension@1','prior_receipt':str(prior),'prior_receipt_sha256':a.sha(prior),
               'unchanged_tested_inputs':receipt['testedInputs'],'qualified_archives_and_extracted_files':archive_inventory,
               'closed_snapshot_sha256':a.sha(a.EVIDENCE/'closed-snapshot.json'),'closed_sources':baseline['files'],
               'consumer_inputs':{name:tracked_files(c) for name,c in [('candidate',CANDIDATE),('closed',CLOSED)]},
               'new_harness_sha256':a.sha(a.SOURCE),'instrument_files':{str(p.relative_to(a.ROOT)):a.sha(p) for p in SOURCES},'binary_sha256':{name:a.sha(BINARIES/name) for name in ['candidate','closed']},'rustc':subprocess.check_output(['rustc','-vV'],text=True),
               'cargo_executable_artifacts':artifacts,'consumer_exclusions':{'Cargo.toml':'Generated archive patches verified exactly against qualified archives; original manifest unchanged in testedInputs.','wasm-probe.ts':'Qualification runs this from repository fixture, not copied consumer; unchanged original hash verified in testedInputs. No Wasm rerun claimed.'},'readiness_sha256':{str(p.relative_to(a.ROOT)):a.sha(p) for p in a.EXECUTION.glob('*-readiness.*')},'commands':commands,'scope':'Prior functional qualification preserved, not rerun; new harness release tests/build and readiness preflights only.'}
    a.write(a.EXECUTION/'receipt-extension.json',extension)
    a.write(a.EXECUTION/'schedules.json',a.schedule_manifest())
    # Exact fixtures are checked from executable output, not reconstructed from
    # draft binding expectations. Input and finite response come from frozen row table.
    fixtures={}
    for file in a.EXECUTION.glob('*-readiness.jsonl'):
        for line in file.read_text().splitlines():
            r=json.loads(line)
            for h in r['cohorts']:
                for obj in h['objects']:
                    key='|'.join([r['version'],r['workload'],str(obj['authenticated'])])
                    body='"'+'x'*(r['bytes']-2)+'"';method='post' if 'post' in r['workload'] else 'get'
                    record={'source':obj['source'],'context':obj['context'],'input':r['fixture_input'],'response':r['fixture_response']}
                    hashes={k:__import__('hashlib').sha256(v.encode()).hexdigest() for k,v in record.items()}
                    if key in fixtures:a.require(fixtures[key]==hashes,'inconsistent exact fixture')
                    fixtures[key]=hashes
    expected_keys={'|'.join([v,w,str(auth)]) for v,w,method,size,tok,mech,layer in a.specs('Q') for auth in ([False] if mech=='none' else [False,True])}
    a.require(set(fixtures)==expected_keys,'fixture table coverage')
    a.write(a.EXECUTION/'fixtures.json',fixtures)
    print('Prepared; review before freeze. No timed samples run.',flush=True)

def freeze():
    a.require(not (a.EXECUTION/'R').exists() and not (a.EXECUTION/'Q').exists(),'phase already attempted')
    for name in ['astra-instrument.md','claude-instrument.md']:
        a.require((a.EXECUTION/name).is_file(),'missing independent instrument review '+name)
    verify_extension()
    accepted=json.loads((a.EXECUTION/'instrument-acceptance.json').read_text())
    a.require(accepted['accepted'] is True and accepted['receipt_sha256']==a.sha(a.EXECUTION/'receipt-extension.json'),'instrument not accepted')
    for who in ['astra','claude']:
        report=a.EXECUTION/(who+'-instrument.md')
        a.require(accepted[who+'_sha256']==a.sha(report),'stale instrument review acceptance')
        a.require('FINAL INSTRUMENT DISPOSITION: ACCEPTED' in report.read_text(),'reviewer has not explicitly accepted '+who)
    files=SOURCES+[a.EXECUTION/x for x in ['receipt-extension.json','schedules.json','fixtures.json','astra-instrument.md','claude-instrument.md','instrument-acceptance.json']]
    m={'format':a.SAMPLER,'files':{str(p.relative_to(a.ROOT)):a.sha(p) for p in files},
       'binaries':{name:{'path':str(BINARIES/name),'sha256':a.sha(BINARIES/name)} for name in ['candidate','closed']},
       'compiler':subprocess.check_output(['rustc','-vV'],text=True),'created_ns':time.time_ns(),'phase_seconds_cap':2700}
    a.write(a.EXECUTION/'freeze.json',m)
    print('Freeze written; commit before timing.')

def verify_extension():
    e=json.loads((a.EXECUTION/'receipt-extension.json').read_text())
    a.require(a.sha(e['prior_receipt'])==e['prior_receipt_sha256'],'prior receipt changed')
    a.require(a.sha(a.SOURCE)==e['new_harness_sha256'],'harness changed after extension')
    a.require(all(c['exit']==0 for c in e['commands']),'unsuccessful receipt command')
    a.require(subprocess.check_output(['rustc','-vV'],text=True)==e['rustc'],'receipt compiler mismatch')
    for rel,h in e['readiness_sha256'].items():a.require(a.sha(a.ROOT/rel)==h,'readiness record changed')
    for rel,h in e['instrument_files'].items():a.require(a.sha(a.ROOT/rel)==h,'instrument changed after extension '+rel)
    for name,h in e['binary_sha256'].items():a.require(a.sha(BINARIES/name)==h,'binary differs from build receipt')
    for rel,h in e['unchanged_tested_inputs'].items():a.require(a.sha(a.ROOT/rel)==h,'functional input changed '+rel)
    for path,h in e['qualified_archives_and_extracted_files'].items():a.require(a.sha(path)==h,'package changed '+path)
    for rel,h in e['closed_sources'].items():a.require(a.sha(CLOSED.parent/rel)==h,'closed source changed '+rel)
    for name,consumer in [('candidate',CANDIDATE),('closed',CLOSED)]:a.require(tracked_files(consumer)==e['consumer_inputs'][name],'consumer changed '+name)

def verify_freeze():
    m=json.loads((a.EXECUTION/'freeze.json').read_text())
    a.require(m['format']==a.SAMPLER and m['phase_seconds_cap']==2700,'wrong frozen protocol')
    for rel,h in m['files'].items():a.require(a.sha(a.ROOT/rel)==h,'frozen input changed '+rel)
    for b in m['binaries'].values():a.require(a.sha(b['path'])==b['sha256'],'binary changed')
    a.require(subprocess.check_output(['rustc','-vV'],text=True)==m['compiler'],'compiler changed')
    verify_extension()
    # Source freeze must be a real committed checkpoint before the first timing.
    tracked=list(m['files'])+[str((a.EXECUTION/'freeze.json').relative_to(a.ROOT))]
    for rel in tracked:
        r=subprocess.run(['git','show','HEAD:'+rel],cwd=a.ROOT,capture_output=True)
        a.require(r.returncode==0 and r.stdout==(a.ROOT/rel).read_bytes(),'uncommitted frozen file '+rel)
    return m

def phase_run(phase):
    m=verify_freeze();r_summary=None
    if phase=='Q':
        rp=a.EXECUTION/'R';r_summary=json.loads((rp/'summary.json').read_text())
        a.require(r_summary['status']=='method-adequate-for-one-qualification','R inadequate: Q refused')
        provenance=json.loads((rp/'provenance.json').read_text())
        a.require(provenance['freeze_sha256']==a.sha(a.EXECUTION/'freeze.json'),'R freeze mismatch')
        rerows=[x for p in range(3) for x in a.read_rows(rp/f'{p}-candidate.jsonl.gz','R',p)]
        a.require(a.summarize_r(rerows)==r_summary,'R summary does not reproduce')
        accepted=json.loads((rp/'review-acceptance.json').read_text())
        a.require(accepted['summary_sha256']==a.sha(rp/'summary.json') and accepted['accepted'] is True,'R not accepted')
        a.require(accepted['provenance_sha256']==a.sha(rp/'provenance.json'),'R provenance acceptance mismatch')
        a.require(accepted['raw_sha256']=={str(p):a.sha(rp/f'{p}-candidate.jsonl.gz') for p in range(3)},'R raw acceptance mismatch')
        for who in ['astra','claude']:
            report=rp/(who+'-results.md')
            a.require(accepted[who+'_sha256']==a.sha(report),'missing/stale result review')
            a.require('FINAL R DISPOSITION: ADEQUATE' in report.read_text(),'reviewer rejected R')
    out=a.EXECUTION/phase;out.mkdir(exist_ok=False) # permanent single-attempt marker
    for name in (['candidate'] if phase=='R' else ['candidate','closed']):
        # Phase-specific executable copy: retain hash, first launch, and path.
        dest=BINARIES/(phase+'-'+name);a.require(not dest.exists(),'phase binary already exists')
        shutil.copyfile(m['binaries'][name]['path'],dest);dest.chmod(0o755)
    start=time.monotonic();deadline=start+2700
    meta={'phase':phase,'freeze_sha256':a.sha(a.EXECUTION/'freeze.json'),'start_ns':time.time_ns(),'processes':[],'status':'running','seconds_cap':2700}
    a.write(out/'provenance.json',meta);allrows=[]
    try:
        for name in (['candidate'] if phase=='R' else ['candidate','closed']):
            invoke(out,phase,0,name,deadline,meta,True)
        a.require(deadline-time.monotonic()>10,'no time for fixed settle');time.sleep(10)
        for p in range(3):
            for name in (['candidate'] if phase=='R' else ['closed','candidate'] if p%2==0 else ['candidate','closed']):
                print(f'{phase} process {p+1}/3 {name}',flush=True)
                file=invoke(out,phase,p,name,deadline,meta,False)
                got=a.read_rows(file,phase,p,name=='closed')
                if phase=='Q':got=[dict(x,binary=name) for x in got]
                allrows.extend(got)
        a.require(time.monotonic()<=deadline,'phase wall-time cap')
        summary=a.summarize_r(allrows) if phase=='R' else a.summarize_q(allrows,r_summary)
        a.write(out/'summary.json',summary);meta['status']=summary['status']
    except (Exception,KeyboardInterrupt) as e:
        meta['status']='inconclusive-stop';meta['error']=repr(e)
        a.write(out/'summary.json',{'status':'inconclusive-stop','error':repr(e),'product_qualified_complete':False,'partial_rows':allrows})
    finally:
        meta.update(end_ns=time.time_ns(),elapsed_seconds=time.monotonic()-start)
        a.write(out/'provenance.json',meta)
    print(json.dumps({k:meta[k] for k in ['status','elapsed_seconds']}),flush=True)
    return 0 if meta['status'] in ['method-adequate-for-one-qualification','performance-qualified-pending-independent-review'] else 2

def stop_and_drain(proc,stdout):
    proc.terminate()
    try:tail,_=proc.communicate(timeout=2)
    except subprocess.TimeoutExpired:
        proc.kill();tail,_=proc.communicate()
    if tail:stdout.write(tail);stdout.flush();os.fsync(stdout.fileno())

def stream_checkpoints(proc,stdout,deadline,validate):
    pending=b'';row_index=0
    with selectors.DefaultSelector() as sel:
        sel.register(proc.stdout,selectors.EVENT_READ)
        while True:
            remaining=deadline-time.monotonic()
            a.require(remaining>0,'phase wall-time cap')
            events=sel.select(remaining)
            a.require(events,'phase wall-time cap')
            chunk=os.read(proc.stdout.fileno(),65536)
            if not chunk:break
            stdout.write(chunk);stdout.flush();pending+=chunk
            while b'\n' in pending:
                line,pending=pending.split(b'\n',1)
                os.fsync(stdout.fileno())
                validate(line,row_index)
                row_index+=1
                proc.stdin.write(b'next\n');proc.stdin.flush()
    a.require(not pending,'truncated process row')
    return row_index

def invoke(out,phase,p,name,deadline,meta,preflight):
    remaining=deadline-time.monotonic();a.require(remaining>0,'phase wall-time cap')
    stem=('first-launch-' if preflight else str(p)+'-')+name
    raw=out/(stem+'.jsonl');error=out/(stem+'.stderr')
    binary=BINARIES/(phase+'-'+name)
    record={'process':p,'name':name,'preflight':preflight,'binary':str(binary),'binary_sha256':a.sha(binary),'before':environment()}
    remaining=deadline-time.monotonic();a.require(remaining>0,'phase cap after metadata')
    cmd=[str(binary),'phase='+phase,f'process={p}',f'remaining-seconds={max(0.001,remaining-1)}','checkpoint-ack']+(['anonymous-only'] if name=='closed' else [])+(['preflight-only'] if preflight else [])
    record['command']=cmd
    failure=None
    with raw.open('wb') as stdout,error.open('wb') as stderr:
        proc=subprocess.Popen(cmd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr)
        pending=b'';row_index=0
        try:
            def validate(line,index):
                a.validate_row(json.loads(line),phase,p,index,name=='closed',preflight,json.loads((a.EXECUTION/'fixtures.json').read_text()))
                record.setdefault('checkpoints',[]).append({'row':index,'sha256':__import__('hashlib').sha256(line).hexdigest()})
                # Record the accepted checkpoint before the next row can start.
                a.write(out/(stem+'-checkpoints.json'),record['checkpoints'])
            row_index=stream_checkpoints(proc,stdout,deadline,validate)
            a.require(row_index==len(a.specs(phase)),'incomplete process rows')
            proc.wait(timeout=max(.001,deadline-time.monotonic()))
        except (Exception,KeyboardInterrupt) as e:
            failure=repr(e)
            stop_and_drain(proc,stdout)
            record['stop']=failure
        finally:
            if not proc.stdin.closed:proc.stdin.close()
            if not proc.stdout.closed:proc.stdout.close()
        record['exit']=proc.returncode
    record['after']=environment();meta['processes'].append(record);a.write(out/'provenance.json',meta)
    compressed=raw.with_suffix('.jsonl.gz')
    with raw.open('rb') as src,gzip.open(compressed,'wb') as dst:shutil.copyfileobj(src,dst)
    # Keep original row checkpoints as well as compressed archival copy until review.
    a.require(failure is None and proc.returncode==0,'process incomplete: '+stem+' '+str(failure))
    a.read_rows(compressed,phase,p,name=='closed',preflight)
    return compressed

def main(phase):
    parser=argparse.ArgumentParser();parser.add_argument('--prepare',action='store_true');parser.add_argument('--freeze',action='store_true')
    args=parser.parse_args()
    if args.prepare:prepare();return 0
    if args.freeze:freeze();return 0
    return phase_run(phase)
