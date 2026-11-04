from pathlib import Path
import json,re
P=Path(__file__).resolve().parent
results={}
for label in ['current','minimum']:
 def read(variant):
  return [json.loads(s.split('REVIEW_JSON ',1)[1])for s in(P/f'{label}-{variant}-independent_hot_path_ownership.log').read_text().splitlines()if 'REVIEW_JSON ' in s]
 baseline=read('baseline');candidate=read('candidate');assert len(baseline)==len(candidate)
 old={(x['case'],x.get('detail')):x for x in baseline};new={(x['case'],x.get('detail')):x for x in candidate};assert old.keys()==new.keys()
 for key in old:
  for field in ['outcome','outcome_json']:assert old[key].get(field)==new[key].get(field),(label,key,field)
 changes=[{'case':list(key),'before':old[key]['checks'],'after':new[key]['checks']}for key in old if 'checks'in old[key] and old[key]['checks']!=new[key]['checks']]
 results[label]={'complete_observations':len(old),'all_match':True,'changed_callback_counts':changes,'exhaustive_candidate_public_stops':sum(len(v.get('stops',[]))for v in candidate)}
 statuses=json.loads((P/f'{label}-candidate-statuses.json').read_text());assert all(v==0 for v in statuses.values())
 logs=[P/f'{label}-candidate-{f.replace("::","-")}.log'for f in statuses]
 groups=[]
 for log in logs:
  m=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',log.read_text());assert m,log
  groups.append({'log':log.name,'passed':int(m[1]),'failed':int(m[2]),'ignored':int(m[3])})
 results[label]['tested_groups']=groups
(P/'comparison.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps({k:{'observations':v['complete_observations'],'stops':v['exhaustive_candidate_public_stops'],'changed_counts':len(v['changed_callback_counts']),'test_passes':sum(x['passed']for x in v['tested_groups'])}for k,v in results.items()},indent=2))
