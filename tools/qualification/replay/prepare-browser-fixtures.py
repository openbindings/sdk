from pathlib import Path
import base64,json,importlib.util
import corpus
root=Path(__file__).resolve().parents[3]
fixture=json.loads((root/'crates/openbindings-schema-evaluator-test-support/fixtures/cases.json').read_text())
spec=importlib.util.spec_from_file_location('browser_judge',Path(__file__).with_name('judge-browser.py'))
judge=importlib.util.module_from_spec(spec);spec.loader.exec_module(judge)
fixture=judge.current_fixture(fixture)
suite=[]
for group in fixture['groups']:
 suite.append({'id':group['id'],'action':'validate-operation-values','given':{'documentBase64':base64.b64encode(group['document'].encode()).decode(),'resources':[fixture['resources'][r] for r in group['resources']],'operation':'op','side':'input','valuesJson':[c['value'] for c in group['cases']]}})
output=corpus.OUT/'corpus';output.mkdir(exist_ok=True)
(output/'browser-requests.json').write_text(json.dumps({'core':[corpus.request(c) for c in corpus.load()],'suite':suite})+'\n')
