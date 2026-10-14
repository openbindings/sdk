"""Account for every S6d/e kit/corpus observation against S6c and approved deltas.

Raw fixtures and the S6c ledger are immutable. No unspecified refusal, coordinate,
partition or diagnostic change is admitted by this check.
"""
import argparse
import collections
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
INPUTS = ROOT / 'tools/qualification/inputs'
FIXTURES = ROOT / 'crates/openbindings-schema-evaluator-test-support/fixtures'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def index(rows, key):
    result = {row[key]: row for row in rows}
    assert len(result) == len(rows), 'duplicate case IDs'
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kit-receipt', type=Path, required=True)
    parser.add_argument('--corpus-receipt', type=Path, required=True)
    args = parser.parse_args()
    ledger = json.loads((INPUTS / 's6de-migration-ledger.json').read_text())
    baseline_path = INPUTS / ledger['baseline_ledger']
    assert digest(baseline_path.read_bytes()) == ledger['baseline_sha256']
    baseline = json.loads(baseline_path.read_text())
    assert digest((FIXTURES / 'cases.json').read_bytes()) == ledger['raw_fixture_sha256']
    fixture = json.loads((FIXTURES / 'cases.json').read_text())
    documents = json.loads((FIXTURES / 'document-migrations.json').read_text())
    resources = fixture['resources'] | {r['resource_key']: r['resource'] for r in documents['reexpressions']}
    frozen = index([c for group in fixture['groups'] for c in group['cases']], 'id')
    recorded = index(baseline['kit'], 'case_id')
    assert frozen.keys() == recorded.keys() and len(frozen) == 1566
    rejected = {i for row in documents['core_rejections'] for i in row['case_ids']}
    assert len(rejected) == 2
    changes = index(ledger['kit_changes'], 'case_id')
    provenance = index(json.loads((FIXTURES / 'provenance.json').read_text())['runtime_policy_translations'], 'case')
    kit = json.loads(args.kit_receipt.read_text())
    actual = index(kit['report']['observations'], 'id')
    admissions = index(kit['admission'], 'id')
    assert actual.keys() == recorded.keys() - rejected and len(actual) == 1564
    assert not kit['report']['configuration_failures']
    observed_changes = set()
    for key, row in recorded.items():
        assert digest(frozen[key]['value'].encode()) == row['instance_sha256']
        if key in rejected:
            assert row['route'] == 'core-rejection' and row['after']['rule'] == 'OBI-12'
            continue
        before = baseline['pools']['observations'][row['after']]
        observed = {k: v for k, v in actual[key].items() if k != 'id'}
        assert not observed['failures'], key
        if key in changes:
            change = changes[key]
            assert change['before'] == before and change['after'] == observed, key
            assert before != observed and change['authority'] and change['reason'], key
            assert provenance[key]['authority'] and provenance[key]['to_expected'] == observed['expected'], key
            for field in ['original_document', 'original_value', 'original_resources']:
                assert change[field] == provenance[key][field], key
            observed_changes.add(key)
        else:
            assert observed == before, key
        admission = admissions[row['group_id']]
        assert admission['report']['conclusion'] == 'conformant', key
        assert digest(admission['document'].encode()) == row['after_context']['document'], key
        supplied = [{'key': k, 'uri': resources[k]['uri'], 'sha256': digest(resources[k]['documentJson'].encode())} for k in admission['resources']]
        assert baseline['pools']['resource_sets'][row['after_context']['resources']] == supplied, key
    assert observed_changes == changes.keys()
    corpus = index([json.loads(line) for line in args.corpus_receipt.read_text().splitlines()], 'id')
    old_corpus = index(baseline['corpus'], 'case_id')
    assert corpus.keys() == old_corpus.keys() and len(corpus) == 435
    for key, row in old_corpus.items():
        assert corpus[key] == baseline['pools']['observations'][row['after']], key
    partitions = collections.Counter('preparation-refusal' if r['preparation_refusal'] else r['outcome']['outcome'] for r in actual.values())
    assert dict(partitions) == ledger['summary']['after_partitions']
    print(json.dumps({'status': 'pass', 'kit_ids': 1566, 'evaluated': 1564,
                      'core_rejections': sorted(rejected), 'changed_case_ids': sorted(changes),
                      'corpus_ids': 435, 'changed_corpus_ids': [], 'partitions': dict(partitions)}, indent=2))


if __name__ == '__main__':
    main()
