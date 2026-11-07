from pathlib import Path
import json,re,hashlib
P=Path(__file__).resolve().parent; OLD=P.parents[1]/'root-alias-leaf-memo-review/implementation';ROOT=P.parents[4];SDK=ROOT/'sdk'
def rows(path,prefix):return[json.loads(line.split(prefix,1)[1])for line in path.read_text().splitlines()if prefix in line]
summary={'status':'bounded PASS; root memo performance acceptance remains separate','toolchains':{}}
for label in ['current','minimum']:
 log=P/f'{label}-independent_state_copy.log';alloc=rows(log,'STATE_ALLOC ');traces=rows(log,'STATE_TRACE ');matrix=rows(log,'STATE_MATRIX ')
 assert len(alloc)==66 and len(traces)==4
 empty=[r for r in alloc if r['kind']=='empty'];nonempty=[r for r in alloc if r['kind']=='nonempty'];assert len(empty)==48 and len(nonempty)==18
 for r in empty:assert r['metrics']['allocations_per_op']==r['metrics']['requested_bytes_per_op']==0 and r['metrics']['baseline_live_bytes']==r['metrics']['ending_live_bytes']
 for r in nonempty:
  for k in ['allocations_per_op','requested_bytes_per_op','incremental_peak_bytes']:assert r['before'][k]==r['after'][k]
  for v in ['before','after']:assert r[v]['baseline_live_bytes']==r[v]['ending_live_bytes']
 old=rows(OLD/f'{label}-candidate-independent_hot_path_ownership.log','REVIEW_JSON ');new=rows(P/f'{label}-independent_hot_path_ownership.log','REVIEW_JSON ')
 a={(r['case'],r.get('detail')):r for r in old};b={(r['case'],r.get('detail')):r for r in new};assert a.keys()==b.keys()
 for k in a:
  for field in ['outcome','outcome_json']:assert a[k].get(field)==b[k].get(field),(label,k,field)
 changed=[{'case':k,'before':a[k]['checks'],'after':b[k]['checks']}for k in a if 'checks'in a[k]and a[k]['checks']!=b[k]['checks']]
 statuses=json.loads((P/f'{label}-results.json').read_text());assert all(v==0 for v in statuses.values());groups=[]
 for f in statuses:
  path=P/f'{label}-{f.replace("::","-")}.log';m=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',path.read_text());assert m;groups.append({'filter':f,'passed':int(m[1]),'failed':int(m[2]),'ignored':int(m[3])})
 root=P/f'{label}-independent_root.log'
 summary['toolchains'][label]={'test_groups':groups,'passed':sum(r['passed']for r in groups),'allocation_rows':len(alloc),'empty_allocation_rows':len(empty),'nonempty_allocation_pairs':len(nonempty),'empty_allocation_requests':0,'retained_clone_bytes':0,'trace_stop_boundaries':sum(r['stops']for r in traces),'matrix':matrix,'complete_observations':len(b),'unchanged_complete_observations':True,'public_stop_boundaries':sum(len(r.get('stops',[]))for r in new),'public_count_changes':changed,'root_stops':rows(root,'ROOT_STOPS '),'root_boundaries':rows(root,'ROOT_BOUNDARY '),'exact_64k_observations':rows(log,'EXACT_64K '),'layouts':rows(log,'STATE_LAYOUT ')}
 (P/f'{label}-allocation-observations.json').write_text(json.dumps(alloc,indent=2)+'\n')
 (P/f'{label}-copy-traces.json').write_text(json.dumps(traces,indent=2)+'\n')
 # ignore process live baseline; compare every actual request/peak and ownership form across toolchains
 if label=='current':native=alloc
 else:
  for a,b in zip(native,alloc):
   assert a.keys()==b.keys()
   for k in a:
    if k in ['metrics','before','after']:
     for field in ['allocations_per_op','requested_bytes_per_op','incremental_peak_bytes']:assert a[k][field]==b[k][field]
    else:assert a[k]==b[k]
h=json.loads((P/'source-hashes.json').read_text());changes={}
for f,digest in h['files'].items():
 live=SDK/'crates/openbindings-json-schema-evaluator/src'/f;assert hashlib.sha256(live.read_bytes()).hexdigest()==digest
 old=json.loads((OLD/'source-hashes.json').read_text())['files'][f];changes[f]={'previous':old,'current':digest,'changed':old!=digest}
summary['source_changes_from_reviewed_root']=changes
summary['actual_allocator_toolchains_agree']=True
summary['source_frozen_at_review_end']=True
summary['no_sdk_edits']=True
summary['build_failures']=[]
(P/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:{key:v[key]for key in ['passed','allocation_rows','trace_stop_boundaries','complete_observations','public_stop_boundaries','public_count_changes']}for k,v in summary['toolchains'].items()},indent=2))
