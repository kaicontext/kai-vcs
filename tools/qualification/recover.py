"""Standalone recovery drill. Run with only downloaded artifacts, no checkout."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile


def require(value, message):
    if not value:
        raise RuntimeError(message)


def frames(data):
    require(data[:13] == b'KAI-BUNDLE\0\0\x01', 'invalid fixture magic')
    pos, result = 13, []
    while pos < len(data):
        require(pos + 8 <= len(data), 'fixture length truncated')
        n = struct.unpack('>Q', data[pos:pos + 8])[0]
        pos += 8
        require(pos + n <= len(data), 'fixture frame truncated')
        result.append(data[pos:pos + n])
        pos += n
    return result


def drill(report):
    def begin(stage, case=None):
        report['stage'] = stage
        report.pop('command', None)
        report.pop('case', None)
        if case is not None:
            report['case'] = case

    begin('artifact-verification')
    artifacts = Path(__file__).resolve().parent
    require(not (artifacts / 'Cargo.toml').exists(), 'run without a source checkout')
    hashes = json.loads((artifacts / 'sha256.json').read_text())
    for name, expected in hashes.items():
        report['case'] = name
        require(hashlib.sha256((artifacts / name).read_bytes()).hexdigest() == expected,
                f'artifact checksum mismatch: {name}')
    begin('fixture-metadata')
    expected = json.loads((artifacts / 'expected.json').read_text())
    roots = json.loads((artifacts / 'case-roots.json').read_text())
    begin('reader-setup')
    executable = artifacts / ('kai-bundle-reader.exe' if os.name == 'nt' else 'kai-bundle-reader')
    if os.name != 'nt':
        executable.chmod(0o755)  # Artifact transport does not preserve executable mode.
    work = Path(tempfile.mkdtemp(prefix='kai-qualification-'))
    report['work_directory'] = str(work)
    env = {k: os.environ[k] for k in ('SystemRoot', 'WINDIR', 'COMSPEC') if k in os.environ}
    env.update(HOME=str(work), USERPROFILE=str(work), PATH='', LC_ALL='C', TMP=str(work), TEMP=str(work))
    report['reader_environment'] = 'empty PATH; isolated HOME; no Cargo/Git/helper access'

    def run(*args, success=True):
        command = [str(executable)] + list(map(str, args))
        report['command'] = command
        result = subprocess.run(command, cwd=work, env=env, capture_output=True,
                                text=True, timeout=60)
        require((result.returncode == 0) == success,
                f'unexpected exit {result.returncode}: {command}\n{result.stderr}')
        if not success:
            require(not result.stdout, 'failed operation reported success output')
        return result

    tar = shutil.which('tar')
    require(tar is not None, 'a capable system tar is required')

    def extract(archive, destination):
        destination.mkdir()
        report['command'] = [tar, '-xpf', str(archive), '-C', str(destination)]
        result = subprocess.run(report['command'],
                                env=env, cwd=work, capture_output=True, text=True, timeout=60)
        require(result.returncode == 0, 'system tar restoration failed: ' + result.stderr)

    def pass_case(name):
        report['passed'].append(name)
        print('PASS:', name, flush=True)

    for name in ('repository', 'source'):
        begin('repository-recovery', name)
        original = (artifacts / f'{name}.kai-bundle').read_bytes()
        copied = work / f'{name}.kai-bundle'
        copied.write_bytes(original)
        index = run(copied, '--index').stdout
        archive = work / f'{name}-recovery.tar'
        run(copied, '--recover', archive)
        copied.unlink()
        restored = work / f'{name}-recovered'
        extract(archive, restored)
        require((restored / 'repository.kai-bundle').read_bytes() == original, 'bundle identity changed')
        require((restored / 'manifest.object').read_bytes() == frames(original)[0], 'manifest changed')
        objects = [p.read_bytes() for p in (restored / 'objects').iterdir()]
        require(sorted(objects) == sorted(frames(original)[1:]), 'missing/changed/extra recovered object')
        require(run(restored / 'repository.kai-bundle', '--index').stdout == index, 'index changed after recovery')
        if name == 'repository':
            for text in ('state=0 targets=deleted', 'state=2 targets=', 'working-conflict', 'state 0 absent', 'included=false'):
                require(text in index, 'repository state missing: ' + text)
            snapshot = work / 'resolved.tar'
            run(restored / 'repository.kai-bundle', '--archive', expected['repository_ids']['Resolved revision'], snapshot)
            extract(snapshot, work / 'resolved')
            require((work / 'resolved/file').read_bytes() == b'saved source\n', 'resolved source changed')
            for label in ('Unresolved revision', 'Checkpoint'):
                bad = work / 'unresolved.tar'
                run(restored / 'repository.kai-bundle', '--archive', expected['repository_ids'][label], bad, success=False)
                require(not bad.exists(), 'unresolved source produced output')
        pass_case(name + ': exact objects, manifest, bundle and index restored')

    begin('source-materialization', 'source')
    source = artifacts / 'source.kai-bundle'
    source_tar = work / 'source-snapshot.tar'
    run(source, '--archive', expected['source_root'], source_tar)
    with tarfile.open(source_tar, encoding='utf-8', errors='surrogateescape') as archive:
        require(archive.getmember('run.sh').mode == 0o755, 'executable archive mode changed')
        link = archive.getmember('hello-link')
        require(link.issym() and link.linkname == 'hello.txt', 'symlink metadata changed')
        require(archive.extractfile('data/bytes.bin').read() == bytes([0, 255, 13, 10]), 'binary content changed')
    if os.name == 'posix':
        extract(source_tar, work / 'source-restored')
        require(stat.S_IMODE((work / 'source-restored/run.sh').stat().st_mode) == 0o755, 'executable mode not restored')
        require(os.readlink(work / 'source-restored/hello-link') == 'hello.txt', 'link target not restored')
        pass_case('POSIX executable and symlink materialization')
    else:
        report['limitations'].append('Windows: executable/symlink archive metadata checked; native materialization not qualified.')
    pass_case('executable, symlink and binary archive fidelity')

    names = {'portable': ['space name.txt', 'n' * 120, 'snowman-\u2603.bin'],
             'raw-name': ['\udcff'], 'control-name': ['line\nbreak', 'tab\tname']}
    for case, expected_names in names.items():
        begin('path-materialization', case)
        output = work / (case + '.tar')
        run(artifacts / (case + '.kai-bundle'), '--archive', roots[case], output)
        with tarfile.open(output, encoding='utf-8', errors='surrogateescape') as archive:
            require(sorted(archive.getnames()) == sorted(expected_names), 'unusual names changed: ' + case)
            for member in archive:
                require(archive.extractfile(member).read() == bytes([0, 255, 13, 10]), 'unusual file bytes changed')
        materialize = case == 'portable' or (case == 'control-name' and os.name == 'posix') or (case == 'raw-name' and sys.platform.startswith('linux'))
        if materialize:
            destination = work / (case + '-restored')
            extract(output, destination)
            for name in expected_names:
                require((destination / name).read_bytes() == bytes([0, 255, 13, 10]), 'materialized filename/bytes changed')
        else:
            report['limitations'].append(f'{case}: archive bytes verified; filesystem materialization not qualified on {sys.platform}.')
        pass_case(case + ': exact archive names/bytes' + (' and materialization' if materialize else ''))

    for case in ('reserved', 'metadata-alias', 'case-collision', 'normalization-collision'):
        begin('unsafe-path-rejection', case)
        bundle = artifacts / (case + '.kai-bundle')
        run(bundle)
        output = work / (case + '-refused.tar')
        run(bundle, '--archive', roots[case], output, success=False)
        require(not output.exists(), 'unsafe path preflight left output')
        pass_case(case + ': source preflight refused before output')
    for case in ('missing', 'incremental'):
        begin('incomplete-bundle-rejection', case)
        output = work / (case + '-refused.tar')
        run(artifacts / (case + '.kai-bundle'), '--recover', output, success=False)
        require(not output.exists(), 'incomplete bundle left output')
        pass_case(case + ': incomplete recovery refused')
    original = source.read_bytes()
    mutations = {'corrupt': original[:-1] + bytes([original[-1] ^ 1]),
                 'truncated-header': original[:12], 'truncated-object': original[:-1],
                 'missing-last-frame': original[:-(len(frames(original)[-1]) + 8)],
                 'trailing-data': original + b'x'}
    for name, data in mutations.items():
        begin('corruption-rejection', name)
        bundle = work / (name + '.kai-bundle')
        bundle.write_bytes(data)
        output = work / (name + '-refused.tar')
        run(bundle, '--recover', output, success=False)
        require(not output.exists(), 'invalid bundle left output')
        pass_case(name + ': recovery refused before output')
    begin('overwrite-refusal', 'partial-output')
    partial = work / 'partial.tar'
    partial.write_bytes(b'partial prior interrupted export')
    run(source, '--recover', partial, success=False)
    require(partial.read_bytes() == b'partial prior interrupted export', 'partial export overwritten')
    pass_case('existing partial export preserved; no implicit overwrite/resume')
    report['limitations'].append('Injected Rust writer failures cover short/zero/broken-pipe/flush failures; process-kill/power-loss recovery is not certified.')
    report.update(status='passed', stage='complete')
    report.pop('case', None)
    report.pop('command', None)


def main():
    report = {'status': 'failed', 'platform': platform.platform(), 'python': sys.version,
              'passed': [], 'limitations': [], 'stage': 'initialization'}
    try:
        drill(report)
    except Exception as error:
        report['error'] = {'type': type(error).__name__, 'message': str(error)}
        print('FAIL:', error, file=sys.stderr)
    finally:
        destination = Path(__file__).resolve().parent / 'recovery-report.json'
        destination.write_text(json.dumps(report, indent=2) + '\n')
        print('Report:', destination)
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
