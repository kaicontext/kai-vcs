"""Build artifacts for a separate recovery runner. No third-party Python modules."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('destination', type=Path)
    parser.add_argument('--toolchain', default='1.99.0')
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    dest = args.destination.resolve()
    dest.mkdir(parents=True, exist_ok=False)
    cargo = ['cargo', '+' + args.toolchain]

    def run(*rest):
        result = subprocess.run(cargo + list(rest), cwd=repo,
                                capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return result.stdout.strip()

    run('build', '--locked', '-p', 'kai-object-reference', '--bin', 'kai-bundle-reader')
    exe = 'kai-bundle-reader' + ('.exe' if os.name == 'nt' else '')
    shutil.copy2(repo / 'target/debug' / exe, dest / exe)
    source_root = run('run', '--locked', '--example', 'source_bundle', '--', str(dest / 'source.kai-bundle'))
    repository = run('run', '--locked', '--example', 'repository_bundle', '--', str(dest / 'repository.kai-bundle'))
    run('run', '--locked', '--example', 'recovery_cases', '--', str(dest))
    expected = {'source_root': source_root,
                'repository_ids': dict(line.split(': ', 1) for line in repository.splitlines())}
    (dest / 'expected.json').write_text(json.dumps(expected, indent=2) + '\n')
    shutil.copy2(Path(__file__).with_name('recover.py'), dest / 'recover.py')
    # Transport integrity only; semantic expected results are checked independently.
    hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in dest.iterdir() if p.is_file()}
    (dest / 'sha256.json').write_text(json.dumps(hashes, indent=2) + '\n')
    print(f'Prepared {len(hashes)} files for independent recovery: {dest}')


if __name__ == '__main__':
    main()
