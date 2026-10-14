"""Generate exact 1 MiB documents and preserve ordinary corpus case identities."""
from pathlib import Path
import json, sys, hashlib
SDK=Path(__file__).resolve().parents[2]
OUT=SDK/'target/document-diagnostics/fixtures'
OUT.mkdir(parents=True,exist_ok=True)
MIB=1<<20
(OUT/'inputs').mkdir(exist_ok=True)
rows=[]
def emit(id,doc,lane,elements,unit='array element',pinned=None):
    # Whitespace adds no extra JSON value or hidden error payload.
    assert len(doc.encode())<=MIB,(id,len(doc.encode()))
    doc += ' '*(MIB-len(doc.encode()))
    path=OUT/'inputs'/f'{id}.json';path.write_text(doc)
    rows.append(dict(id=id,path=str(path),lane=lane,elements=elements,element_unit=unit,pinned=pinned,sha256=hashlib.sha256(doc.encode()).hexdigest()))
def wrap(s): return '{"openbindings":"0.2.0","operations":{"op":{"input":'+s+'}}}'
def array_case(id,make,lane='meta',element='0',pinned=None):
    overhead=len(make('[]'));n=(MIB-overhead+1)//(len(element)+1)
    emit(id,make('['+','.join([element]*n)+']'),lane,n,pinned=pinned)
def string_case(id,make,lane='meta',unit='ASCII byte',token='a',pinned=None):
    overhead=len(make('""'));n=(MIB-overhead)//len(token.encode())
    emit(id,make('"'+token*n+'"'),lane,n,unit,pinned)
array_case('document-schema-anyof',lambda a:wrap(a),'document',pinned='openbindings#/$defs/JSONSchema/anyOf')
array_case('meta-type-anyof',lambda a:wrap('{"type":'+a+'}'),pinned='validation#/properties/type/anyOf')
array_case('meta-dependencies-anyof',lambda a:wrap('{"dependencies":{"a":'+a+'}}'),pinned='schema#/properties/dependencies/additionalProperties/anyOf')
# Pure fixed-type errors do not own the rejected large value.
for id,node in [('core','{"$comment":%s}'),('applicator','{"items":%s}'),('unevaluated','{"unevaluatedItems":%s}'),('metadata','{"title":%s}'),('format','{"format":%s}'),('content','{"contentSchema":%s}')]:
    array_case('control-'+id,lambda a,node=node:wrap(node%a))
# Each uniqueness shape, duplicates late enough to build the hash table.
def unique(id,make,lane='meta',pinned=None):
    values=[];size=len(make('[]'))
    for i in range(MIB):
        s=json.dumps('a'+str(i),separators=(',',':'))
        if size+len(s)+1+5>MIB:break
        values.append(s);size+=len(s)+1
    values.append('"a0"')
    emit(id,make('['+','.join(values)+']'),lane,len(values),pinned=pinned)
unique('document-aliases-unique',lambda a:'{"openbindings":"0.2.0","operations":{"op":{"aliases":'+a+'}}}','document')
unique('document-kinds-unique',lambda a:'{"openbindings":"0.2.0","operations":{},"dependencies":{"d":{"operation":"op","kinds":'+a+'}}}','document')
unique('meta-required-unique',lambda a:wrap('{"required":'+a+'}'))
unique('meta-dependent-required-unique',lambda a:wrap('{"dependentRequired":{"a":'+a+'}}'))
array_case('meta-type-unique',lambda a:wrap('{"type":'+a+'}'),element='"null"')
array_case('meta-dependencies-unique',lambda a:wrap('{"dependencies":{"a":'+a+'}}'),element='"a"')
# Long keys exercise name scratch, ownership and instance pointer construction.
string_case('document-property-name',lambda s:'{"openbindings":"0.2.0","operations":{'+s[:-1]+'!":{}}}','document')
string_case('document-additional-name',lambda s:'{"openbindings":"0.2.0","operations":{},'+s+':0}','document')
string_case('meta-property-name-buffer',lambda s:wrap('{"$vocabulary":{'+s+':0}}'))
string_case('meta-additional-path',lambda s:wrap('{"$defs":{'+s+':0}}'))
# All numeric failure families interpret exact decimal coefficients.
for field,token in [('multipleOf','-'),('minLength','-'),('maxLength','-'),('minItems','-'),('maxItems','-'),('minProperties','-'),('maxProperties','-'),('minContains','-'),('maxContains','-')]:
    make=lambda number,field=field:wrap('{"'+field+'":'+number+'}')
    n=MIB-len(make('-'))
    emit('meta-number-'+field,make('-'+'1'*n),'meta',n,'decimal digit')
for bound,negative in [('maximum',False),('minimum',True)]:
    make=lambda n:'{"openbindings":"0.2.0","operations":{},"bindings":{"b":{"operation":"op","source":"s","preference":'+n+'}}}'
    n=MIB-len(make('-' if negative else ''))
    emit('document-number-'+bound,make(('-' if negative else '')+'1'*n),'document',n,'decimal digit')
# Marker presentation is allocated by the exact adapter when strings are interpreted.
string_case('document-pattern-marker',lambda s:'{"openbindings":'+s+',"operations":{}}','document','UTF-16 unit','\ue000')
string_case('meta-pattern-marker',lambda s:wrap('{"$anchor":'+s+'}'),unit='UTF-16 unit',token='\ue000')
string_case('meta-enum-marker',lambda s:wrap('{"type":'+s+'}'),unit='UTF-16 unit',token='\ue000')
string_case('document-minlength-marker',lambda s:'{"openbindings":"0.2.0","operations":{},"version":'+s+',"description":false}','document','UTF-16 unit','\ue000')
# Remaining repeated keyword positions use the same kernels with original source paths.
for field,entry in [('schemas','true'),('operations','{}'),('dependencies','{"operation":"op"}'),('sources','{"kind":"k"}'),('bindings','{"operation":"op","source":"s"}')]:
    body='"'+('!'+'a'*99999)+'":'+entry
    doc='{"openbindings":"0.2.0","operations":{}'
    if field=='operations':doc='{"openbindings":"0.2.0"'
    emit('name-'+field,doc+',"'+field+'":{'+body+'}}','document',100000,'ASCII name byte')
