"""Independent receipt checker against the fixed valid update-refs fixture.
Request hashing and intent validation are covered separately, not reimplemented here.
"""
import json
from pathlib import Path
from check import fields, integer, require, oid, counter
from transaction_status import parse


def durability(v):
    fields(v, ['class'], ['replication_policy'])
    require(v['class'] in ['single-node-durable', 'replicated-durable'])
    require((v['class'] == 'replicated-durable') == (v.get('replication_policy') is not None))
    if v.get('replication_policy') is not None:
        oid(v['replication_policy'])
    return v['class'], v.get('replication_policy')


def receipt(v, request, expected):
    fields(v, 'schema_version transaction_id repository_id request_digest repository_generation operation_id policy_outcome_id durability refs'.split())
    integer(v['schema_version'], 1, 1)
    intent = request['intent']
    for key in ['transaction_id', 'repository_id', 'operation_id']:
        require(v[key] == intent[key])
    require(v['request_digest'] == request['request_digest'])
    oid(v['policy_outcome_id'])
    require(durability(v['durability']) == durability(expected))
    counter(v['repository_generation'])
    require(int(v['repository_generation']) == int(intent['expected_namespace_generation']) + 1)
    require(isinstance(v['refs'], list) and len(v['refs']) == len(intent['updates']))
    for actual, update in zip(v['refs'], intent['updates']):
        fields(actual, 'name old_generation new_generation old_targets new_targets'.split())
        require(actual['name'] == update['name'])
        require(actual['old_generation'] == update['expected_generation'])
        require(actual['old_targets'] == update['expected_old_targets'])
        require(actual['new_targets'] == update['new_targets'])
        counter(actual['new_generation'])
        require(int(actual['new_generation']) == int(actual['old_generation']) + 1)


if __name__ == '__main__':
    root = Path(__file__).resolve().parents[2] / 'tests/fixtures'
    request = json.loads((root / 'update-refs.json').read_text())[0]
    rows = json.loads((root / 'publication-receipt.json').read_text())
    for row in rows:
        try:
            receipt(parse(row['receipt']), request, row['expected_durability'])
            accepted = True
        except (ValueError, TypeError, KeyError, UnicodeError, RecursionError):
            accepted = False
        assert accepted == row['accepted'], row['name']
    print(f'{len(rows)} publication receipt vectors agree with independent Python checking.')
