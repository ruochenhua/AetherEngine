"""Real, bounded fake-process tests. Every launcher lives in TemporaryDirectory."""
import json
import os
from pathlib import Path
from dataclasses import replace
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "scripts/runner_process.py"
sys.path.insert(0, str(ROOT / "scripts"))

FAKE = r'''#!/usr/bin/env python3
import json, os, signal, sys, time
from pathlib import Path
signal.alarm(10)
mode = sys.argv[2]
root = Path(os.environ['FAKE_ROOT'])
(root / (mode + '.started')).write_text(str(time.monotonic()))
(root / (mode + '.argv')).write_text(json.dumps(sys.argv))
print('stdout-' + mode, flush=True)
print('stderr-' + mode, file=sys.stderr, flush=True)
if mode in ('hang', 'timeout', 'orphan', 'panic'):
    child = os.fork()
    if child == 0:
        signal.alarm(10)
        os.close(int(os.environ['AETHER_READY_FD']))
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        (root / (mode + '.descendant')).write_text(str(os.getpid()))
        while True: time.sleep(.02)
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
if mode == 'delayed': time.sleep(.6)
if mode not in ('hang', 'early'):
    os.write(int(os.environ['AETHER_READY_FD']), b'R')
os.close(int(os.environ['AETHER_READY_FD']))
if mode == 'panic':
    print('panic diagnostic', file=sys.stderr, flush=True)
    sys.exit(17)
if mode == 'early': sys.exit(19)
if mode == 'signal': sys.exit(134)
if mode in ('hang', 'timeout'):
    while True: time.sleep(.02)
if mode == 'delayed': time.sleep(.15)
'''


class ProcessTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue(RUNNER.exists(), 'Missing serial process supervisor')
        import runner_process
        import process_lease
        self.runner = runner_process
        self.lease = process_lease
        self.tmp = tempfile.TemporaryDirectory(prefix='aether-process-test-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.fake = self.root / 'fake launcher'
        self.fake.write_text(FAKE.replace('#!/usr/bin/env python3', '#!' + sys.executable))
        self.fake.chmod(0o755)
        self.env = dict(os.environ, FAKE_ROOT=str(self.root))

    def command(self, modes, *options):
        cases = [{'id': mode, 'scene': mode, 'launcher_args': ['literal ; $(no shell)']}
                 for mode in modes]
        manifest = self.root / 'cases.json'
        manifest.write_text(json.dumps(cases))
        return [sys.executable, str(RUNNER), '--launcher', str(self.fake),
                '--cases', str(manifest), '--report-dir', str(self.root / 'reports'),
                '--handshake-timeout', '4', '--case-timeout', '.5',
                '--grace-period', '.1', *options]

    def run_cases(self, modes, *options):
        result = subprocess.run(self.command(modes, *options), env=self.env,
                                capture_output=True, text=True, timeout=15)
        if result.returncode:
            result.stderr += '\n'.join(path.read_text() for path in (self.root / 'reports').glob('*/launcher.log'))
        return result

    def log(self, mode):
        return json.loads((self.root / 'reports' / mode / 'launcher.log').read_text())

    def assert_gone(self, record, mode):
        with self.assertRaises(ProcessLookupError):
            os.killpg(record['pgid'], 0)
        descendant = self.root / (mode + '.descendant')
        if descendant.exists():
            with self.assertRaises(ProcessLookupError):
                os.kill(int(descendant.read_text()), 0)
        self.assertTrue(record['group_empty'])

    def test_success_artifacts_and_exact_argv(self):
        result = self.run_cases(['ok'])
        self.assertEqual(result.returncode, 0, result.stderr)
        record = self.log('ok')
        self.assertEqual(record['states'], ['Reserved', 'Bound', 'Ready', 'Terminating', 'Cleaned'])
        argv = [str(self.fake), '--scene', 'ok', 'literal ; $(no shell)', '--no-gui-overlay']
        self.assertEqual(record['argv'], argv)
        self.assertEqual(json.loads((self.root / 'ok.argv').read_text()), argv)
        for key in ('pid', 'pgid', 'os_start_time', 'nonce', 'started_at', 'ended_at',
                    'exit_code', 'signal', 'metadata', 'ready_at'):
            self.assertIn(key, record)
        self.assertEqual(record['exit_code'], 0)
        self.assertEqual((self.root / 'reports/ok/stdout').read_text(), 'stdout-ok\n')
        self.assertEqual((self.root / 'reports/ok/stderr').read_text(), 'stderr-ok\n')
        self.assert_gone(record, 'ok')

    def test_handshake_timeout_serial_cleanup(self):
        result = self.run_cases(['hang', 'ok'])
        self.assertNotEqual(result.returncode, 0)
        record = self.log('hang')
        self.assertEqual(record['diagnostic'], 'ChildHandshakeTimeout')
        self.assertEqual(record['states'], ['Reserved', 'Bound', 'Terminating', 'Cleaned'])
        self.assert_gone(record, 'hang')
        self.assertLessEqual(record['cleaned_at'], float((self.root / 'ok.started').read_text()))
        self.assertEqual(self.log('ok')['exit_code'], 0)

    def test_panic_timeout_orphan_and_signal_keep_diagnostics(self):
        for mode, diagnostic, code in [('panic', 'ChildExit', 17),
                                       ('timeout', 'CaseTimeout', -signal.SIGKILL),
                                       ('orphan', None, 0), ('early', 'ChildExit', 19),
                                       ('signal', 'ChildExit', 134)]:
            with self.subTest(mode=mode):
                self.run_cases([mode])
                record = self.log(mode)
                self.assertEqual(record['diagnostic'], diagnostic)
                self.assertEqual(record['exit_code'], code)
                self.assertIn('stderr-' + mode, (self.root / 'reports' / mode / 'stderr').read_text())
                self.assert_gone(record, mode)
                if mode in ('panic', 'timeout', 'orphan'):
                    self.assertEqual([s['signal'] for s in record['signals']], ['SIGTERM', 'SIGKILL'])
                    self.assertEqual({s['pgid'] for s in record['signals']}, {record['pgid']})
                shutil.rmtree(self.root / 'reports')

    def test_timer_starts_at_ready(self):
        result = self.run_cases(['delayed'])
        self.assertEqual(result.returncode, 0, result.stderr)
        record = self.log('delayed')
        self.assertGreater(record['ready_at'] - record['spawned_at'], .55)
        self.assertLess(record['cleaned_at'] - record['ready_at'], 1.5)

    def test_ready_written_between_pipe_check_and_exit_observation(self):
        supervisor = self.lease.Supervisor(.1)
        runner = self.runner.Runner(supervisor, 2, 2)
        report_dir = self.root / 'reports'
        report_dir.mkdir()
        exited = supervisor.exited

        def observe_exit(lease):
            deadline = time.monotonic() + 3
            while not exited(lease):
                if time.monotonic() > deadline:
                    raise AssertionError('fake did not exit')
                time.sleep(.01)
            return True

        with mock.patch.dict(os.environ, self.env), mock.patch.object(supervisor, 'exited', side_effect=observe_exit):
            record = runner.run_case(str(self.fake), {'id': 'delayed', 'scene': 'delayed', 'launcher_args': []}, report_dir)
        self.assertIsNone(record['diagnostic'])
        self.assertIn('Ready', record['states'])

    def test_signal_and_exit_cleanup(self):
        for sig in (signal.SIGINT, signal.SIGTERM):
            with self.subTest(signal=sig):
                proc = subprocess.Popen(self.command(['timeout', 'ok'], '--case-timeout', '8'),
                                        env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
                try:
                    deadline = time.monotonic() + 4
                    marker = self.root / 'timeout.descendant'
                    while not marker.exists() and time.monotonic() < deadline:
                        time.sleep(.01)
                    self.assertTrue(marker.exists())
                    proc.send_signal(sig)
                    proc.send_signal(sig)
                    _, stderr = proc.communicate(timeout=8)
                    self.assertEqual(proc.returncode, 128 + sig, stderr)
                    self.assert_gone(self.log('timeout'), 'timeout')
                    self.assertFalse((self.root / 'ok.started').exists())
                finally:
                    if proc.poll() is None:
                        proc.kill()
                        proc.wait()
                    proc.stderr.close()
                marker.unlink()
                # A fresh run directory is required: logs must never be overwritten.
                shutil.rmtree(self.root / 'reports')

    def test_lease_rejects_illegal_stale_nonce_and_duplicate_cleanup(self):
        api = self.lease
        lease = api.ProvisionalLease(str(self.fake))
        with self.assertRaisesRegex(api.Diagnostic, 'IllegalTransition'):
            lease.mark_ready(lease.nonce)
        supervisor = api.Supervisor(grace_period=.01)
        with open(os.devnull, 'wb') as sink:
            read_fd, write_fd = os.pipe()
            try:
                supervisor.spawn(lease, [str(self.fake), '--scene', 'timeout'], sink, sink,
                                 dict(self.env, AETHER_READY_FD=str(write_fd)), write_fd)
                identity = lease.identity
                with self.assertRaisesRegex(api.Diagnostic, 'IllegalTransition'):
                    lease.bind(identity, lease.nonce)
                for nonce in ('wrong',):
                    with self.assertRaisesRegex(api.Diagnostic, 'ProcessIsolationLost'):
                        supervisor.cleanup(lease, nonce)
                    with self.assertRaisesRegex(api.Diagnostic, 'ProcessIsolationLost'):
                        lease.mark_ready(nonce)
                with mock.patch.object(api, 'identity_for', return_value=None):
                    with mock.patch.object(os, 'killpg') as kill:
                        with self.assertRaisesRegex(api.Diagnostic, 'ProcessIsolationLost'):
                            supervisor.cleanup(lease, lease.nonce)
                        kill.assert_not_called()
                for stale in (replace(identity, start_time='stale'), replace(identity, pgid=identity.pgid + 1)):
                    with mock.patch.object(api, 'identity_for', return_value=stale), mock.patch.object(os, 'killpg') as kill:
                        with self.assertRaisesRegex(api.Diagnostic, 'ProcessIsolationLost'):
                            lease.mark_ready(lease.nonce)
                        with self.assertRaisesRegex(api.Diagnostic, 'ProcessIsolationLost'):
                            supervisor.cleanup(lease, lease.nonce)
                        kill.assert_not_called()
                supervisor.cleanup(lease, lease.nonce)
                with self.assertRaisesRegex(api.Diagnostic, 'AlreadyCleaned'):
                    supervisor.cleanup(lease, lease.nonce)
                with mock.patch.object(os, 'killpg') as kill:
                    supervisor.ensure_cleaned(lease)  # orchestration is idempotent
                    kill.assert_not_called()
            finally:
                supervisor.ensure_cleaned(lease)
                os.close(read_fd)
                os.close(write_fd)

    def test_spawn_bind_and_unsupported_fail_closed(self):
        api = self.lease
        with mock.patch.object(api, 'supported', return_value=False):
            with mock.patch.object(os, 'fork') as spawn:
                with self.assertRaisesRegex(api.Diagnostic, 'UnsupportedProcessIsolation'):
                    api.Supervisor()
                spawn.assert_not_called()
        for fail_bind in (False, True):
            lease = api.ProvisionalLease(str(self.fake))
            supervisor = api.Supervisor(grace_period=.01)
            binary = str(self.fake) if fail_bind else str(self.root / 'missing')
            with open(os.devnull, 'wb') as sink:
                read_fd, write_fd = os.pipe()
                try:
                    patch = mock.patch.object(lease, 'bind', side_effect=api.Diagnostic('BindFailure'))
                    with patch if fail_bind else mock.patch.dict(os.environ, {}):
                        with self.assertRaises(api.Diagnostic):
                            supervisor.spawn(lease, [binary, '--scene', 'timeout'], sink, sink,
                                             dict(self.env, AETHER_READY_FD=str(write_fd)), write_fd)
                    self.assertEqual(lease.state.value, 'Cleaned')
                    if lease.identity:
                        with self.assertRaises(ProcessLookupError):
                            os.killpg(lease.identity.pgid, 0)
                finally:
                    supervisor.ensure_cleaned(lease)
                    os.close(read_fd)
                    os.close(write_fd)

    def test_no_terminal_and_ci_wiring(self):
        for path in (RUNNER, ROOT / 'scripts/process_lease.py'):
            self.assertNotIn('osascript', path.read_text())
            self.assertNotIn('tell application', path.read_text())
        self.assertIn('./tests/runner-process-test.sh', (ROOT / 'scripts/verify-ci.sh').read_text())

    def test_identity_probe_failure_never_executes_launcher(self):
        api = self.lease
        supervisor = api.Supervisor(grace_period=.01)
        lease = api.ProvisionalLease(str(self.fake))
        read_fd, write_fd = os.pipe()
        try:
            with open(os.devnull, 'wb') as sink, mock.patch.object(api, 'identity_for', return_value=None):
                with self.assertRaisesRegex(api.Diagnostic, 'ProcessIsolationLost'):
                    supervisor.spawn(lease, [str(self.fake), '--scene', 'ok'], sink, sink,
                                     dict(self.env, AETHER_READY_FD=str(write_fd)), write_fd)
            self.assertFalse((self.root / 'ok.started').exists())
            self.assertEqual(lease.state.value, 'Cleaned')
        finally:
            os.close(read_fd)
            os.close(write_fd)

    def test_group_setup_failure_never_executes_launcher(self):
        api = self.lease
        supervisor = api.Supervisor(grace_period=.01)
        lease = api.ProvisionalLease(str(self.fake))
        read_fd, write_fd = os.pipe()
        try:
            with open(os.devnull, 'wb') as sink, mock.patch.object(os, 'setsid', side_effect=OSError('setup failed')):
                with self.assertRaisesRegex(api.Diagnostic, 'UnsupportedProcessIsolation'):
                    supervisor.spawn(lease, [str(self.fake), '--scene', 'ok'], sink, sink,
                                     dict(self.env, AETHER_READY_FD=str(write_fd)), write_fd)
            self.assertFalse((self.root / 'ok.started').exists())
            self.assertEqual(lease.state.value, 'Cleaned')
            with self.assertRaises(ProcessLookupError):
                os.kill(lease.process.pid, 0)
        finally:
            os.close(read_fd)
            os.close(write_fd)

    def test_binary_identity_mismatch_rejected_before_exec(self):
        api = self.lease
        supervisor = api.Supervisor(grace_period=.01)
        lease = api.ProvisionalLease(str(self.root / 'different-binary'))
        read_fd, write_fd = os.pipe()
        try:
            with open(os.devnull, 'wb') as sink:
                with self.assertRaisesRegex(api.Diagnostic, 'ProcessIsolationLost'):
                    supervisor.spawn(lease, [str(self.fake), '--scene', 'ok'], sink, sink,
                                     dict(self.env, AETHER_READY_FD=str(write_fd)), write_fd)
            self.assertFalse((self.root / 'ok.started').exists())
        finally:
            supervisor.ensure_cleaned(lease)
            os.close(read_fd)
            os.close(write_fd)

    def test_explicit_selection_is_manifest_order(self):
        result = self.run_cases(['first', 'excluded', 'last'], '--case', 'last', '--case', 'first')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.root / 'excluded.started').exists())
        self.assertLessEqual(self.log('first')['cleaned_at'], float((self.root / 'last.started').read_text()))

    def test_exit_hook_cleans_an_active_lease(self):
        source = '''
import atexit, os, sys, time
from process_lease import ProvisionalLease, Supervisor
from runner_process import Runner
supervisor = Supervisor(.1)
runner = Runner(supervisor)
runner.active = ProvisionalLease(sys.argv[1])
atexit.register(runner.cleanup)
read_fd, write_fd = os.pipe()
with open(os.devnull, 'wb') as sink:
    supervisor.spawn(runner.active, [sys.argv[1], '--scene', 'timeout'], sink, sink,
                     dict(os.environ, AETHER_READY_FD=str(write_fd)), write_fd)
print(runner.active.identity.pgid, flush=True)
os.close(write_fd)
os.read(read_fd, 1)
os.close(read_fd)
time.sleep(.1)
'''
        result = subprocess.run([sys.executable, '-c', source, str(self.fake)],
                                env=dict(self.env, PYTHONPATH=str(ROOT / 'scripts')),
                                capture_output=True, text=True, timeout=8)
        self.assertEqual(result.returncode, 0, result.stderr)
        with self.assertRaises(ProcessLookupError):
            os.killpg(int(result.stdout.strip()), 0)
        with self.assertRaises(ProcessLookupError):
            os.kill(int((self.root / 'timeout.descendant').read_text()), 0)

    def test_shell_comparison_and_reference_behavior(self):
        project = self.root / 'project'
        (project / 'scripts').mkdir(parents=True)
        for filename in ('verify-regression.sh', 'runner_process.py', 'process_lease.py'):
            shutil.copy(ROOT / 'scripts' / filename, project / 'scripts' / filename)
        (project / 'tests/reference').mkdir(parents=True)
        matrix = {'defaults': {'threshold': .99}, 'scenes': [
            {'name': 'sample', 'scene': 'fake scene.ron', 'debug_mode': 6, 'ssao': True, 'ssr': True}]}
        (project / 'tests/visual-matrix.json').write_text(json.dumps(matrix))
        launcher = self.root / 'capture'
        launcher.write_text('#!' + sys.executable + '\n' + '''
import os, sys
from pathlib import Path
os.write(int(os.environ['AETHER_READY_FD']), b'R')
if os.environ.get('FAKE_FAIL'): sys.exit(23)
if not os.environ.get('FAKE_NO_IMG'):
    Path(sys.argv[sys.argv.index('--screenshot')+1]).write_bytes(b'new')
''')
        launcher.chmod(0o755)
        compare = self.root / 'compare.py'
        compare.write_text('''import os, sys
from pathlib import Path
for flag in ('--diff', '--side-by-side'):
    Path(sys.argv[sys.argv.index(flag)+1]).write_bytes(b'diff')
print(os.environ.get('FAKE_METRICS', '{"ssim":1,"mae":0,"diff_pct":0}'))
''')
        env = dict(self.env, AETHER_LAUNCHER_BIN=str(launcher), AETHER_COMPARE_SCRIPT=str(compare),
                   AETHER_REGRESSION_SETTLE_SECONDS='0', AETHER_HANDSHAKE_TIMEOUT='8')
        reference = project / 'tests/reference/sample.png'

        def run(name, status, code=0, extra=(), **variables):
            result = subprocess.run([str(project / 'scripts/verify-regression.sh'), '--scene', 'sample',
                                     '--report', name, *extra], env=dict(env, **variables),
                                    capture_output=True, text=True, timeout=20)
            self.assertEqual(result.returncode, code, result.stdout + result.stderr)
            html = (project / 'tests/reports' / (name + '.html')).read_text()
            self.assertIn('<!doctype html>', html)
            self.assertIn(status, html)
            return html

        run('new', 'NEW')
        self.assertFalse(reference.exists())
        run('create', 'REF_CREATED', extra=('--update-references', '--reason', 'create baseline'))
        self.assertEqual(reference.read_bytes(), b'new')
        reference.write_bytes(b'old')
        run('crash', 'CRASH', 1, ('--update-references', '--reason', 'refresh after approved change'), FAKE_FAIL='1')
        self.assertEqual(reference.read_bytes(), b'old')
        self.assertTrue(list((project / 'tests/reports').glob('*/sample/previous-output.png')))
        run('missing', 'NO_IMG', 1, FAKE_NO_IMG='1')
        run('update', 'REF_UPDATED', extra=('--update-references', '--reason', 'approved baseline refresh'))
        update_html = (project / 'tests/reports' / 'update.html').read_text()
        self.assertIn('approved baseline refresh', update_html)
        html = run('pass', 'PASS')
        self.assertIn('../output/sample.diff.png', html)
        self.assertIn('../output/sample.side.png', html)
        run('regression', 'REGRESSION', 1, FAKE_METRICS='{"ssim":0.9,"mae":2,"diff_pct":3}')
        run('fallback', 'PASS', FAKE_METRICS='{"ssim":null,"diff_pct":1.5}')
        missing_metrics = run('missing-metrics', 'PASS', FAKE_METRICS='{"ssim":null,"diff_pct":1.5}')
        self.assertIn('<td>N/A</td><td>N/A</td><td>1.50%</td>', missing_metrics)
        run('fallback-fail', 'REGRESSION', 1, FAKE_METRICS='{"ssim":null,"diff_pct":2.5}')
        run('bad-json', 'COMPARE_ERROR', 1, FAKE_METRICS='invalid json')

    def test_reference_update_requires_reason(self):
        project = self.root / 'project'
        (project / 'scripts').mkdir(parents=True)
        (project / 'tests/reference').mkdir(parents=True)
        for filename in ('verify-regression.sh', 'runner_process.py', 'process_lease.py'):
            shutil.copy(ROOT / 'scripts' / filename, project / 'scripts' / filename)
        (project / 'tests/visual-matrix.json').write_text(json.dumps({
            'scenes': [{'name': 'sample', 'scene': 'fake scene.ron'}]}))
        launcher = self.root / 'capture'
        launcher.write_text('#!' + sys.executable + '\n' + '''
import os, sys
from pathlib import Path
os.write(int(os.environ['AETHER_READY_FD']), b'R')
Path(sys.argv[sys.argv.index('--screenshot') + 1]).write_bytes(b'new')
''')
        launcher.chmod(0o755)
        result = subprocess.run(
            [str(project / 'scripts/verify-regression.sh'), '--scene', 'sample',
             '--report', 'missing-reason', '--update-references'],
            env=dict(self.env, AETHER_LAUNCHER_BIN=str(launcher)),
            capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((project / 'tests/reference/sample.png').exists())

    def test_matrix_simulation_time_is_forwarded_for_repeatable_capture(self):
        matrix = self.root / 'matrix.json'
        matrix.write_text(json.dumps({
            'defaults': {'simulation_time': 0.5},
            'scenes': [{'name': 'sample', 'scene': 'repeatable.ron'}]}))
        report_dir = self.root / 'reports'
        result = subprocess.run(
            [sys.executable, str(RUNNER), '--launcher', str(self.fake), '--matrix', str(matrix),
             '--report-dir', str(report_dir), '--handshake-timeout', '8', '--case-timeout', '2'],
            env=self.env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        argv = json.loads((self.root / 'repeatable.ron.argv').read_text())
        self.assertIn('--time-mode', argv)
        self.assertIn('seek', argv)
        self.assertIn('--simulation-time', argv)
        self.assertEqual(argv[argv.index('--simulation-time') + 1], '0.5')

    def test_shell_cancellation_waits_for_supervisor(self):
        project = self.root / 'project'
        (project / 'scripts').mkdir(parents=True)
        (project / 'tests').mkdir()
        for filename in ('verify-regression.sh', 'runner_process.py', 'process_lease.py'):
            shutil.copy(ROOT / 'scripts' / filename, project / 'scripts' / filename)
        (project / 'tests/visual-matrix.json').write_text(json.dumps({'scenes': [
            {'name': 'hang', 'scene': 'hang'}, {'name': 'ok', 'scene': 'ok'}]}))
        env = dict(self.env, AETHER_LAUNCHER_BIN=str(self.fake), AETHER_HANDSHAKE_TIMEOUT='8',
                   AETHER_TERMINATION_GRACE='.1', AETHER_REGRESSION_SETTLE_SECONDS='0')
        proc = subprocess.Popen([str(project / 'scripts/verify-regression.sh')], env=env,
                                stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 6
            marker = self.root / 'hang.descendant'
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(.01)
            self.assertTrue(marker.exists())
            proc.terminate()
            _, stderr = proc.communicate(timeout=8)
            self.assertEqual(proc.returncode, 143, stderr)
            records = list((project / 'tests/reports').glob('*/hang/launcher.log'))
            self.assertEqual(len(records), 1)
            record = json.loads(records[0].read_text())
            self.assert_gone(record, 'hang')
            self.assertFalse((self.root / 'ok.started').exists())
            with self.assertRaises(ProcessLookupError):
                os.kill(record['metadata']['runner_pid'], 0)
        finally:
            if proc.poll() is None:
                proc.terminate()
                proc.wait(timeout=10)
            proc.stderr.close()


if __name__ == '__main__':
    unittest.main(verbosity=2)
