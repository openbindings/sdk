#!/usr/bin/env python3
from pathlib import Path
import json
p=Path(__file__).resolve().parent;old=p.parents[1]/'mask-keyword-inline-review/implementation'
def extract(path,prefix):return [json.loads(x.split(prefix+' ',1)[1])for x in path.read_text().splitlines()if prefix+' 'in x]
result={};direct={};allocs={}
for label in ['current','minimum']:
 f=p/f'{label}-independent_hot_path_ownership.log';b=old/f'{label}-baseline-independent_hot_path_ownership.log'
 a={(x['case'],x.get('detail')):x for x in extract(b,'REVIEW_JSON')};z={(x['case'],x.get('detail')):x for x in extract(f,'REVIEW_JSON')};assert a.keys()==z.keys();diff=[k for k in a if a[k]!=z[k]]
 ta={(x['id'],x['cap'],x['detail']):x for x in extract(b,'INLINE_TRACE')};tz={(x['id'],x['cap'],x['detail']):x for x in extract(f,'INLINE_TRACE')};tdiff=[k for k in ta if ta[k]!=tz[k]];assert not diff and not tdiff
 direct[label]=extract(p/f'{label}-independent_integer.log','INTEGER_TRACE');al=extract(p/f'{label}-independent_integer.log','INTEGER_ALLOCATION')
 for row in al:
  for key in ['requested_bytes_per_op','allocations_per_op','incremental_peak_bytes']:assert row['old'][key]==row['new'][key]
  for side in ['old','new']:assert row[side]['ending_live_bytes']==row[side]['baseline_live_bytes']
 allocs[label]=[{'id':x['id'],**{k:x['new'][k]for k in ['requested_bytes_per_op','allocations_per_op','incremental_peak_bytes']}}for x in al]
 result[label]={'public_records':len(a),'public_full_record_differences':diff,'public_trace_rows':len(ta),'public_trace_differences':tdiff,'direct_token_cases':52,'direct_budget_prespent_rows':len(direct[label]),'direct_stop_rows':sum(len(x['stops'])for x in direct[label]),'paired_actual_allocation_rows':len(al),'paired_allocation_differences':0,'retained_allocations':0}
assert direct['current']==direct['minimum']
result['direct_current_minimum_trace_differences']=0
result['cross_toolchain_allocation_note']='Five wide fallback cases differ between compiler builds identically on both old and new paths. Exact actual allocation equality is checked within each paired build; trace/reservation policy is identical across builds. See cross-toolchain-allocation-differences.json.'
(p/'comparison.json').write_text(json.dumps(result,indent=2)+'\n');(p/'allocation-summary.json').write_text(json.dumps(allocs,indent=2)+'\n');print(json.dumps(result,indent=2))
