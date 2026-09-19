#!/usr/bin/env python3
"""Serial case coordinator. AETHER_READY_FD carries one byte after first frame.

--cases accepts [{id, scene, launcher_args}]. --matrix adapts the existing v1
visual matrix without changing its schema. Repeat --case to select a list in
manifest order. Diagnostics live in an exclusive per-run report directory.
"""
import argparse
import atexit
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import platform
import select
import signal
import sys
import time

from process_lease import Diagnostic, ProvisionalLease, State, Supervisor


def timestamp():
    return datetime.now(timezone.utc).isoformat()


class Runner:
    def __init__(self, supervisor, handshake_timeout=2, case_timeout=120):
        self.supervisor = supervisor
        self.handshake_timeout = handshake_timeout
        self.case_timeout = case_timeout
        self.cancelled = 0
        self.active = None

    def cancel(self, signum, _frame):
        self.cancelled = self.cancelled or signum

    def cleanup(self):
        if self.active is not None:
            self.supervisor.ensure_cleaned(self.active)

    @staticmethod
    def take_ready(lease, read_fd, record):
        if lease.state == State.Bound and select.select([read_fd], [], [], 0)[0]:
            if os.read(read_fd, 1):
                lease.mark_ready(lease.nonce)
                record['ready_at'] = time.monotonic()

    def run_case(self, launcher, case, report_dir):
        directory = report_dir / case['id']
        directory.mkdir()
        argv = [launcher, '--scene', case['scene']] + case['launcher_args'] + ['--no-gui-overlay']
        lease = ProvisionalLease(launcher)
        self.active = lease
        record = {'argv': argv, 'nonce': lease.nonce, 'expected_binary': launcher,
                  'started_at': timestamp(), 'diagnostic': None, 'ready_at': None,
                  'metadata': {'runner_pid': os.getpid(), 'os': platform.platform(),
                               'machine': platform.machine(), 'python': platform.python_version(),
                               'device': 'not queried (runner does not open a GPU session)'}}
        read_fd, write_fd = os.pipe()
        try:
            # Preserve a previous screenshot as an artifact, while allowing
            # the shell's existing missing-image check to detect a failed capture.
            if 'output' in case:
                output = Path(case['output'])
                if output.exists():
                    output.replace(directory / 'previous-output.png')
            with (directory / 'stdout').open('wb') as stdout, (directory / 'stderr').open('wb') as stderr:
                env = dict(os.environ, AETHER_READY_FD=str(write_fd))
                record['spawned_at'] = time.monotonic()
                self.supervisor.spawn(lease, argv, stdout, stderr, env, write_fd)
                os.close(write_fd)
                write_fd = None
                deadline = record['spawned_at'] + self.handshake_timeout
                while True:
                    if self.cancelled:
                        raise Diagnostic('RunnerCancelled')
                    self.take_ready(lease, read_fd, record)
                    if record['ready_at'] is not None:
                        deadline = record['ready_at'] + self.case_timeout
                    if self.supervisor.exited(lease):
                        # The byte may arrive after the first pipe check but
                        # before the OS reports exit. Drain before classifying.
                        self.take_ready(lease, read_fd, record)
                        break
                    if time.monotonic() >= deadline:
                        raise Diagnostic('ChildHandshakeTimeout' if lease.state == State.Bound else 'CaseTimeout')
                    time.sleep(.01)
        except Diagnostic as exc:
            record['diagnostic'] = str(exc)
            if lease.state == State.Ready:
                lease.transition(State.Failed)
        except Exception as exc:
            record['diagnostic'] = 'RunnerError'
            record['error'] = repr(exc)
        finally:
            os.close(read_fd)
            if write_fd is not None:
                os.close(write_fd)
            try:
                self.cleanup()
            except Exception as exc:
                record['diagnostic'] = 'ProcessIsolationLost'
                record['cleanup_error'] = repr(exc)
            identity = lease.identity
            code = lease.process.returncode if lease.process else None
            if record['diagnostic'] is None:
                if code != 0:
                    record['diagnostic'] = 'ChildExit'
                elif record['ready_at'] is None:
                    record['diagnostic'] = 'ChildHandshakeTimeout'
            record.update(pid=identity.pid if identity else None,
                          pgid=identity.pgid if identity else None,
                          os_start_time=identity.start_time if identity else None,
                          ended_at=timestamp(), exit_code=code,
                          signal=-code if code is not None and code < 0 else None,
                          states=lease.states, signals=lease.signals,
                          group_empty=lease.group_empty, cleaned_at=lease.cleaned_at)
            (directory / 'launcher.log').write_text(json.dumps(record, indent=2) + '\n')
            if lease.state == State.Cleaned:
                self.active = None
        return record


