"""Independent v2 envelope checks using the fixed valid publication request."""
import json
from pathlib import Path
from check import fields, integer, require
from transaction_status import parse, request as lookup_request, reply as status_reply
from publication_receipt import durability, receipt


def query(v, original):
    fields(v, ['schema_version', 'lookup'])
    integer(v['schema_version'], 2, 2)
    lookup_request(v['lookup'])
    for key in ['repository_id', 'transaction_id']:
        require(v['lookup'][key] == original['intent'][key])
    require(v['lookup']['request_digest'] == original['request_digest'])


def reply(v, q, original, expected):
    query(q, original)
    durability(expected)
    fields(v, ['schema_version', 'status'], ['receipt'])
    integer(v['schema_version'], 2, 2)
    status_reply(v['status'], q['lookup'])
    result = v['status'].get('result')
    committed = False
    if result is not None:
        require(result['refs'] == [])
        require(result.get('operation_id') in [None, original['intent']['operation_id']])
        committed = result['outcome'] in ['committed', 'committed_materialization_pending']
        if not committed:
            require(result.get('generation') is None)
    if committed:
        require(v.get('receipt') is not None)
        receipt(v['receipt'], original, expected)
        require(result['generation'] == v['receipt']['repository_generation'])
    else:
        require(v.get('receipt') is None)


if __name__ == '__main__':
    root = Path(__file__).resolve().parents[2] / 'tests/fixtures'
    original = json.loads((root / 'update-refs.json').read_text())[0]
    rows = json.loads((root / 'receipt-status.json').read_text())
    for row in rows:
        try:
            q = parse(row['query']); query(q, original)
            valid_query = True
        except (ValueError, TypeError, KeyError, UnicodeError, RecursionError):
            valid_query = False
        assert valid_query == row['query_accepted'], f"query: {row['name']}"
        try:
            require(valid_query)
            reply(parse(row['reply']), q, original, {'class': 'single-node-durable'})
            accepted = True
        except (ValueError, TypeError, KeyError, UnicodeError, RecursionError):
            accepted = False
        assert accepted == row['accepted'], row['name']
    print(f'{len(rows)} receipt-status vectors agree with independent Python checking.')
