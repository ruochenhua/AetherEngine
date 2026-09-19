"""Identity-bound POSIX process groups, using only the Python standard library.

The supervisor is the sole waiter for its fork/exec child. It never wait()s
until group signalling is finished: even an exited leader remains an unreaped
child, reserving its PID (and hence PGID). This closes the PID-reuse race between
identity validation and killpg. Signal handlers only set a cancellation flag;
they never re-enter this state machine. Descendants must stay in the group.
"""
from dataclasses import dataclass
from enum import Enum
import os
import select
import signal
import subprocess
import sys
import threading
import time
import uuid


class Diagnostic(RuntimeError):
    pass


class State(Enum):
    Reserved = 'Reserved'
    Bound = 'Bound'
    Ready = 'Ready'
    Terminating = 'Terminating'
    Failed = 'Failed'
    Cleaned = 'Cleaned'


@dataclass(frozen=True)
class Identity:
    pid: int
    start_time: str
    pgid: int


def process_table(pid=None):
    # lstart is an OS start time. Its second resolution is sufficient because
    # the unreaped direct child pins the PID for the entire lease lifetime.
    selector = ['-p', str(pid)] if pid is not None else ['-ax']
    result = subprocess.run(['/bin/ps', *selector, '-o', 'pid=,pgid=,lstart=,stat='],
                            capture_output=True, text=True, timeout=2,
                            env=dict(os.environ, LC_ALL='C'))
    if result.returncode != 0 and not (pid is not None and result.returncode == 1 and not result.stdout):
        result.check_returncode()
    records = {}
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) == 8:
            pid, pgid = int(fields[0]), int(fields[1])
            records[pid] = (Identity(pid, ' '.join(fields[2:7]), pgid), fields[7])
    return records


def identity_for(pid):
    record = process_table(pid).get(pid)
    return record[0] if record else None


def supported():
    if sys.platform not in ('darwin', 'linux') or not hasattr(os, 'fork'):
        return False
    if signal.getsignal(signal.SIGCHLD) == signal.SIG_IGN:
        return False  # automatic reaping would break the identity pin
    try:
        return identity_for(os.getpid()) is not None
    except (OSError, subprocess.SubprocessError):
        return False


class ProvisionalLease:
    def __init__(self, expected_binary):
        self.nonce = uuid.uuid4().hex
        self.expected_binary = expected_binary
        self.state = State.Reserved
        self.states = [self.state.value]
        self.identity = None
        self.process = None
        self.signals = []
        self.group_empty = False
        self.cleaned_at = None
        self.lock = threading.RLock()

    def transition(self, state):
        allowed = {
            State.Reserved: (State.Bound, State.Failed),
            State.Bound: (State.Ready, State.Terminating),
            State.Ready: (State.Terminating, State.Failed),
            State.Terminating: (State.Cleaned,),
            State.Failed: (State.Cleaned,),
            State.Cleaned: (),
        }
        if state not in allowed[self.state]:
            raise Diagnostic('AlreadyCleaned' if self.state == State.Cleaned else 'IllegalTransition')
        self.state = state
        self.states.append(state.value)

    def validate(self, nonce):
        try:
            if (nonce != self.nonce or self.identity is None or self.process is None
                    or self.process.returncode is not None
                    or self.process.pid != self.identity.pid
                    or self.identity.pgid != self.identity.pid
                    or identity_for(self.identity.pid) != self.identity):
                raise Diagnostic('ProcessIsolationLost')
        except (OSError, subprocess.SubprocessError) as exc:
            raise Diagnostic('ProcessIsolationLost') from exc

    def bind(self, identity, nonce):
        with self.lock:
            if self.state != State.Reserved:
                raise Diagnostic('IllegalTransition')
            self.identity = identity
            self.validate(nonce)
            self.transition(State.Bound)

    def mark_ready(self, nonce):
        with self.lock:
            if self.state != State.Bound:
                raise Diagnostic('IllegalTransition')
            self.validate(nonce)
            self.transition(State.Ready)