def load_cases(args):
    data = json.loads(Path(args.cases or args.matrix).read_text())
    if args.cases:
        cases = data
    else:
        defaults = data.get('defaults', {})
        cases = []
        for item in data['scenes']:
            name = item['name']
            output = str(Path(args.output_dir) / (name + '.png'))
            argv = ['--screenshot', output, '--exit-after-frames', str(item.get('frames', defaults.get('frames', 60)))]
            if item.get('debug_mode'):
                argv += ['--debug-mode', str(item['debug_mode'])]
            for flag in ('ssao', 'ssr'):
                if item.get(flag, False):
                    argv += ['--' + flag]
            argv += ['--freeze-time', '--width', str(item.get('width', defaults.get('width', 1280))),
                     '--height', str(item.get('height', defaults.get('height', 720)))]
            cases.append({'id': name, 'scene': item['scene'], 'launcher_args': argv, 'output': output})
    ids = [case['id'] for case in cases]
    if len(ids) != len(set(ids)) or any(not name or Path(name).name != name or name in ('.', '..') for name in ids):
        raise ValueError('case IDs must be unique directory names')
    if args.case and not set(args.case).issubset(ids):
        raise ValueError('unknown case ID')
    return [case for case in cases if not args.case or case['id'] in args.case]


def duration(value):
    value = float(value)
    if not math.isfinite(value) or value <= 0:
        raise argparse.ArgumentTypeError('duration must be positive and finite')
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--launcher', required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument('--cases')
    source.add_argument('--matrix')
    parser.add_argument('--case', action='append', default=[])
    parser.add_argument('--output-dir', default='tests/output')
    parser.add_argument('--report-dir', required=True)
    for option, variable, default in [('handshake-timeout', 'AETHER_HANDSHAKE_TIMEOUT', '2'),
                                      ('case-timeout', 'AETHER_CASE_TIMEOUT', '120'),
                                      ('grace-period', 'AETHER_TERMINATION_GRACE', '5')]:
        parser.add_argument('--' + option, type=duration, default=os.environ.get(variable, default))
    parser.add_argument('--settle-seconds', type=float, default=0)
    args = parser.parse_args()
    cases = load_cases(args)
    runner = Runner(Supervisor(args.grace_period), args.handshake_timeout, args.case_timeout)
    atexit.register(runner.cleanup)
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, runner.cancel)
    report_dir = Path(args.report_dir)
    report_dir.mkdir(parents=True, exist_ok=False)
    failed = False
    try:
        for case in cases:
            if runner.cancelled:
                break
            record = runner.run_case(args.launcher, case, report_dir)
            failed |= record['diagnostic'] is not None
            if record['diagnostic'] == 'ProcessIsolationLost':
                break  # fail closed: never start another case with uncertain ownership
            if record['diagnostic'] is None:
                deadline = time.monotonic() + args.settle_seconds
                while not runner.cancelled and time.monotonic() < deadline:
                    time.sleep(min(.05, max(0, deadline - time.monotonic())))
    finally:
        runner.cleanup()
        atexit.unregister(runner.cleanup)
    return 128 + runner.cancelled if runner.cancelled else int(failed)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (Diagnostic, ValueError, OSError) as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(2)
