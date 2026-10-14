#!/usr/bin/env python3
"""Pin diagnostic-only migrations by ID; reject verdict/refusal/evidence drift."""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
LEDGER = ROOT / 'tools/qualification/inputs/diagnostics-migration-ledger.json'

def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))

def indexed(rows):
    result = {r['id']: r for r in rows}
    assert len(result) == len(rows), 'duplicate case IDs'
    return result

def semantic(value):
    if isinstance(value, list):
        return [semantic(v) for v in value]
    if isinstance(value, dict):
        # Diagnostic text/locations in refusals stay part of the exact comparison.
        omitted = {'findings', 'findings_truncated'}
        if value.get('outcome') == 'fails':
            omitted |= {'message', 'problems', 'problems_complete'}
        return {k: semantic(v) for k, v in value.items() if k not in omitted}
    return value

def compare(before, after):
    before, after = indexed(before), indexed(after)
    assert before.keys() == after.keys(), 'case inventory changed'
    changes = []
    for case_id in sorted(before):
        a, b = before[case_id], after[case_id]
        assert semantic(a) == semantic(b), f'verdict/refusal/evidence drift: {case_id}'
        if a != b:
            changes.append({'id': case_id, 'before': {k: v for k, v in a.items() if b.get(k) != v}, 'after': {k: v for k, v in b.items() if a.get(k) != v}})
    return changes

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before-kit', required=True)
    parser.add_argument('--coarse-kit', required=True)
    parser.add_argument('--upstream-kit', required=True)
    parser.add_argument('--before-corpus', required=True)
    parser.add_argument('--after-corpus', required=True)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    before, coarse, upstream = map(load, [args.before_kit, args.coarse_kit, args.upstream_kit])
    for receipt in [coarse, upstream]:
        assert not receipt['report']['configuration_failures']
        assert all(not r['failures'] for r in receipt['report']['observations'])
    corpus = lambda path: [json.loads(line) for line in Path(path).read_text(encoding='utf-8').splitlines()]
    old_corpus, new_corpus = map(corpus, [args.before_corpus, args.after_corpus])
    locations = load(ROOT / 'tools/qualification/inputs/document-location-expectations.json')
    by_id = indexed(new_corpus)
    for case in locations['cases']:
        actual = [f['location']['pointer'] for f in by_id[case['id']]['findings'] if f['rule'] == case['rule']]
        assert all(pointer in actual for pointer in case['expected_pointers']), (case['id'], actual)
    ledger = {
        'subject': 'diagnostics modes and document locations',
        'policy': 'Verdicts, refusal stages/reasons/codes, and rule evidence unchanged; only diagnostics differ.',
        'counts': {'kit': len(before['report']['observations']), 'corpus': len(old_corpus)},
        'document_locations': locations,
        'kit_coarse': compare(before['report']['observations'], coarse['report']['observations']),
        'kit_upstream': compare(before['report']['observations'], upstream['report']['observations']),
        'kit_document_findings': compare(before['admission'], coarse['admission']),
        'corpus': compare(old_corpus, new_corpus),
    }
    assert coarse['admission'] == upstream['admission']
    if args.write:
        LEDGER.write_text(json.dumps(ledger, indent=2, ensure_ascii=True) + '\n', encoding='utf-8', newline='\n')
    else:
        assert ledger == load(LEDGER), 'diagnostic migration ledger differs'
    print(json.dumps({key: len(ledger[key]) for key in ['kit_coarse','kit_upstream','kit_document_findings','corpus']}))

if __name__ == '__main__':
    main()
