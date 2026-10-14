"""Preserve raw allocation/time numbers and summarize the document diagnostics run."""
from pathlib import Path
import hashlib
import json
import statistics

root = Path(__file__).resolve().parents[2]
out = root / 'target/document-diagnostics'
rows = [json.loads(line) for line in (out / 'memory.jsonl').read_text(encoding='utf-8').splitlines()]
fixtures = json.loads((out / 'fixtures/inputs.json').read_text(encoding='utf-8'))
raw = []
for row in rows:
    row = dict(row)
    if 'locations' in row:
        locations = row.pop('locations')
        row['finding_count'] = len(locations)
        row['pointer_bytes'] = sum(len((loc.get('pointer') or '').encode()) + loc.get('pointer_bytes', 0) for loc in locations)
    raw.append(row)
summary = {}
for label, ordinary in [('large', False), ('corpus', True)]:
    samples = [r for r in raw if r['phase'] == 'document' and (r['group'] == 'corpus') == ordinary]
    summary[label] = {'cases': len({r['id'] for r in samples}), 'samples': len(samples)}
    for mode in ['validity', 'location', 'fixed', 'sdk']:
        largest = max(samples, key=lambda r: r[mode + '_peak_bytes'])
        summary[label][mode] = {
            'max_peak_bytes': largest[mode + '_peak_bytes'], 'max_peak_case': largest['id'],
            'median_nanos': statistics.median(r[mode + '_nanos'] for r in samples),
            'max_nanos': max(r[mode + '_nanos'] for r in samples),
        }
    walked = [r for r in samples if r['location_walk']]
    summary[label]['location_walk_cases'] = len({r['id'] for r in walked})
    summary[label]['median_active_location_nanos'] = statistics.median(r['location_nanos'] for r in walked)
    summary[label]['max_fixed_minus_validity_bytes'] = max(r['fixed_peak_bytes'] - r['validity_peak_bytes'] for r in samples)
    summary[label]['max_sdk_minus_validity_bytes'] = max(r['sdk_peak_bytes'] - r['validity_peak_bytes'] for r in samples)
    summary[label]['median_fixed_minus_validity_nanos'] = statistics.median(r['fixed_nanos'] - r['validity_nanos'] for r in samples)
files = [root / 'crates/openbindings-internal-schema/src/fixed_schema.rs',
         root / 'crates/openbindings-internal-schema/src/fixed_locations.rs',
         root / 'tools/diagnostic-memory-probe/src/main.rs',
         root / 'tools/diagnostic-memory-probe/src/values.rs',
         root / 'crates/openbindings-json-schema-evaluator/src/lib.rs',
         root / 'tools/diagnostic-memory-probe/generate_inputs.py']
files += sorted((root / 'crates/openbindings-internal-schema/schemas').rglob('*.json'))
receipt = {
    'scope': 'Release macOS arm64; System allocator incremental live peak, not RSS. Parsing, schema index construction for isolated checks, receipt encoding and warmed compilation excluded. Complete SDK samples use fresh owners, so validation caching is excluded.',
    'bounds_bytes': {'validity': 18 * 1024**2, 'fixed_above_validity': 10 * 1024**2, 'sdk_above_validity': 12 * 1024**2},
    'default_value_samples': [r for r in raw if r['phase'] == 'default-value'],
    'compilation': [r for r in raw if r['phase'] == 'location-map-compilation'],
    'summary': summary,
    'source_sha256': {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files},
    'inputs': [{k: v for k, v in r.items() if k != 'path'} for r in fixtures],
    'samples': raw,
}
(out / 'allocation-samples.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps(summary, indent=2))
