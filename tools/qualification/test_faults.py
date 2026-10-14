"""Regression checks for portable reporting and retained failure evidence."""
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import faults


class ReportingTests(unittest.TestCase):
    def test_environment_commit_never_invokes_git(self):
        with patch.dict(faults.os.environ, {'GITHUB_SHA': 'supplied-commit'}), \
                patch.object(faults.subprocess, 'check_output', side_effect=FileNotFoundError) as git:
            self.assertEqual(faults.source_commit(), 'supplied-commit')
            git.assert_not_called()

    def test_missing_git_or_checkout_leaves_commit_unknown(self):
        for failure in [FileNotFoundError(), subprocess.CalledProcessError(128, ['git'])]:
            with self.subTest(failure=type(failure).__name__), \
                    patch.dict(faults.os.environ, {}, clear=True), \
                    patch.object(faults.subprocess, 'check_output', side_effect=failure):
                self.assertIsNone(faults.source_commit())

    def test_git_fallback_records_commit(self):
        with patch.dict(faults.os.environ, {}, clear=True), \
                patch.object(faults.subprocess, 'check_output', return_value='local-commit\n'):
            self.assertEqual(faults.source_commit(), 'local-commit')

    def test_failed_cut_preserves_completed_results_and_error(self):
        trace = [{'point': 'before-write', 'completed_bytes': 0},
                 {'point': 'after-write', 'completed_bytes': 4}]

        def child_run(child, bundle, output, cut=None):
            if cut == 1:
                raise RuntimeError('injected boundary failure')
            output.write_bytes(b'data' if cut is None else b'')
            return (trace, True) if cut is None else (trace[:1], False)

        def reader_run(command, **kwargs):
            output = Path(command[-1])
            if output.exists():
                return SimpleNamespace(returncode=1, stderr=b'output exists')
            output.write_bytes(b'data')
            return SimpleNamespace(returncode=0, stderr=b'')

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = root / 'input'
            inputs.write_bytes(b'fixture')
            args = SimpleNamespace(child=inputs, reader=inputs, bundle=inputs,
                                   report=root / 'report.json')
            with patch.object(faults.platform, 'platform', return_value='test-platform'), \
                    patch.object(faults, 'source_commit', return_value='test-commit'), \
                    patch.object(faults, 'run_child', side_effect=child_run), \
                    patch.object(faults.subprocess, 'run', side_effect=reader_run):
                with self.assertRaisesRegex(RuntimeError, 'injected boundary failure'):
                    faults.qualify(args)
            report = json.loads(args.report.read_text())
            self.assertEqual(report['status'], 'failed')
            self.assertEqual(report['stage'], 'cut')
            self.assertEqual(report['active_boundary'], dict(trace[1], index=1))
            self.assertEqual(len(report['cuts']), 1)
            self.assertTrue(report['cuts'][0]['restart_recovery_equal'])
            self.assertEqual(report['error']['message'], 'injected boundary failure')
            self.assertEqual(inputs.read_bytes(), b'fixture')

    def test_missing_input_still_writes_failure_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = SimpleNamespace(child=root / 'missing', reader=root / 'missing',
                                   bundle=root / 'missing', report=root / 'report.json')
            with patch.object(faults, 'source_commit', return_value=None):
                with self.assertRaises(FileNotFoundError):
                    faults.qualify(args)
            report = json.loads(args.report.read_text())
            self.assertEqual(report['status'], 'failed')
            self.assertEqual(report['stage'], 'inputs')
            self.assertEqual(report['cuts'], [])
            self.assertIsNone(report['commit'])


if __name__ == '__main__':
    unittest.main()
