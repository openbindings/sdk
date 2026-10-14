"""Check complete S6c case-ID accounting against current public-API replays.

The ledger is an immutable before/after receipt for S6c, not a blanket allowance
for later semantic changes. Every changed fixture and observation is explicit.
"""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / 'crates/openbindings-schema-evaluator-test-support/fixtures'
LEDGER = ROOT / 'tools/qualification/inputs/s6c-migration-ledger.json'


def digest(value):
    return hashlib.sha256(value.encode()).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'))


def index(rows, key):
    result = {row[key]: row for row in rows}
    assert len(result) == len(rows), 'duplicate case IDs'
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kit-receipt', type=Path, required=True)
    parser.add_argument('--corpus-receipt', type=Path, required=True)
    args = parser.parse_args()
    ledger = json.loads(LEDGER.read_text())
    fixtures = json.loads((FIXTURES / 'cases.json').read_text())
    migrations = json.loads((FIXTURES / 'document-migrations.json').read_text())
    kit = json.loads(args.kit_receipt.read_text())
    corpus = [json.loads(line) for line in args.corpus_receipt.read_text().splitlines()]
    observations = ledger['pools']['observations']
    for pool, entries in ledger['pools'].items():
        for key, value in entries.items():
            if pool != 'documents':
                assert digest(canonical(value)) == key, 'corrupt interned evidence'
    frozen = index([c for g in fixtures['groups'] for c in g['cases']], 'id')
    recorded = index(ledger['kit'], 'case_id')
    assert recorded.keys() == frozen.keys() and len(recorded) == 1566
    rejected = {i for m in migrations['core_rejections'] for i in m['case_ids']}
    reexpressed = {i for m in migrations['reexpressions'] for i in m['case_ids']}
    assert len(rejected) == 2 and len(reexpressed) == 9
    actual = index(kit['report']['observations'], 'id')
    assert actual.keys() == recorded.keys() - rejected
    assert not kit['report']['configuration_failures']
    admissions = index(kit['admission'], 'id')
    resources = fixtures['resources'] | {m['resource_key']: m['resource'] for m in migrations['reexpressions']}
    for key, row in recorded.items():
        assert digest(frozen[key]['value']) == row['instance_sha256']
        if key in rejected:
            assert row['route'] == 'core-rejection'
            assert row['after']['rule'] == 'OBI-12' and row['after']['proof'] is False
            continue
        assert observations[row['after']] == {k: v for k, v in actual[key].items() if k != 'id'}, key
        assert not actual[key]['failures'], key
        assert row['route'] == ('validated-supplied-resource' if key in reexpressed else 'validated-value-contract')
        admission = admissions[row['group_id']]
        assert admission['report']['conclusion'] == 'conformant', key
        assert digest(admission['document']) == row['after_context']['document'], key
        resource_set = [{'key': k, 'uri': resources[k]['uri'], 'sha256': digest(resources[k]['documentJson'])} for k in admission['resources']]
        assert ledger['pools']['resource_sets'][row['after_context']['resources']] == resource_set, key
    actual = index(corpus, 'id')
    recorded = index(ledger['corpus'], 'case_id')
    assert actual.keys() == recorded.keys() and len(recorded) == 435
    for key, row in recorded.items():
        assert observations[row['after']] == actual[key], key
        assert row['before'] == row['after'], key
    print(json.dumps({'kit_ids': 1566, 'evaluated': 1564, 'core_rejections': sorted(rejected), 'reexpressed': 9, 'corpus_ids': 435, 'status': 'pass'}, indent=2))


if __name__ == '__main__':
    main()
