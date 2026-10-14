"""Independent stdlib-only checker for the published JSON contract corpus.
Does not load the Rust implementation. This is not a command executor.
"""
import json
import re
import unicodedata
from pathlib import Path

def require(ok):
    if not ok:
        raise ValueError('invalid contract')

def unique_object(pairs):
    obj = {}
    for key, value in pairs:
        require(key not in obj)
        obj[key] = value
    return obj

def integer(v, low, high):
    require(type(v) is int and low <= v <= high)

def text(v):
    require(isinstance(v, str))

def oid(v):
    require(isinstance(v, str) and re.fullmatch(r'k1:b3:[0-9a-f]{64}', v))

def counter(v):
    require(isinstance(v, str) and re.fullmatch(r'0|[1-9][0-9]*', v) and int(v) <= 2**64-1)

def fields(v, required, optional=()):
    require(isinstance(v, dict) and set(required) <= v.keys() and v.keys() <= set(required) | set(optional))

def labels(v):
    require(isinstance(v, list) and all(isinstance(s, str) and re.fullmatch(r'[a-z0-9_./-]+', s) for s in v))
    require(v == sorted(set(v)))

def capabilities(v):
    fields(v, 'schema_version protocol_versions object_formats supported_features required_features max_object_bytes max_frame_bytes compression signature_formats ref_transactions partial shallow durability'.split(), ['replication_policy'])
    integer(v['schema_version'], 1, 1)
    ps = v['protocol_versions']; require(isinstance(ps, list) and ps)
    for p in ps: integer(p, 1, 65535)
    require(ps == sorted(set(ps)))
    for key in ['object_formats', 'supported_features', 'required_features', 'compression', 'signature_formats']: labels(v[key])
    require(v['object_formats'] and v['compression'] and set(v['required_features']) <= set(v['supported_features']))
    integer(v['max_object_bytes'], 1, 2**32-1); integer(v['max_frame_bytes'], v['max_object_bytes'], 2**32-1)
    for key in ['ref_transactions', 'partial', 'shallow']: require(type(v[key]) is bool)
    require(v['durability'] in ['none', 'single-node-durable', 'replicated-durable'])
    require((v['durability'] == 'replicated-durable') == (v.get('replication_policy') is not None))
    if v.get('replication_policy') is not None: oid(v['replication_policy'])

def result(v):
    fields(v, 'schema_version request_id outcome exit_code refs recovery external_receipts interrupted'.split(), ['operation_id', 'generation'])
    integer(v['schema_version'], 1, 1); integer(v['exit_code'], 0, 10)
    require(isinstance(v['request_id'], str) and re.fullmatch('[0-9a-f]{32}', v['request_id']))
    op = v.get('operation_id') is not None; gen = v.get('generation') is not None
    if op: require(isinstance(v['operation_id'], str) and re.fullmatch('[0-9a-f]{32}', v['operation_id']))
    if gen: counter(v['generation'])
    require(type(v['interrupted']) is bool)
    for key in ['refs', 'recovery', 'external_receipts']: require(isinstance(v[key], list))
    require(gen or not v['refs']); last = None
    for r in v['refs']:
        fields(r, ['name', 'generation'], ['target']); name = r['name']; text(name)
        parts = name.split('/')
        require(unicodedata.normalize('NFC', name) == name and not any(unicodedata.category(c) == 'Cc' or c == '\\' for c in name))
        require(all(p not in ['', '.', '..'] for p in parts))
        require((len(parts) >= 2 and parts[0] in ['branches', 'tags', 'shelves']) or (len(parts) >= 4 and parts[0] == 'remotes' and parts[2] == 'branches'))
        require(last is None or last.encode() < name.encode()); last = name
        counter(r['generation']); require(int(r['generation']) > 0)
        if r.get('target') is not None: oid(r['target'])
    require(all(isinstance(s, str) and s.strip() for s in v['recovery']))
    seen = set(); all_complete = True
    for r in v['external_receipts']:
        fields(r, ['system', 'receipt_id', 'state'])
        require(all(isinstance(r[k], str) and r[k].strip() for k in ['system', 'receipt_id']))
        require(r['state'] in ['complete', 'pending', 'failed', 'unknown'])
        pair = (r['system'], r['receipt_id']); require(pair not in seen); seen.add(pair)
        all_complete &= r['state'] == 'complete'
    code = v['exit_code']; failure = code in [1,2,4,5,6,7,8]; recovery = bool(v['recovery']); receipts = bool(seen)
    valid = {
        'not_started': failure and not op and not gen and not receipts,
        'prepared': code == 1 and op and not gen and recovery and not receipts,
        'committed': code == 0 and op and gen and all_complete,
        'committed_materialization_pending': code == 9 and op and gen and recovery,
        'conflicted': code == 3 and op and recovery and all_complete,
        'rejected': failure and not gen and not receipts,
        'indeterminate': code == 10 and op and not gen and recovery,
    }
    require(valid.get(v['outcome'], False))

if __name__ == '__main__':
    path = Path(__file__).resolve().parents[2] / 'tests/fixtures/interface.json'
    rows = json.loads(path.read_text())
    for row in rows:
        try:
            require(len(row['json'].encode()) <= 1024*1024)
            value = json.loads(row['json'], object_pairs_hook=unique_object)
            (capabilities if row['kind'] == 'capabilities' else result)(value)
            accepted = True
        except (ValueError, TypeError, KeyError, UnicodeError):
            accepted = False
        assert accepted == row['accepted'], row['name']
    print(f'{len(rows)} interface vectors agree with the independent Python checker.')
