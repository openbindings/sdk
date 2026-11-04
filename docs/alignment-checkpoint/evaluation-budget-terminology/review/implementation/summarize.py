from pathlib import Path
import json,re,hashlib
P=Path(__file__).resolve().parent;SDK=P.parents[3]/'sdk';summary={'status':'bounded native/source PASS; release-peer and rebuilt-host evidence pending','toolchains':{}}
def rows(log,prefix):return[json.loads(s.split(prefix,1)[1])for s in log.read_text().splitlines()if prefix in s]
for label in['current','minimum']:
 status=json.loads((P/f'{label}-results.json').read_text());assert all(v==0 for v in status.values());groups=[]
 for name in status:
  if name=='build':continue
  log=P/f'{label}-{name.replace("::","-")}.log';m=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',log.read_text());assert m;groups.append({'filter':name,'passed':int(m[1]),'failed':int(m[2]),'ignored':int(m[3])})
 log=P/f'{label}-independent_budget.log';summary['toolchains'][label]={'test_groups':groups,'passed':sum(x['passed']for x in groups),'defaults_and_domain_rows':rows(log,'LIMIT_ROUNDTRIP '),'meta_limits':rows(log,'META_LIMIT '),'carrier_observations':rows(log,'CARRIER_OBSERVATION ')}
assert {k:v for k,v in summary['toolchains']['current'].items()if k!='test_groups'}=={k:v for k,v in summary['toolchains']['minimum'].items()if k!='test_groups'}
sources=json.loads((P/'source-hashes.json').read_text());summary['source_hashes_unchanged']=all(hashlib.sha256((SDK/x['path']).read_bytes()).hexdigest()==x['after_sha256']for x in sources['files']);assert summary['source_hashes_unchanged'];summary['static_audit']={'changed_files':len(sources['files']),'exact_renames_only':sum(x['field_only_exact']for x in sources['files']),'json_spelling_only':sum(x.get('json_spelling_only',False)for x in sources['files']),'accounting_site_changes':False,'default_or_code_changes':False,'instrument_changed':['src/sdk.rs'],'third_party_lock_or_instrument_phase_changes':False};summary['sdk_edits_by_reviewer']=False
(P/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps({k:{'tests':v['passed'],'roundtrips':len(v['defaults_and_domain_rows']),'meta':len(v['meta_limits']),'carrier':len(v['carrier_observations'])}for k,v in summary['toolchains'].items()},indent=2))
