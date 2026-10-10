#!/usr/bin/env python3
"""Evaluate frozen absolute budgets and compare exact host/fixture campaigns."""
import json
from pathlib import Path
import statistics
import sys

PROTOCOL=json.loads((Path(__file__).parent/'protocol.json').read_text())
TIERS=['small','representative','near']

def evaluate(samples, baseline, allowance, absolute=None):
    if not samples or (baseline is not None and not baseline): raise ValueError('missing samples')
    median=statistics.median(samples)
    noise=median>=1 and max(samples)>3*min(samples)
    result={'medianMs':median,'minimumMs':min(samples),'maximumMs':max(samples),'descriptiveP95Ms':sorted(samples)[__import__('math').ceil(len(samples)*.95)-1],
            'absoluteAllowanceMs':absolute,'absolutePassed':None if absolute is None else median<=absolute}
    if baseline is not None:
        base=statistics.median(baseline)
        noise=noise or (base>=1 and max(baseline)>3*min(baseline))
        result.update({'baselineMedianMs':base,'ratio':None if not base else median/base,'addedMs':median-base,'regressionPassed':median<=base*1.2 or median-base<=allowance})
    result['status']='inconclusive-noise' if noise else 'failed' if result['absolutePassed'] is False or result.get('regressionPassed') is False else 'within-frozen-budget'
    return result

