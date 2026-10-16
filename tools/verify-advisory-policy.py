"""Require a fail-closed advisory policy and justified advisory-ID exceptions."""
import argparse
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
ADVISORY_ID = re.compile(r'RUSTSEC-\d{4}-\d{4}')


def validate(config):
    if set(config) != {'advisories'}:
        raise ValueError('deny.toml must contain only the advisories section')
    advisories = config['advisories']
    if advisories.get('yanked') != 'deny' or advisories.get('disable-yank-checking', False):
        raise ValueError('Yanked versions must fail the advisory check')
    for entry in advisories.get('ignore', []):
        if not isinstance(entry, dict) or set(entry) != {'id', 'reason'}:
            raise ValueError('Every ignore needs an advisory ID and written reason')
        if not isinstance(entry['id'], str) or not ADVISORY_ID.fullmatch(entry['id']):
            raise ValueError('Every ignore must identify a RustSec advisory')
        if not isinstance(entry['reason'], str) or not entry['reason'].strip():
            raise ValueError('Every ignore needs a nonempty written reason')


def self_test():
    validate({'advisories': {'yanked': 'deny', 'ignore': []}})
    exception = {'id': 'RUSTSEC-2026-0001', 'reason': 'The affected feature is unreachable.'}
    validate({'advisories': {'yanked': 'deny', 'ignore': [exception]}})
    invalid = [
        {}, {'advisories': {'yanked': 'warn'}},
        {'advisories': {'yanked': 'deny', 'disable-yank-checking': True}},
        *({'advisories': {'yanked': 'deny', 'ignore': [entry]}} for entry in [
            exception['id'], {'id': exception['id']},
            {**exception, 'id': 'some-crate@1.0.0'},
            {**exception, 'reason': ''}, {**exception, 'reason': '   '},
            {**exception, 'reason': None},
        ]),
    ]
    for config in invalid:
        try:
            validate(config)
        except ValueError:
            continue
        raise AssertionError(f'Accepted unsafe advisory policy: {config!r}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        self_test()
    with (ROOT / 'deny.toml').open('rb') as source:
        validate(tomllib.load(source))
    print('Advisory policy: yanked versions denied; every exception has an ID and reason')


if __name__ == '__main__':
    main()
