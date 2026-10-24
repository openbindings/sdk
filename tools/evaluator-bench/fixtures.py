"""Deterministic exact-token workloads; expectations are independent of timings."""
import json

def compact(value):
    return json.dumps(value, separators=(',', ':'))

def fixtures():
    rows = {}
    def add(name, schema, value, status='satisfies', modes=('eval',), **options):
        rows[name] = dict(schema=schema if isinstance(schema, str) else compact(schema),
                          value=value, expected=dict(status=status), modes=list(modes), **options)
        return rows[name]
    ordinary = ('cold', 'compile', 'context', 'eval', 'parse', 'full', 'baseline')
    obj = dict(type='object', properties=dict(name=dict(type='string', minLength=3, pattern='^[A-Za-z]+$'),
               age=dict(type='integer', minimum=0), tags=dict(type='array', items=dict(type='string'))),
               required=['name', 'age'], additionalProperties=False)
    value = dict(name='Matthew', age=35, tags=['review', 'schema', 'sdk'] * 10)
    array = compact(list(range(20000)))
    add('scalar', dict(type='integer', minimum=0, maximum=100), '42', modes=ordinary)
    add('object', obj, compact(value), modes=ordinary)
    add('array', dict(type='array', items=dict(type='integer', minimum=0)), array, modes=ordinary)
    add('refs', {'$defs': {'leaf': obj}, 'type': 'array', 'items': {'$ref': '#/$defs/leaf'}}, compact([value] * 20), modes=ordinary)
    add('large-object', dict(type='object', additionalProperties=dict(type='integer')), compact({f'key{i}': i for i in range(50000)}), modes=ordinary)
    add('large-literal', {'const': list(range(20000))}, array, modes=('cold', 'compile', 'context', 'eval', 'full'))
    add('decimal-array', '{"type":"array","items":{"multipleOf":0.1,"minimum":0}}', '['+','.join(f'{i//10}.{i%10}' for i in range(5000))+']')
    add('wide-integers', '{"type":"array","items":{"minimum":9007199254740993}}', '[9007199254740993,9223372036854775807,9223372036854775808]')
    add('wide-adjacent', '{"minimum":9007199254740993}', '9007199254740992', 'fails')
    add('arithmetic-stop', '{"multipleOf":3}', '1e'+'9'*5000, 'no-verdict')['expected']['code'] = 'numeric-arithmetic-limit'
    add('huge-exponent', '{"minimum":1e400}', '10e399')
    add('mixed-equality', '{"const":[-0,9223372036854775807,1e400]}', '[0.0,9223372036854775807.0,10e399]')
    add('unique-array', dict(uniqueItems=True), array)
    add('unique-objects', dict(uniqueItems=True), compact([dict(a=i, b=[i, i+1]) for i in range(1000)]))
    add('late-duplicate', dict(uniqueItems=True), array[:-1]+',19999.0]', 'fails')
    add('equal-decimals', dict(uniqueItems=True), '[0.1,0.10]', 'fails')
    add('late-invalid', dict(type='array', items=dict(type='integer', minimum=0)), array[:-1]+',-1]', 'fails', modes=('eval', 'parse', 'full'))
    add('one-of', dict(oneOf=[dict(const=i) for i in range(128)]), '127')
    add('explicit-resource', {'$ref': 'https://benchmark.invalid/item'}, '1', resources={'https://benchmark.invalid/item': '{"type":"integer"}'}, modes=ordinary)
    # Many independent referring resources share one large supplied carrier.
    # context isolates public contract preparation from input construction.
    add('reference-fan-in', {'allOf': [
        {'$id': f'https://scale.test/caller/{i}', '$ref': 'https://scale.test/target'}
        for i in range(100)]}, '1', resources={'https://scale.test/target': compact({
            '$id': 'https://scale.test/target', 'type': 'integer',
            '$defs': {f'p{i}': {'type': 'integer'} for i in range(5000)}})},
        modes=('cold', 'compile', 'context', 'eval', 'full'))
    # Long repeated reference spellings must not be copied into each cache layer.
    for count, width in [(1000, 256), (5000, 1024)]:
        uri = 'https://memory.test/' + 'a' * width
        add(f'reference-text-{count}', {'allOf': [
            {'$ref': uri + '#/$defs/value'} for _ in range(count)]}, '1',
            resources={uri: compact({'$id': uri, '$defs': {'value': {'type': 'integer'}}})},
            modes=('compile', 'context', 'eval', 'full'))
    # unevaluatedProperties reaches recursive patternProperties through 100 referrers. Its walk
    # keeps only the patterns; eval's retained heap shows any copy of their subschemas it holds.
    patterns = {f'^p{j}': {'properties': {'x': {'$ref': '#'}}, 'minProperties': 1} for j in range(5)}
    walks = {f'R{i}': {'unevaluatedProperties': False, '$ref': '#/$defs/patterns'} for i in range(100)}
    add('unevaluated-patterns', {'properties': {f'r{i}': {'$ref': f'#/$defs/R{i}'} for i in range(100)},
        '$defs': {**walks, 'patterns': {'patternProperties': patterns}}},
        compact({'r0': {'p0': {'x': {}}}, 'r99': {'p4': {'x': {'r1': {'p2': {'x': {}}}}}}}),
        modes=('compile', 'context', 'eval', 'full'))
    # A value that reaches an unsupplied resource gives no verdict naming it;
    # a reference that identifies no schema refuses preparation, and a refused
    # preparation gives no verdict for any value.
    add('missing-resource', {'$ref': 'https://benchmark.invalid/missing'}, '1', 'no-verdict', modes=('cold', 'compile', 'context', 'eval', 'full'))
    add('missing-independent', {'anyOf': [True, {'$ref': 'https://benchmark.invalid/missing'}]}, '1')
    add('undefined-pointer', {'$ref': '#/$defs/absent'}, '1', 'no-verdict')
    recursive = {'$defs': {'node': {'anyOf': [{'type': 'integer'}, {'type': 'array', 'items': {'$ref': '#/$defs/node'}}]}}, '$ref': '#/$defs/node'}
    add('deep', recursive, '['*32+'0'+']'*32)
    add('depth-stop', recursive, '['*32+'0'+']'*32, 'no-verdict', limits=dict(evaluation_depth=8))
    add('work-stop', {'type': 'array', 'items': {'type': 'integer'}}, array, 'no-verdict', limits=dict(evaluation_steps=16))
    # Patterns match in time linear in the string, so a megabyte string is decided.
    add('regex-failure', dict(pattern='[A-Z]+[0-9]+'), compact('a'*65536), 'fails')
    add('regex-stop', dict(pattern='[A-Z]+[0-9]+'), compact('a'*1048576), 'fails')
    add('cancelled', True, '1', 'no-verdict', cancelled=True)
    add('source-diagnostics', '{"maximum":9223372036854775807}', '9223372036854775808', 'fails', upstream=True)
    for name, code in [('missing-resource', 'resource-unavailable'),
                       ('undefined-pointer', 'non-schema-target'), ('depth-stop', 'evaluation-depth-limit'),
                       ('work-stop', 'evaluation-work-limit'), ('cancelled', 'caller-cancelled')]:
        rows[name]['expected']['code'] = code
    rows['missing-resource']['expected']['detail'] = dict(code='resource-unavailable',
        location=dict(pointer='', resource='https://benchmark.invalid/missing'),
        message='evaluation reached a resource that was not supplied', reason='resource-unavailable')
    rows['work-stop']['expected']['detail'] = dict(code='evaluation-work-limit', location=None,
        message='evaluation work limit reached', reason='limit-exceeded')
    # The refusal states its arithmetic and the setting to raise.
    rows['depth-stop']['expected']['detail'] = dict(code='evaluation-depth-limit', location=None,
        message='this value nests more than 0 levels, the deepest this value contract evaluates within the '
                '8 schema evaluations its 2 KiB stack_bytes holds at 256 bytes each; raise stack_bytes '
                '(stackBytes in TypeScript)', reason='limit-exceeded')
    rows['source-diagnostics']['expected'].update(problems_complete=False, problems=[dict(
        code='maximum', instance_pointer='',
        message='number exceeds the inclusive upper bound; inspect maximum at the schema location',
        schema_location=dict(pointer='/maximum', resource='https://benchmark.invalid/root'))])
    # Fixed text includes escaped Unicode so conformant text access does work.
    for count in [1, 50, 1000]:
        operations = {}
        for i in range(count):
            op = dict(aliases=[f'alias{i:04d}', f'legacy{i:04d}'],
                      description=f'Operation {i}: café \u2603', tags=['api', f'group{i % 7}'])
            if i % 3 != 1:
                op['input'] = {'$ref': '#/schemas/shared/$defs/item'} if count == 1000 else {'type': 'integer'}
            if i % 3 != 2:
                op['output'] = {'$ref': '#/schemas/shared/$defs/item'} if count == 1000 else True
            operations[f'op{i:04d}'] = op
        document = dict(openbindings='0.2.0', operations=operations)
        if count == 1000:
            document['schemas'] = {'shared': {'$defs': {'item': {
                'type': 'object', 'properties': {'id': {'type': 'integer'}, 'label': {'type': 'string'}}}}}}
        reads = dict(operations=count, aliases=2*count, descriptions=count, tags=2*count,
                     inputs=sum('input' in op for op in operations.values()),
                     outputs=sum('output' in op for op in operations.values()),
                     text_bytes=sum(len(text.encode()) for key, op in operations.items()
                                    for text in [key, *op['aliases'], op['description'], *op['tags']]))
        rows[f'document-{count}'] = dict(document=compact(document),
            expected=dict(status='conformant', finding_count=0), modes=['document'])
        rows[f'read-{count}'] = dict(document=compact(document),
            expected=dict(status='conformant', finding_count=0, reads=reads), modes=['read'])
    rows['document-invalid'] = dict(document=compact(dict(openbindings='0.2.0', operations={
        'bad name': {'input': None, 'tags': 42},
        'other': {'aliases': ['same']}, 'third': {'aliases': ['same']}})),
        expected=dict(status='non-conformant', finding_count=6), modes=['document'])
    # The complete report: each finding's rule, code, original location and
    # message, so a changed message or location is a declared outcome change.
    def finding(rule, code, pointer, offset, message):
        return dict(rule=rule, status='violated', code=code, message=message,
                    location=dict(pointer=pointer, byte_offset=offset, line=1, byte_column=offset + 1))
    fixed = 'value violates the fixed normative schema'
    rows['document-invalid']['expected']['report'] = dict(
        release='0.2.0', revision='30416ef1ca626e58eb8c383a0c71422a68f9243f',
        conclusion='non-conformant',
        evidence={f'OBI-{rule:02d}': 'violated' if rule in (2, 4, 5, 10) else 'met' for rule in range(1, 14)},
        findings=[
            finding('OBI-02', 'schema-mismatch', '/operations/bad name', 38, fixed),
            finding('OBI-02', 'schema-mismatch', '/operations/bad name/input', 58, fixed),
            finding('OBI-02', 'schema-mismatch', '/operations/bad name/tags', 70, fixed),
            finding('OBI-04', 'name-grammar', '/operations/bad name', 38,
                    'a name must be a nonempty ASCII string: start with a letter, digit or underscore; '
                    'then use letters, digits, underscores, dots or hyphens'),
            finding('OBI-05', 'duplicate-operation-name', '/operations/third/aliases/0', 123,
                    'operation keys and aliases must be distinct'),
            finding('OBI-10', 'schema-mismatch', '/operations/bad name/input', 58,
                    'a schema must be a JSON object or boolean'),
        ],
        findings_truncated=False)
    add('explain-one', {'type': 'array', 'items': {'type': 'integer'}}, '["bad"]',
        'fails', modes=('explain',))['expected'].update(problem_count=1, problems_complete=False,
        first_problem=dict(code='type', instance_pointer='/0',
            schema_location=dict(pointer='/items/type', resource='https://benchmark.invalid/root'),
            message='expected JSON type: integer'))
    # Sixty-four independent violations exercise a broad failing value. The
    # current evaluator reports its first problem only, never a fabricated list.
    add('explain-many', {'type': 'array', 'items': {'type': 'integer'}},
        compact(['bad'] * 64), 'fails', modes=('explain',))['expected'].update(
            rows['explain-one']['expected'])
    return rows
