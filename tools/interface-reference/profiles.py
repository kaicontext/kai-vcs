"""Independent canonical evidence-profile validator; no Rust dependency."""
import sys
sys.dont_write_bytecode = True
import json
import re
from pathlib import Path
from check import require, unique_object, oid, integer

def fields(v, names, optional=()):
    require(isinstance(v, dict) and list(v) == [n for n in names if n not in optional or n in v])

def text(v):
    require(isinstance(v, str) and v.strip())

def digest(v, prefix):
    require(isinstance(v, str) and re.fullmatch(re.escape(prefix)+r'[0-9a-f]{64}', v))

def sorted_names(v):
    require(isinstance(v, list))
    names = [item['name'] for item in v]
    for n in names: text(n)
    require(names == sorted(set(names)))

def validate(kind, raw):
    require(len(raw.encode()) <= 1024*1024)
    v = json.loads(raw, object_pairs_hook=unique_object)
    require(json.dumps(v, ensure_ascii=False, separators=(',', ':')) == raw)
    if kind == 'inputs':
        fields(v, ['schema_version','revision','tree','dirty_overlay_digest','dependencies','submodules','review'], ['dirty_overlay_digest','review'])
        integer(v['schema_version'],1,1); oid(v['revision']); oid(v['tree'])
        if 'dirty_overlay_digest' in v: digest(v['dirty_overlay_digest'],'b3:')
        sorted_names(v['dependencies'])
        for d in v['dependencies']: fields(d,['name','digest']); digest(d['digest'],'b3:')
        require(isinstance(v['submodules'],list)); last = None
        for s in v['submodules']:
            fields(s,['path_hex','algorithm','object_id'])
            require(isinstance(s['path_hex'],str) and re.fullmatch(r'(?:[0-9a-f]{2})+',s['path_hex']))
            path = bytes.fromhex(s['path_hex'])
            require(all(p not in [b'',b'.',b'..'] and b'\0' not in p for p in path.split(b'/')))
            require(last is None or last < path); last = path
            require(s['algorithm'] in ['sha1','sha256'])
            require(isinstance(s['object_id'],str) and re.fullmatch('[0-9a-f]{'+str(40 if s['algorithm']=='sha1' else 64)+'}',s['object_id']))
        if 'review' in v:
            r = v['review']; fields(r,['identifier','target_revision','policy','base_revision'],['base_revision']); text(r['identifier']); oid(r['target_revision']); oid(r['policy'])
            if 'base_revision' in r: oid(r['base_revision'])
    else:
        fields(v,['schema_version','platform','container_digest','toolchains','variables','limitations'],['container_digest'])
        integer(v['schema_version'],1,1); text(v['platform'])
        if 'container_digest' in v: digest(v['container_digest'],'sha256:')
        sorted_names(v['toolchains'])
        for t in v['toolchains']: fields(t,['name','version']); text(t['version'])
        sorted_names(v['variables'])
        for e in v['variables']:
            fields(e,['name','kind','value']); require('\0' not in e['name'] and '=' not in e['name'])
            require(e['kind'] in ['public','opaque_secret_version'] and isinstance(e['value'],str))
            if e['kind']=='opaque_secret_version': text(e['value'])
        require(isinstance(v['limitations'],list))
        for reason in v['limitations']: text(reason)
        require(v['limitations']==sorted(set(v['limitations'])))

if __name__ == '__main__':
    root = Path(__file__).resolve().parents[2]
    cases = json.loads((root/'tests/fixtures/evidence-profile-cases.json').read_text())
    for row in cases:
        try:
            validate(row['kind'],row['json']); accepted=True
        except (ValueError,TypeError,KeyError,UnicodeError):
            accepted=False
        assert accepted==row['accepted'],row['name']
    print(f'{len(cases)} profile cases agree with the independent Python checker.')
