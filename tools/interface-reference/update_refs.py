"""Independent digest preimage encoder. BLAKE3 is verified by the Rust tests."""
import json
import struct
from pathlib import Path


def canonical(v):
    def text(s):
        b = s.encode('utf-8')
        return struct.pack('>Q', len(b)) + b

    def strings(values):
        return struct.pack('>Q', len(values)) + b''.join(map(text, values))

    out = b'kai.update-refs.request.v1\0' + struct.pack('>H', v['schema_version'])
    for key in ('transaction_id', 'repository_id', 'expected_namespace_generation', 'operation_id', 'reason'):
        out += text(v[key])
    out += struct.pack('>Q', len(v['updates']))
    for u in v['updates']:
        out += text(u['name']) + text(u['expected_generation'])
        out += strings(u['expected_old_targets']) + strings(u['new_targets'])
    out += text(v['required_closure_manifest'])
    return out + strings(v['policy_input_ids']) + strings(v['client_capabilities'])


if __name__ == '__main__':
    rows = json.loads((Path(__file__).resolve().parents[2] / 'tests/fixtures/update-refs.json').read_text())
    for row in rows:
        assert canonical(row['intent']).hex() == row['canonical_hex'], row['name']
    print(f'{len(rows)} update-refs preimages agree with independent Python encoding.')
