#!/usr/bin/env python3
from pathlib import Path
import json
p=Path(__file__).resolve().parent
rows={}
for name in ['current-candidate','current-original','minimum-candidate','minimum-original']:
 rows[name]=[json.loads(x.split('REVIEW_JSON ',1)[1]) for x in (p/(name+'-independent_hot_path_ownership.log')).read_text().splitlines() if 'REVIEW_JSON ' in x]
key=lambda x:(x['case'],x.get('detail'))
def outcome(row):return row.get('outcome',row.get('outcome_json'))
result={}
for a,b in [('current-original','current-candidate'),('minimum-original','minimum-candidate'),('current-candidate','minimum-candidate')]:
 ra={key(x):x for x in rows[a]};rb={key(x):x for x in rows[b]};assert len(ra)==len(rb)==106
 diff=[{'key':k,'left':outcome(ra[k]),'right':outcome(rb[k])} for k in ra if outcome(ra[k])!=outcome(rb[k])]
 result[a+' versus '+b]={'rows':len(ra),'outcome_differences':diff,'checks_changed':sum(ra[k].get('checks')!=rb[k].get('checks') for k in ra),'check_total_left':sum(x.get('checks',0) for x in ra.values()),'check_total_right':sum(x.get('checks',0) for x in rb.values())};assert not diff
(p/'public-observation-comparison.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
