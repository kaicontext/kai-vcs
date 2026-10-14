#!/usr/bin/env python3
"""Kill real archive writer processes at every observed boundary, then restart.
Process-crash qualification only: File.flush is not fsync or power-loss evidence.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import queue
import subprocess
import tempfile
import threading
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_child(child, bundle, output, cut=None):
    process = subprocess.Popen([str(child), str(bundle), str(output)],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, bufsize=1)
    lines = queue.Queue()
    def pump():
        for line in process.stdout:
            lines.put(line.strip())
        lines.put(None)
    threading.Thread(target=pump, daemon=True).start()
    events, acknowledged = [], False
    try:
        while True:
            line = lines.get(timeout=30)
            if line is None:
                break
            if line == 'DONE':
                acknowledged = True
                continue
            point, count = line.split()
            event = {'point': point, 'completed_bytes': int(count)}
            events.append(event)
            if len(events) - 1 == cut:
                process.kill()
                assert process.wait(timeout=30) != 0
                return events, acknowledged
            process.stdin.write('g')
            process.stdin.flush()
        assert process.wait(timeout=30) == 0, process.stderr.read()
        assert cut is None and acknowledged
        return events, acknowledged
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)
        for stream in [process.stdin, process.stdout, process.stderr]:
            stream.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--child', required=True, type=Path)
    parser.add_argument('--reader', required=True, type=Path)
    parser.add_argument('--bundle', required=True, type=Path)
    parser.add_argument('--report', required=True, type=Path)
    args = parser.parse_args()
    child, reader, bundle = [p.resolve() for p in (args.child, args.reader, args.bundle)]
    started = time.time()
    source_hash = digest(bundle)
    results = []
    with tempfile.TemporaryDirectory(prefix='kai-process-faults-') as directory:
        root = Path(directory)
        baseline = root / 'baseline.tar'
        trace, _ = run_child(child, bundle, baseline)
        complete = baseline.read_bytes()
        for index, boundary in enumerate(trace):
            partial = root / f'cut-{index}.tar'
            observed, acknowledged = run_child(child, bundle, partial, index)
            assert observed == trace[:index + 1]
            assert partial.read_bytes() == complete[:boundary['completed_bytes']]
            assert acknowledged == (boundary['point'] == 'after-response')
            # A fresh reader must refuse to overwrite crash leftovers.
            before = digest(partial)
            refused = subprocess.run([str(reader), str(bundle), '--recover', str(partial)],
                                     capture_output=True, timeout=30)
            assert refused.returncode != 0 and digest(partial) == before
            fresh = root / f'restarted-{index}.tar'
            restarted = subprocess.run([str(reader), str(bundle), '--recover', str(fresh)],
                                       capture_output=True, timeout=30)
            assert restarted.returncode == 0, restarted.stderr
            assert fresh.read_bytes() == complete
            assert digest(bundle) == source_hash
            results.append(dict(boundary, index=index, acknowledged=acknowledged,
                                prefix_preserved=True, overwrite_refused=True,
                                restart_recovery_equal=True, input_unchanged=True))
            partial.unlink()
            fresh.unlink()
    report = {'schema_version': 1, 'scope': 'process termination, not power loss or P1 root transactions',
              'platform': platform.platform(), 'seed': str(0x4b41492d50302d31),
              'commit': os.environ.get('GITHUB_SHA', subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()),
              'bundle_sha256': source_hash, 'child_sha256': digest(child), 'reader_sha256': digest(reader),
              'elapsed_seconds': time.time() - started, 'cuts': results}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(f"Passed {len(results)} process-kill/restart boundaries; report: {args.report}")


if __name__ == '__main__':
    main()