name='"!'+('a'*99999)+'"'
emit('name-examples','{"openbindings":"0.2.0","operations":{"op":{"examples":{'+name+':{}}}}}','document',100000,'ASCII name byte')
for definition,make in [
    ('operation',lambda body:'{"openbindings":"0.2.0","operations":{"op":{'+body+'}}}'),
    ('example',lambda body:'{"openbindings":"0.2.0","operations":{"op":{"examples":{"e":{'+body+'}}}}}'),
    ('dependency',lambda body:'{"openbindings":"0.2.0","operations":{},"dependencies":{"d":{"operation":"op",'+body+'}}}'),
    ('source',lambda body:'{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k",'+body+'}}}'),
    ('binding',lambda body:'{"openbindings":"0.2.0","operations":{},"bindings":{"b":{"operation":"op","source":"s",'+body+'}}}')]:
    n=MIB-len(make('"":0'))
    emit('additional-'+definition,make('"'+'a'*n+'":0'),'document',n,'ASCII name byte')
for id,field,child in [('core-name-scratch','$vocabulary','true'),('applicator-name-scratch','patternProperties','{}')]:
    make=lambda key:wrap('{"'+field+'":{'+key+':'+child+'},"minLength":-1}')
    n=MIB-len(make('""'))
    emit(id,make('"'+'a'*n+'"'),'meta',n,'ASCII name byte')
for field in ['properties','patternProperties','dependentSchemas','definitions','dependentRequired']:
    make=lambda key:wrap('{"'+field+'":{'+key+':0}}')
    n=MIB-len(make('""'))
    emit('path-'+field,make('"'+'a'*n+'"'),'meta',n,'ASCII name byte')
make=lambda n:wrap('{"minLength":0.'+n+'}')
n=MIB-len(make(''))
emit('meta-integer-type',make('1'*n),'meta',n,'decimal digit')
# Shorter interpreted strings leave the rest of the 1 MiB document as whitespace.
emit('pattern-document-sized','{"openbindings":"'+('a'*100000)+'","operations":{}}','document',100000,'ASCII character')
emit('pattern-meta-sized',wrap('{"$anchor":"'+('a'*100000)+'!"}'),'meta',100001,'ASCII character')
emit('pattern-marker-sized',wrap('{"$anchor":"'+('\ue000'*100000)+'"}'),'meta',100000,'UTF-16 unit')
emit('minlength-marker-sized','{"openbindings":"0.2.0","operations":{},"dependencies":{"d":{"operation":"op","kinds":["'+('\ue000'*100000)+'",""]}}}','document',100000,'UTF-16 unit')
# Repeated paths make an unrestricted constant bytes/input-byte factor impossible.
key='a'*8192;n=4096
emit('meta-repeated-path',wrap('{"dependencies":{"'+key+'":['+','.join(['0']*n)+']}}'),'meta',n,'array element')

array_case('document-dialect-enum',lambda a:wrap('{"$schema":'+a+'}'),'document',pinned='openbindings#/$defs/JSONSchema/anyOf/0/properties/$schema/enum')
emit('core-name-marker',wrap('{"$vocabulary":{"'+('\ue000'*100000)+'":true},"minLength":-1}'),'meta',100000,'UTF-16 name unit')
emit('additional-name-marker','{"openbindings":"0.2.0","operations":{},"'+('\ue000'*100000)+'":0}','document',100000,'UTF-16 name unit')

for field in ['schemas','operations','dependencies','sources','bindings']:
    key='a'*100000
    doc='{"openbindings":"0.2.0","operations":{}'
    if field=='operations':doc='{"openbindings":"0.2.0"'
    emit('document-path-'+field,doc+',"'+field+'":{"'+key+'":0}}','document',100000,'ASCII name byte')
emit('document-path-examples','{"openbindings":"0.2.0","operations":{"op":{"examples":{"'+('a'*100000)+'":0}}}}','document',100000,'ASCII name byte')

# Retention controls exercise both the count cap and the shared pointer allowance.
for key_size in [1, 8192]:
    key='a'*key_size
    fields=','.join('"bad'+str(i)+'":0' for i in range(4097))
    emit('finding-retention-'+str(key_size),'{"openbindings":"0.2.0","operations":{"'+key+'":{'+fields+'}}}', 'document',4097,'unexpected member')

# Padding is whitespace; ordinary cases retain the corpus's exact carriage.
sys.path.insert(0,str(SDK/'tools/qualification/replay'))
import corpus
for i,case in enumerate(corpus.load()):
    if case['action']!='validate-document':continue
    raw=corpus.carriage(case['given'])
    if len(raw)>16384:continue
    try:json.loads(raw)
    except (ValueError,UnicodeDecodeError):continue
    path=OUT/'inputs'/f'corpus-{i:03}.json';path.write_bytes(raw)
    rows.append(dict(id=case['id'],path=str(path),lane='both',elements=None,element_unit=None,group='corpus',sha256=hashlib.sha256(raw).hexdigest()))
(OUT/'inputs.json').write_text(json.dumps(rows,indent=2)+'\n')
print(len(rows),'cases;',sum(r.get('group')=='corpus' for r in rows),'corpus')
