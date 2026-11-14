"""Verify the pinned current evaluator suite and complete historical case accounting."""
import argparse
import copy
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'crates/openbindings-schema-evaluator-test-support/fixtures'


def digest(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def unique(rows, key='id'):
    result = {row[key]: row for row in rows}
    assert len(result) == len(rows), 'duplicate case/group identity'
    return result


def check(data):
    historical = data['cases.json']
    current = data['current-suite.json']
    ledger = data['current-adjudication.json']
    moved = unique(data['current-admission.json'])
    records = unique(ledger['records'])
    old_groups = unique(historical['groups'])
    groups = unique(current['groups'])
    old_cases = unique([c for g in old_groups.values() for c in g['cases']])
    cases = unique([c for g in groups.values() for c in g['cases']])
    assert set(old_cases) == set(records) == set(cases) | set(moved)
    assert not set(cases) & set(moved), 'case counted in both admission and values'
    assert len(old_cases) == 1566 and len(cases) == 1546 and len(moved) == 20
    assert len(groups) == 486
    assert ledger['source_sha256'] == data['_manifest']['sha256']['cases.json']
    expressions = unique(data['current-reexpressions.json']['reexpressions'], 'group')
    group_records = unique(ledger['groups'])
    assert set(group_records) == set(old_groups)
    for gid, old in old_groups.items():
        record = group_records[gid]
        assert record['source_document_sha256'] == digest(old['document'])
        assert record['cases'] == [c['id'] for c in old['cases']]
        if gid in groups:
            group = groups[gid]
            expression = expressions.get(gid)
            assert group['document'] == (expression['document'] if expression else old['document'])
            assert record['effective_document_sha256'] == digest(group['document'])
            assert group['resources'] == old['resources'] + ([expression['resource_key']] if expression else [])
            assert [c['id'] for c in group['cases']] == [c['id'] for c in old['cases']]
            for case in group['cases']:
                before, decision = old_cases[case['id']], records[case['id']]
                assert case['value'] == before['value'], 'exact instance changed'
                assert case['acceptable_paths'] == before['acceptable_paths']
                assert case['expected'] == decision['expected']
                assert case['expected'] in ('satisfies', 'fails', 'undefined')
                assert case['optional_capability'] is None and case['refusal_kind'] is None
                assert decision['no_verdict_is_pass'] is False
                assert decision['disposition'] != 'move-to-document-admission'
        else:
            for case in old['cases']:
                negative, decision = moved[case['id']], records[case['id']]
                assert negative['group'] == gid and negative['documentJson'] == old['document']
                assert negative['original_value'] == case['value']
                assert negative['original_value_expectation'] == case['expected']
                assert negative['valid'] is False
                assert negative['violates'] == decision['expected']['violates']
                assert decision['disposition'] == 'move-to-document-admission'
    expected_resources = copy.deepcopy(historical['resources'])
    for expression in expressions.values():
        assert expression['original_document'] == old_groups[expression['group']]['document']
        assert expression['case_ids'] == [c['id'] for c in old_groups[expression['group']]['cases']]
        assert expression['resource_key'] not in expected_resources
        expected_resources[expression['resource_key']] = expression['resource']
    assert current['resources'] == expected_resources
    assert current['builtin_resources'] == historical['builtin_resources']


def load(directory):
    manifest = json.loads((directory / 'alignment.json').read_text())
    assert manifest['spec_revision'] == 'c1419c9fc3ae5a00c2b83da710c11bf67dab7934'
    assert manifest['interfaces_revision'] == 'a3c19658fc89815d7a417eae29ba31fd4e930daf'
    data = {'_manifest': manifest}
    for name, expected in manifest['sha256'].items():
        raw = (directory / name).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == expected, 'input hash drift: ' + name
        data[name] = json.loads(raw)
    return data


def controls(data):
    mutations = [
        lambda d: d['current-suite.json']['groups'].pop(),
        lambda d: d['current-suite.json']['groups'].append(d['current-suite.json']['groups'][0]),
        lambda d: d['current-admission.json'].pop(),
        lambda d: d['current-suite.json']['groups'][0]['cases'][0].update(value='null'),
        lambda d: d['current-suite.json']['groups'][0]['cases'][0].update(expected='no-verdict'),
        lambda d: d['current-suite.json']['groups'][0]['cases'][0].update(optional_capability='allowed'),
        lambda d: d['current-suite.json']['groups'][0].update(document='{}'),
        lambda d: d['current-admission.json'][0].update(documentJson='{}'),
        lambda d: d['current-adjudication.json']['records'][0].update(no_verdict_is_pass=True),
    ]
    for mutation in mutations:
        altered = copy.deepcopy(data)
        mutation(altered)
        try:
            check(altered)
        except (AssertionError, KeyError):
            pass
        else:
            raise AssertionError('adjudication mutation was accepted')
    return len(mutations)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    data = load(DIRECTORY)
    check(data)
    print(json.dumps({'status': 'pass', 'historical': 1566, 'value_cases': 1546,
                      'negative_admission_cases': 20,
                      'rejected_mutations': controls(data) if args.self_test else 0}))


if __name__ == '__main__':
    main()