class Supervisor:
    def __init__(self, grace_period=5):
        if not supported():
            raise Diagnostic('UnsupportedProcessIsolation')
        self.grace_period = grace_period

    def spawn(self, lease, argv, stdout, stderr, env, ready_fd):
        if lease.state != State.Reserved or lease.process is not None:
            raise Diagnostic('IllegalTransition')
        # Gate exec until identity binding succeeds. If setup/binding fails,
        # closing the gate revokes a child that cannot yet have descendants.
        gate_read, gate_write = os.pipe()
        ack_read, ack_write = os.pipe()
        released = False
        try:
            if argv[0] != lease.expected_binary:
                raise Diagnostic('ProcessIsolationLost')
            if not os.path.isfile(argv[0]) or not os.access(argv[0], os.X_OK):
                raise Diagnostic('SpawnFailure')
            try:
                pid = os.fork()
            except OSError as exc:
                raise Diagnostic('SpawnFailure') from exc
            if pid == 0:
                try:
                    os.dup2(stdout.fileno(), 1)
                    os.dup2(stderr.fileno(), 2)
                    os.close(gate_write)
                    os.close(ack_read)
                    os.setsid()
                    os.write(ack_write, b'G')
                    os.close(ack_write)
                    if os.read(gate_read, 1) != b'G':
                        os._exit(125)
                    os.close(gate_read)
                    os.set_inheritable(ready_fd, True)
                    # Enumerate actual descriptors: Darwin's SC_OPEN_MAX can
                    # exceed a million, making an EBADF close loop very slow.
                    for entry in os.listdir('/dev/fd'):
                        fd = int(entry)
                        if fd > 2 and fd != ready_fd:
                            try:
                                os.close(fd)
                            except OSError:
                                pass  # directory iterator's fd is already closed
                    for sig in (signal.SIGINT, signal.SIGTERM):
                        signal.signal(sig, signal.SIG_DFL)
                    os.execve(argv[0], argv, env)
                except BaseException as exc:
                    os.write(2, ('SpawnFailure: ' + repr(exc) + '\n').encode())
                    os._exit(127)
            lease.process = Child(pid)
            os.close(gate_read)
            gate_read = None
            os.close(ack_write)
            ack_write = None
            if not select.select([ack_read], [], [], 2)[0] or os.read(ack_read, 1) != b'G':
                raise Diagnostic('UnsupportedProcessIsolation')
            lease.identity = identity_for(lease.process.pid)
            lease.bind(lease.identity, lease.nonce)
            os.write(gate_write, b'G')
            released = True
        except Exception:
            os.close(gate_write)
            gate_write = None
            if released:
                self.ensure_cleaned(lease)
            else:
                # No executable has run; EOF on the gate is enough to revoke
                # even when the identity probe or group setup itself failed.
                if lease.process:
                    lease.process.wait(timeout=2)
                lease.transition(State.Failed)
                lease.transition(State.Cleaned)
            raise
        finally:
            for fd in (gate_read, gate_write, ack_read, ack_write):
                if fd is not None:
                    os.close(fd)

    def exited(self, lease):
        record = process_table(lease.identity.pid).get(lease.identity.pid)
        if record is None or record[0] != lease.identity:
            raise Diagnostic('ProcessIsolationLost')
        return record[1].startswith('Z')

    def send(self, lease, nonce, sig):
        # The lock + sole-reaper ownership + deferred signal handlers make
        # validation/signalling one atomic lease operation, without PID reuse.
        with lease.lock:
            lease.validate(nonce)
            try:
                os.killpg(lease.identity.pgid, sig)
            except PermissionError:
                # Darwin reports EPERM for a group containing only zombies.
                if self.live_members(lease):
                    raise
            lease.signals.append({'signal': signal.Signals(sig).name,
                                  'pgid': lease.identity.pgid,
                                  'at': time.monotonic()})

    def live_members(self, lease):
        return [identity.pid for identity, status in process_table().values()
                if identity.pgid == lease.identity.pgid and not status.startswith('Z')]

    def cleanup(self, lease, nonce):
        with lease.lock:
            if lease.state == State.Cleaned:
                raise Diagnostic('AlreadyCleaned')
            lease.validate(nonce)
            if lease.state not in (State.Terminating, State.Failed):
                lease.transition(State.Terminating)
            self.send(lease, nonce, signal.SIGTERM)
            deadline = time.monotonic() + self.grace_period
            while self.live_members(lease) and time.monotonic() < deadline:
                time.sleep(.01)
            if self.live_members(lease):
                self.send(lease, nonce, signal.SIGKILL)
            # Do not release the PID pin while any group member can still run.
            deadline = time.monotonic() + 5
            while self.live_members(lease):
                if time.monotonic() >= deadline:
                    raise Diagnostic('ProcessIsolationLost')
                time.sleep(.01)
            lease.process.wait(timeout=2)
            # No more signals after reaping. Confirm the group, including
            # orphan zombies reaped by init, is gone before starting any case.
            while True:
                try:
                    os.killpg(lease.identity.pgid, 0)
                except ProcessLookupError:
                    break
                if time.monotonic() >= deadline:
                    raise Diagnostic('ProcessIsolationLost')
                time.sleep(.01)
            lease.group_empty = True
            lease.cleaned_at = time.monotonic()
            lease.transition(State.Cleaned)

    def ensure_cleaned(self, lease):
        if lease.state == State.Cleaned:
            return
        if lease.process is None:
            lease.transition(State.Failed)
            lease.transition(State.Cleaned)
            return
        self.cleanup(lease, lease.nonce)


class Child:
    """A direct child whose PID remains pinned until the supervisor reaps it."""
    def __init__(self, pid):
        self.pid = pid
        self.returncode = None

    def wait(self, timeout):
        deadline = time.monotonic() + timeout
        while self.returncode is None:
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.returncode = os.waitstatus_to_exitcode(status)
            elif time.monotonic() >= deadline:
                raise Diagnostic('ProcessIsolationLost')
            else:
                time.sleep(.01)
        return self.returncode
