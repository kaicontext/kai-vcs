"""Independent list-refs request/reply validation; no repository access."""
import json
import re
import unicodedata
from pathlib import Path
from check import fields, integer, counter, oid, require, unique_object

NAMESPACES = ['branches', 'tags', 'shelves', 'remotes']
def parse(raw):
    require(len(raw.encode()) <= 1024 * 1024)
    return json.loads(raw, object_pairs_hook=unique_object)
def name(value, namespace):
    require(isinstance(value, str))
    # json.loads can preserve unpaired surrogate escapes; native names must
    # encode as UTF-8 even when no ordering comparison will run.
    value.encode("utf-8")
    require(unicodedata.normalize('NFC', value) == value)
    require(not any(unicodedata.category(c) == 'Cc' or c == '\\' for c in value))
    parts = value.split('/')
    require(all(p not in ['', '.', '..'] for p in parts))
    require((len(parts) >= 2 and parts[0] in ['branches', 'tags', 'shelves']) or
            (len(parts) >= 4 and parts[0] == 'remotes' and parts[2] == 'branches'))
    require(parts[0] == namespace)
def request(v):
    fields(v, ['schema_version', 'request_id', 'repository_id', 'namespace', 'limit'], ['after', 'generation'])
    integer(v['schema_version'], 1, 1)
    for key in ['request_id', 'repository_id']:
        require(isinstance(v[key], str) and re.fullmatch('[0-9a-f]{32}', v[key]))
    require(v['namespace'] in NAMESPACES)
    integer(v['limit'], 1, 1000)
    if v.get('generation') is not None:
        counter(v['generation'])
    if v.get('after') is not None:
        name(v['after'], v['namespace'])
        require(v.get('generation') is not None)
def reply(v, req):
    request(req)
    fields(v, ['schema_version', 'request_id', 'repository_id', 'namespace', 'generation', 'refs', 'has_more'], ['after'])
    integer(v['schema_version'], 1, 1)
    for key in ['request_id', 'repository_id', 'namespace']:
        require(v[key] == req[key])
    require(v.get('after') == req.get('after'))
    counter(v['generation'])
    require(req.get('generation') is None or req['generation'] == v['generation'])
    require(type(v['has_more']) is bool)
    require(isinstance(v['refs'], list) and len(v['refs']) <= req['limit'])
    require(not v['has_more'] or v['refs'])
    previous = req.get('after')
    for r in v['refs']:
        fields(r, ['name', 'generation', 'targets'])
        name(r['name'], req['namespace'])
        require(previous is None or previous.encode() < r['name'].encode())
        previous = r['name']
        counter(r['generation']); require(int(r['generation']) > 0)
        targets = r['targets']; require(isinstance(targets, list))
        require(len(targets) <= 1 or req['namespace'] == 'branches')
        for target in targets:
            oid(target)
        require(targets == sorted(set(targets)))

if __name__ == '__main__':
    rows = json.loads((Path(__file__).resolve().parents[2] / 'tests/fixtures/list-refs.json').read_text())
    for row in rows:
        try:
            req = parse(row['request']); request(req)
            if 'reply' in row:
                reply(parse(row['reply']), req)
            accepted = True
        except (ValueError, TypeError, KeyError, UnicodeError, RecursionError):
            accepted = False
        assert accepted == row['accepted'], row['name']
    print(f'{len(rows)} list-refs vectors agree with the independent Python checker.')
