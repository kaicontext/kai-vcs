"""Independent transaction-status payload checker; never executes a mutation."""
import json
import re
from pathlib import Path
from check import fields, integer, require, result, unique_object

def parse(raw):
    require(len(raw.encode()) <= 1024 * 1024)
    value = json.loads(raw, object_pairs_hook=unique_object)
    # Match Rust strings even when a name never participates in ordering.
    def utf8(v):
        if isinstance(v, str):
            v.encode('utf-8')
        elif isinstance(v, list):
            for item in v: utf8(item)
        elif isinstance(v, dict):
            for key, item in v.items(): utf8(key); utf8(item)
    utf8(value)
    return value

def request(v):
    fields(v, ['schema_version', 'request_id', 'repository_id', 'transaction_id', 'request_digest'])
    integer(v['schema_version'], 1, 1)
    for key in ['request_id', 'repository_id', 'transaction_id']:
        require(isinstance(v[key], str) and re.fullmatch('[0-9a-f]{32}', v[key]))
    require(isinstance(v['request_digest'], str) and re.fullmatch('b3:[0-9a-f]{64}', v['request_digest']))

def reply(v, req):
    request(req)
    fields(v, ['schema_version', 'request_id', 'repository_id', 'transaction_id', 'request_digest', 'status'], ['result', 'unknown_reason'])
    integer(v['schema_version'], 1, 1)
    for key in ['request_id', 'repository_id', 'transaction_id', 'request_digest']:
        require(v[key] == req[key])
    if v['status'] == 'known':
        require(v.get('unknown_reason') is None and v.get('result') is not None)
        result(v['result'])
        require(v['result']['request_id'] == req['transaction_id'])
    elif v['status'] == 'unknown':
        require(v.get('result') is None and v.get('unknown_reason') in ['missing', 'expired'])
    elif v['status'] == 'digest_mismatch':
        require(v.get('result') is None and v.get('unknown_reason') is None)
    else:
        require(False)

if __name__ == '__main__':
    rows = json.loads((Path(__file__).resolve().parents[2] / 'tests/fixtures/transaction-status.json').read_text())
    for row in rows:
        try:
            req = parse(row['request']); request(req)
            reply(parse(row['reply']), req)
            accepted = True
        except (ValueError, TypeError, KeyError, UnicodeError, RecursionError):
            accepted = False
        assert accepted == row['accepted'], row['name']
    print(f'{len(rows)} transaction-status vectors agree with the independent Python checker.')
