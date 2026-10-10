#!/usr/bin/env python3
"""Generate byte-stable qualification inputs; no dependency on archived evidence."""
import hashlib
import json
from pathlib import Path
import sys

def compact(value):
    return json.dumps(value, separators=(',', ':'), ensure_ascii=False)

def generate(destination):
    destination.mkdir(parents=True, exist_ok=True)
    manifest = {'schemaVersion': 1, 'tiers': {}, 'regression': {}}
    schema = {'type': 'array', 'items': {'type': 'integer'}}
    for tier, items, operations, repeats in [('small', 8, 1, 100), ('representative', 128, 16, 20), ('near', 900000, 16, 1)]:
        document = {'openbindings': '0.2.0', 'operations': {'run': {'aliases': ['check'], 'input': schema}}}
        document['operations'].update({f'op{i}': {'input': schema} for i in range(1, operations)})
        if tier == 'near':
            document['x-padding'] = [0] * items
        fixtures = {'document': compact(document), 'valid': compact([1] * items), 'invalid': compact(['x'] * items)}
        row = {'items': items, 'operations': operations, 'repetitions': repeats, 'validOutcome': 'no-verdict' if tier == 'near' else 'satisfies', 'files': {}}
        for kind, source in fixtures.items():
            name = f'{tier}-{kind}.json'
            raw = source.encode()
            (destination / name).write_bytes(raw)
            row['files'][kind] = {'name': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
        manifest['tiers'][tier] = row
    healthy = '{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"object","properties":{"n":{"type":"number"}},"required":["n"],"additionalProperties":false}}}}'
    duplicates = lambda n: '{' + ','.join(['"k":0'] * (n + 1)) + '}'
    regression = {
        'healthy': healthy,
        'duplicates-root': duplicates(5000),
        'duplicates-prefix': '{"openbindings":"0.2.0","operations":{},"x-pad":"' + 'p' * 1048576 + '",\n"x-duplicates":' + duplicates(5000) + '}',
        'duplicates-wide-key': '{"openbindings":"0.2.0","operations":{},"x-' + 'a' * 4096 + '":' + duplicates(4096) + '}',
        'names-prefix': '{"openbindings":"0.2.0","x-pad":"' + 'p' * 1048576 + '",\n"operations":{' + ','.join(f'"op{i}":{{}}' for i in range(4097)) + ',"run":{"aliases":[' + ',\n'.join(f'"op{i}"' for i in range(4097)) + ']}}}',
        'contract-wide': healthy.replace('"type":"number"', '"type":"number","minimum":1e999,"multipleOf":1e990'),
    }
    frozen = json.loads((Path(__file__).parent / 'regression-fixtures.json').read_text())
    for name, source in regression.items():
        raw = source.encode()
        identity = {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
        assert identity == frozen[name], (name, identity, frozen[name])
        (destination / (name + '.json')).write_bytes(raw)
        manifest['regression'][name] = identity
    key = 'k' * 32768
    witness = {'document': compact({'openbindings': '0.2.0', 'operations': {'check': {'input': {'type': 'object', 'properties': {key: {'type': 'object', 'additionalProperties': False}}}}}}),
               'invalid': compact({key: {f'f{i}': 0 for i in range(257)}})}
    manifest['adversarial'] = {}
    for kind, source in witness.items():
        name = f'amplification-{kind}.json'
        raw = source.encode()
        (destination / name).write_bytes(raw)
        manifest['adversarial'][kind] = {'name': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
    (destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return manifest

if __name__ == '__main__':
    generate(Path(sys.argv[1]))
