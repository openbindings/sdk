"""Compare public native and TypeScript corpus observations, including every finding."""
import argparse
import json
from pathlib import Path
import corpus

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--native', type=Path, required=True)
parser.add_argument('--typescript', type=Path, required=True)
args = parser.parse_args()

def load(path):
    rows = [json.loads(line) for line in path.read_text(encoding='utf-8').splitlines()]
    assert not corpus.reconcile(corpus.load(), rows)
    return {row['id']: row for row in rows}

def canonical(value):
    if isinstance(value, list):
        return [canonical(item) for item in value]
    if isinstance(value, dict):
        names = {'byteOffset': 'byte_offset', 'byteColumn': 'byte_column',
                 'instancePointer': 'instance_pointer', 'schemaLocation': 'schema_location',
                 'problemsComplete': 'problems_complete'}
        # The TS observer also records setup tags. Both observers' outcome/refusal
        # and independent corpus judgments remain required. Finding truncation is
        # independently covered by the shared native/Node/package/browser cases.
        return {names.get(key, key): canonical(item) for key, item in value.items()
                if key not in {'selection', 'preparation', 'findings_truncated'}}
    return value

native, typescript = load(args.native), load(args.typescript)
for case in corpus.load():
    for observed in [native, typescript]:
        assert corpus.judge(case, observed[case['id']]), case['id']
    assert canonical(native[case['id']]) == canonical(typescript[case['id']]), case['id']
print(json.dumps({'cases': len(native), 'judgments': 'pass', 'findings_and_outcomes_parity': 'pass'}))