def compare(directory, baseline=None):
    receipt=json.loads((directory/'campaign.json').read_text())
    if receipt['mode']!='measure' or receipt['status']!='complete': raise ValueError('only complete measurement campaigns qualify')
    if baseline:
        old=json.loads((baseline/'campaign.json').read_text())
        for field in ['protocolSha256','fixtureManifestSha256','host']:
            if receipt[field]!=old[field]:raise ValueError('campaign identity mismatch: '+field)
    rows=[]
    for host in ['native','node','chromium','webkit','workerd']:
        data=json.loads((directory/(host+'.json')).read_text())
        prior=json.loads((baseline/(host+'.json')).read_text()) if baseline else None
        if prior and host in ['chromium','webkit','workerd'] and data['version']!=prior['version']:raise ValueError('runtime version mismatch '+host)
        budgetHost='browser-worker' if host in ['chromium','webkit'] else host
        budgets=PROTOCOL['absoluteMedianMs'][budgetHost]
        for tierIndex,tier in enumerate(TIERS):
            row=data['tiers'][tier]
            oldRow=prior['tiers'][tier] if prior else None
            if oldRow:
                for fact in ['outcome','complete']:
                    if row['observation'][fact]!=oldRow['observation'][fact]:raise ValueError(f'{host}/{tier} verdict/completeness changed; adjudicate before comparison')
            for metric in ['complete','hot','invalidAndSerialize']:
                if metric not in budgets:continue
                samples=row['samplesMs'][metric];oldSamples=oldRow['samplesMs'][metric] if oldRow else None
                allowance=50 if tier=='near' else .1 if metric=='hot' else 1
                result=evaluate(samples,oldSamples,allowance,budgets[metric][tierIndex])
                rows.append({'host':host,'tier':tier,'metric':metric,**result})
            if host in ['chromium','webkit']:
                rows.append({'host':host,'tier':tier,'metric':'editorRoundtrip',**evaluate(data['editorRoundtrip'][tier],prior['editorRoundtrip'][tier] if prior else None,50 if tier=='near' else 1,budgets['editorRoundtrip'][tierIndex])})
        if 'discovery' in budgets:
            for i,tier in enumerate(TIERS):
                rows.append({'host':host,'tier':tier,'metric':'discovery',**evaluate(data['discovery'][tier]['samplesMs'],prior['discovery'][tier]['samplesMs'] if prior else None,50 if tier=='near' else 1,budgets['discovery'][i])})
        amp=data['amplification'];oldAmp=prior['amplification'] if prior else None
        samples=amp['samplesMs'] if isinstance(amp['samplesMs'],list) else amp['samplesMs']['invalidAndSerialize']
        oldSamples=(oldAmp['samplesMs'] if isinstance(oldAmp['samplesMs'],list) else oldAmp['samplesMs']['invalidAndSerialize']) if oldAmp else None
        outputChanged=oldAmp is not None and amp['observation']!=oldAmp['observation']
        rows.append({'host':host,'tier':'amplification','metric':'invalidAndSerialize','observation':amp['observation'],'outputChanged':outputChanged,
                     'comparisonMeaning':'cost of changed diagnostic output, not equivalent-output speedup' if outputChanged else 'same observed output volume',**evaluate(samples,oldSamples,1,250)})
        if host in ['chromium','webkit']:
            gaps=data['heartbeat']['gapsMs'];p95=sorted(gaps)[__import__('math').ceil(len(gaps)*.95)-1]
            rows.append({'host':host,'metric':'heartbeat','descriptiveP95GapMs':p95,'maximumGapMs':max(gaps),'status':'within-frozen-budget' if p95<=50 else 'failed','allowanceMs':50})
        if host!='native':
            samples=[row['processStartToFirstRequestMs'] if host=='workerd' else row['total'] for row in data['cold']]
            oldSamples=[row['processStartToFirstRequestMs'] if host=='workerd' else row['total'] for row in prior['cold']] if prior else None
            rows.append({'host':host,'metric':'coldSetup',**evaluate(samples,oldSamples,1,budgets['coldFirstRequest' if host=='workerd' else 'coldSetup'])})
    regression=json.loads((directory/'regression.json').read_text())
    if len(regression)!=12:raise ValueError('missing historical jobs')
    if baseline:
        old=json.loads((baseline/'regression.json').read_text())
        for row,previous in zip(regression,old):
            if (row['fixture'],row['lane'],row['observation'])!=(previous['fixture'],previous['lane'],previous['observation']):raise ValueError('historical job/observation mismatch')
            cm=statistics.median(row['samples_ns']);bm=statistics.median(previous['samples_ns'])
            allowance=10000 if row['fixture'] in ['healthy','contract-wide'] else 1000000
            rows.append({'host':'native','metric':'historical-'+row['fixture']+'-'+row['lane'],'candidateMedianNs':cm,'baselineMedianNs':bm,'ratio':cm/bm,'status':'within-frozen-budget' if cm<=bm*1.15 or cm-bm<=allowance else 'failed'})
        for encoding in ['rawBytes','gzip9Bytes']:
            current=json.loads((directory/'node.json').read_text())['package']['assets']['dist/wasm/openbindings_wasm_bg.wasm'][encoding]
            old=json.loads((baseline/'node.json').read_text())['package']['assets']['dist/wasm/openbindings_wasm_bg.wasm'][encoding]
            rows.append({'host':'package','metric':encoding,'ratio':current/old,'status':'within-frozen-budget' if current<=old*1.15 else 'failed'})
    return {'source':receipt['source'],'baseline':str(baseline) if baseline else None,'results':rows,'overall':'qualified frozen measured rows' if all(r['status']=='within-frozen-budget' for r in rows) else 'incomplete; inspect failed/inconclusive rows',
            'notQualified':['universal performance grade','Linux/Windows performance','native C4 HTTP timings','WAN transport','mobile/second device','deployed Cloudflare','total-memory/no-leak proof','arbitrary custom evaluators']}

if __name__=='__main__':
    if sys.argv[1:] == ['--self-test']:
        assert evaluate([100]*7,[10]*7,1,250)['status']=='failed'
        assert evaluate([10]*7,None,1,5)['status']=='failed'
        assert evaluate([1,1,1,1,1,1,4],None,1,10)['status']=='inconclusive-noise'
        try:evaluate([],None,1)
        except ValueError:pass
        else:raise AssertionError('empty samples cannot pass')
        print('Comparator negative controls passed')
    else:
        directory=Path(sys.argv[1]);baseline=Path(sys.argv[2]) if len(sys.argv)>2 else None
        print(json.dumps(compare(directory,baseline),indent=2))
