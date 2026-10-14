"""Independent stdlib validator for the draft collaboration profile corpus."""
import json
import re
from pathlib import Path


def fields(v, names):
    assert isinstance(v, dict) and list(v) == names.split()


def hx(v, n):
    assert isinstance(v, str) and len(v) == n and re.fullmatch('[0-9a-f]*', v)


def count(v):
    assert isinstance(v, str) and re.fullmatch('0|[1-9][0-9]*', v)
    assert int(v) <= 2**64-1
    return int(v)


def native(v):
    assert isinstance(v, str) and v.startswith('k1:b3:')
    hx(v[6:], 64)


def frontier(v):
    assert isinstance(v, list)
    result = {}
    last = ''
    for r in v:
        fields(r, 'replica_hex sequence operations_digest')
        hx(r['replica_hex'], 32)
        assert r['replica_hex'] > last
        last = r['replica_hex']
        assert isinstance(r['operations_digest'], str) and r['operations_digest'].startswith('b3:')
        hx(r['operations_digest'][3:], 64)
        result[last] = count(r['sequence'])
    return result


def pairs(items):
    result = {}
    for k, v in items:
        assert k not in result
        result[k] = v
    return result


def validate(text):
    assert len(text.encode()) <= 1024*1024
    p = json.loads(text, object_pairs_hook=pairs)
    assert json.dumps(p, ensure_ascii=False, separators=(',', ':')) == text
    fields(p, 'schema_version workspace_hex epoch_hex generation tree protocol_hex namespace entries')
    assert type(p['schema_version']) is int and p['schema_version'] == 1
    hx(p['workspace_hex'], 32)
    hx(p['epoch_hex'], 32)
    count(p['generation'])
    native(p['tree'])
    assert len(p['protocol_hex']) > 0 and len(p['protocol_hex']) % 2 == 0
    hx(p['protocol_hex'], len(p['protocol_hex']))
    ns = frontier(p['namespace'])
    assert ns
    assert isinstance(p['entries'], list)
    last = ''
    for e in p['entries']:
        fields(e, 'path_hex kind target frontier')
        path = e['path_hex']
        assert path > last and len(path) % 2 == 0
        hx(path, len(path))
        raw = bytes.fromhex(path)
        assert b'\0' not in raw and all(c not in (b'', b'.', b'..') for c in raw.split(b'/'))
        last = path
        assert type(e['kind']) is int
        if e['kind'] in (1, 2, 3, 4):
            native(e['target'])
        else:
            assert e['kind'] == 5
            assert isinstance(e['target'], str)
            algorithm, digest = e['target'].split(':')
            assert algorithm in ('sha1', 'sha256')
            hx(digest, 40 if algorithm == 'sha1' else 64)
        for replica, sequence in frontier(e['frontier']).items():
            assert replica in ns and sequence <= ns[replica]


if __name__ == '__main__':
    path = Path(__file__).resolve().parents[2] / 'tests/fixtures/collaboration-frontier-cases.json'
    cases = json.loads(path.read_text())
    for c in cases:
        try:
            validate(c['json'])
            valid = True
        except (AssertionError, ValueError, TypeError, KeyError):
            valid = False
        assert valid == c['valid'], c['name']
    print(f'{len(cases)} collaboration frontier cases agree')
