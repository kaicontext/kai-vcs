"""Independent negotiation exchange checker. No Rust implementation is imported."""
import json
import re
from pathlib import Path
from check import capabilities, fields, integer, labels, oid, require, unique_object

CLASSES = ['none', 'single-node-durable', 'replicated-durable']

def parse(raw):
    require(len(raw.encode('utf-8')) <= 1024 * 1024)
    return json.loads(raw, object_pairs_hook=unique_object)

def request(v):
    fields(v, ['schema_version', 'request_id', 'repository_id', 'policy'])
    integer(v['schema_version'], 1, 1)
    for key in ['request_id', 'repository_id']:
        require(isinstance(v[key], str) and re.fullmatch('[0-9a-f]{32}', v[key]))
    p = v['policy']
    fields(p, ['selection', 'require_ref_transactions', 'required_signatures', 'minimum_durability'], ['replication_policy'])
    require(p['selection'] in ['complete', 'partial', 'shallow'])
    require(type(p['require_ref_transactions']) is bool)
    labels(p['required_signatures'])
    require(p['minimum_durability'] in CLASSES)
    require((p['minimum_durability'] == 'replicated-durable') == (p.get('replication_policy') is not None))
    if p.get('replication_policy') is not None:
        oid(p['replication_policy'])

def reply(req, sender, receiver):
    request(req)
    capabilities(sender)
    capabilities(receiver)
    p = req['policy']
    for a, b in [(sender, receiver), (receiver, sender)]:
        require(set(a['required_features']) <= set(b['supported_features']))
    def common(key):
        return sorted(set(sender[key]) & set(receiver[key]))
    protocols = common('protocol_versions')
    formats = common('object_formats')
    compression = common('compression')
    require(protocols and formats and compression)
    signatures = common('signature_formats')
    require(set(p['required_signatures']) <= set(signatures))
    if p['selection'] != 'complete':
        require(sender[p['selection']] and receiver[p['selection']])
    if p['require_ref_transactions']:
        require(sender['ref_transactions'] and receiver['ref_transactions'])
    require(CLASSES.index(receiver['durability']) >= CLASSES.index(p['minimum_durability']))
    if p['minimum_durability'] == 'replicated-durable':
        require(p['replication_policy'] == receiver.get('replication_policy'))
    n = dict(protocol_version=protocols[-1], object_format=formats[0], compression=compression[0],
             features=common('supported_features'), signature_formats=signatures,
             max_object_bytes=min(sender['max_object_bytes'], receiver['max_object_bytes']),
             max_frame_bytes=min(sender['max_frame_bytes'], receiver['max_frame_bytes']),
             selection=p['selection'], ref_transactions=sender['ref_transactions'] and receiver['ref_transactions'],
             receiver_durability=receiver['durability'])
    if receiver.get('replication_policy') is not None:
        n['replication_policy'] = receiver['replication_policy']
    return dict(schema_version=1, request_id=req['request_id'], repository_id=req['repository_id'], negotiated=n)

def same(a, b):
    if type(a) is not type(b):
        return False
    if isinstance(a, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    if isinstance(a, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    return a == b

def verify(raw, req, sender, receiver):
    value = parse(raw)
    fields(value, ['schema_version', 'request_id', 'repository_id', 'negotiated'])
    n = value['negotiated']
    require(isinstance(n, dict))
    if n.get('replication_policy') is None:
        n.pop('replication_policy', None)
    require(same(value, reply(req, sender, receiver)))

if __name__ == '__main__':
    rows = json.loads((Path(__file__).resolve().parents[2] / 'tests/fixtures/negotiation.json').read_text())
    for row in rows:
        try:
            req = parse(row['request'])
            expected = reply(req, row['sender'], row['receiver'])
            if 'reply' in row:
                verify(row['reply'], req, row['sender'], row['receiver'])
            accepted = True
        except (ValueError, TypeError, KeyError, UnicodeError, RecursionError):
            accepted = False
        assert accepted == row['accepted'], row['name']
        if accepted:
            assert same(expected, row['expected']), row['name']
    print(f'{len(rows)} negotiation vectors agree with the independent Python checker.')
