"""Regression coverage for demo inputs, contract bounds and recovery diagnostics."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import recover

ROOT = Path(__file__).resolve().parents[2]


class ReviewRegressions(unittest.TestCase):
    def test_empty_recording_is_a_friendly_cli_error(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'empty.cast'
            path.write_bytes(b'')
            result = subprocess.run([sys.executable, str(ROOT / 'tools/demo/ascii_movie.py'),
                                     '--replay', str(path), '--fast'], capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            self.assertIn('Demo stopped: Empty recording', result.stderr)
            self.assertNotIn('Traceback', result.stderr)

    def test_ref_generation_is_positive_but_repository_generation_can_be_zero(self):
        schema = json.loads((ROOT / 'docs/mutation-result.schema.json').read_text())['properties']
        ref_pattern = schema['refs']['items']['properties']['generation']['pattern']
        for value in ['0', '00', '01', '-1', '']:
            self.assertIsNone(re.fullmatch(ref_pattern, value), value)
        for value in ['1', '18446744073709551615']:
            self.assertIsNotNone(re.fullmatch(ref_pattern, value), value)
        self.assertIsNotNone(re.fullmatch(schema['generation']['anyOf'][0]['pattern'], '0'))

    def test_recovery_failure_names_stage_case_and_exception(self):
        def fail(report):
            report.update(stage='path-materialization', case='portable')
            report['passed'].append('earlier case')
            raise subprocess.TimeoutExpired(['reader', '--archive'], 60)
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(recover, '__file__', str(Path(directory) / 'recover.py')), \
                    patch.object(recover, 'drill', side_effect=fail):
                self.assertEqual(recover.main(), 1)
            report = json.loads((Path(directory) / 'recovery-report.json').read_text())
            self.assertEqual(report['error']['type'], 'TimeoutExpired')
            self.assertEqual(report['stage'], 'path-materialization')
            self.assertEqual(report['case'], 'portable')
            self.assertEqual(report['passed'], ['earlier case'])

    def test_recovery_metadata_failure_has_stage_before_first_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(recover, '__file__', str(Path(directory) / 'recover.py')):
                self.assertEqual(recover.main(), 1)
            report = json.loads((Path(directory) / 'recovery-report.json').read_text())
            self.assertEqual(report['stage'], 'artifact-verification')
            self.assertEqual(report['error']['type'], 'FileNotFoundError')
            self.assertEqual(report['passed'], [])

    @unittest.skipUnless(os.name == 'posix' and shutil.which('tar'), 'POSIX tar required')
    def test_demo_under_restrictive_umask(self):
        # Exercise the actual demo, archive restoration and its mode assertions.
        old = os.umask(0o077)
        try:
            result = subprocess.run([sys.executable, str(ROOT / 'tools/demo/ascii_movie.py'), '--fast'],
                                    cwd=ROOT, capture_output=True, text=True, timeout=120)
        finally:
            os.umask(old)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Regular mode preserved: 0644', result.stdout)
        self.assertIn('Executable mode preserved: 0755', result.stdout)


if __name__ == '__main__':
    unittest.main()
