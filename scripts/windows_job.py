"""Windows identity-bound jobs. No PID scan or shell-based termination.

The job is assigned atomically by PROC_THREAD_ATTRIBUTE_JOB_LIST at creation;
the suspended child cannot execute before bind. The private, non-inherited job
handle has KILL_ON_JOB_CLOSE, including when the supervisor exits unexpectedly.
"""
import ctypes as C
from ctypes import wintypes as W
import msvcrt
import os
import subprocess
import time

from process_lease import Diagnostic, Identity, State

K = C.WinDLL('kernel32', use_last_error=True)
SIZE = C.c_size_t
HANDLE = W.HANDLE

class Limits(C.Structure):
    _fields_ = [('user', C.c_longlong), ('job', C.c_longlong), ('flags', W.DWORD),
                ('min', SIZE), ('max', SIZE), ('active', W.DWORD), ('affinity', SIZE),
                ('priority', W.DWORD), ('class_', W.DWORD)]

class ExtendedLimits(C.Structure):
    _fields_ = [('basic', Limits), ('io', C.c_ulonglong * 6),
                ('process_memory', SIZE), ('job_memory', SIZE),
                ('peak_process', SIZE), ('peak_job', SIZE)]

class Accounting(C.Structure):
    _fields_ = [('times', C.c_longlong * 4), ('faults', W.DWORD),
                ('total', W.DWORD), ('active', W.DWORD), ('terminated', W.DWORD)]

class Startup(C.Structure):
    _fields_ = [('cb', W.DWORD), ('reserved', W.LPWSTR), ('desktop', W.LPWSTR),
                ('title', W.LPWSTR), ('x', W.DWORD), ('y', W.DWORD),
                ('xsize', W.DWORD), ('ysize', W.DWORD), ('xchars', W.DWORD),
                ('ychars', W.DWORD), ('fill', W.DWORD), ('flags', W.DWORD),
                ('show', W.WORD), ('reserved_size', W.WORD), ('reserved2', C.c_void_p),
                ('stdin', HANDLE), ('stdout', HANDLE), ('stderr', HANDLE)]

class StartupEx(C.Structure):
    _fields_ = [('info', Startup), ('attributes', C.c_void_p)]

class ProcessInfo(C.Structure):
    _fields_ = [('process', HANDLE), ('thread', HANDLE), ('pid', W.DWORD), ('tid', W.DWORD)]

def api(name, args, result=W.BOOL):
    function = getattr(K, name)
    function.argtypes, function.restype = args, result
    return function

close = api('CloseHandle', [HANDLE])
create_job = api('CreateJobObjectW', [C.c_void_p, W.LPCWSTR], HANDLE)
set_job = api('SetInformationJobObject', [HANDLE, C.c_int, C.c_void_p, W.DWORD])
query_job = api('QueryInformationJobObject', [HANDLE, C.c_int, C.c_void_p, W.DWORD, C.c_void_p])
in_job = api('IsProcessInJob', [HANDLE, HANDLE, C.POINTER(W.BOOL)])
terminate = api('TerminateJobObject', [HANDLE, W.UINT])
wait = api('WaitForSingleObject', [HANDLE, W.DWORD], W.DWORD)
exit_code = api('GetExitCodeProcess', [HANDLE, C.POINTER(W.DWORD)])
process_times = api('GetProcessTimes', [HANDLE] + [C.POINTER(W.FILETIME)] * 4)
image_name = api('QueryFullProcessImageNameW', [HANDLE, W.DWORD, W.LPWSTR, C.POINTER(W.DWORD)])
resume = api('ResumeThread', [HANDLE], W.DWORD)
init_attributes = api('InitializeProcThreadAttributeList', [C.c_void_p, W.DWORD, W.DWORD, C.POINTER(SIZE)])
set_attribute = api('UpdateProcThreadAttribute', [C.c_void_p, W.DWORD, SIZE, C.c_void_p, SIZE, C.c_void_p, C.c_void_p])
delete_attributes = api('DeleteProcThreadAttributeList', [C.c_void_p], None)
create_process = api('CreateProcessW', [W.LPCWSTR, W.LPWSTR, C.c_void_p, C.c_void_p,
    W.BOOL, W.DWORD, C.c_void_p, W.LPCWSTR, C.POINTER(StartupEx), C.POINTER(ProcessInfo)])

def checked(ok):
    if not ok:
        raise C.WinError(C.get_last_error())
    return ok

def make_job():
    handle = checked(create_job(None, None))
    limits = ExtendedLimits()
    limits.basic.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    try:
        checked(set_job(handle, 9, C.byref(limits), C.sizeof(limits)))
    except Exception:
        close(handle)
        raise
    return handle

def supported():
    try:
        close(make_job())
        return True
    except OSError:
        return False

def active_count(handle):
    data = Accounting()
    checked(query_job(handle, 1, C.byref(data), C.sizeof(data), None))
    return data.active

class Child:
    def __init__(self, info, job):
        self.pid, self.handle, self.job = info.pid, info.process, job
        self.returncode = None

    def identity(self):
        values = [W.FILETIME() for _ in range(4)]
        checked(process_times(self.handle, *(C.byref(v) for v in values)))
        creation = (values[0].dwHighDateTime << 32) | values[0].dwLowDateTime
        return Identity(self.pid, str(creation), self.pid)

    def validate(self, lease, nonce):
        belongs = W.BOOL()
        try:
            checked(in_job(self.handle, self.job, C.byref(belongs)))
            if nonce != lease.nonce or lease.identity != self.identity() or not belongs.value:
                raise Diagnostic('ProcessIsolationLost')
        except OSError as exc:
            raise Diagnostic('ProcessIsolationLost') from exc

    def wait(self, timeout):
        if wait(self.handle, round(timeout * 1000)) != 0:
            raise Diagnostic('ProcessIsolationLost')
        value = W.DWORD()
        checked(exit_code(self.handle, C.byref(value)))
        self.returncode = value.value
        return self.returncode

def spawn_suspended(job, argv, stdout, stderr, env):
    size = SIZE()
    init_attributes(None, 2, 0, C.byref(size))
    attributes = C.create_string_buffer(size.value)
    checked(init_attributes(attributes, 2, 0, C.byref(size)))
    info = ProcessInfo()
    with open(os.devnull, 'rb') as stdin:
        handles = [msvcrt.get_osfhandle(f.fileno()) for f in (stdin, stdout, stderr)]
        old = [os.get_handle_inheritable(h) for h in handles]
        try:
            for h in handles: os.set_handle_inheritable(h, True)
            unique = list(dict.fromkeys(handles))
            inherited = (HANDLE * len(unique))(*unique)
            jobs = (HANDLE * 1)(job)
            checked(set_attribute(attributes, 0, 0x20002, inherited, C.sizeof(inherited), None, None))
            checked(set_attribute(attributes, 0, 0x2000D, jobs, C.sizeof(jobs), None, None))
            startup = StartupEx()
            startup.info.cb = C.sizeof(startup)
            startup.info.flags = 0x101  # USESTDHANDLES | USESHOWWINDOW
            startup.info.show = 0
            startup.info.stdin, startup.info.stdout, startup.info.stderr = handles
            startup.attributes = C.cast(attributes, C.c_void_p)
            environment = C.create_unicode_buffer('\0'.join(f'{k}={v}' for k,v in sorted(env.items(), key=lambda p:p[0].upper())) + '\0\0')
            command = C.create_unicode_buffer(subprocess.list2cmdline(argv))
            flags = 0x80000 | 0x8000000 | 0x400 | 4  # EXTENDED | NO_WINDOW | UNICODE_ENV | SUSPENDED
            checked(create_process(argv[0], command, None, None, True, flags, environment,
                                   None, C.byref(startup), C.byref(info)))
        finally:
            delete_attributes(attributes)
            for h, value in zip(handles, old): os.set_handle_inheritable(h, value)
    return info

class Supervisor:
    def __init__(self, grace_period=5):
        if not supported(): raise Diagnostic('UnsupportedProcessIsolation')
        self.grace_period = grace_period

    def spawn(self, lease, argv, stdout, stderr, env, ready_fd):
        if lease.state != State.Reserved or lease.process is not None:
            raise Diagnostic('IllegalTransition')
        job, info = None, None
        try:
            if argv[0] != lease.expected_binary: raise Diagnostic('ProcessIsolationLost')
            if not os.path.isfile(argv[0]): raise Diagnostic('SpawnFailure')
            job = make_job()
            info = spawn_suspended(job, argv, stdout, stderr, env)
            lease.process = Child(info, job)
            length = W.DWORD(32768); binary = C.create_unicode_buffer(length.value)
            checked(image_name(info.process, 0, binary, C.byref(length)))
            if os.path.normcase(os.path.realpath(binary.value)) != os.path.normcase(os.path.realpath(argv[0])):
                raise Diagnostic('ProcessIsolationLost')
            lease.bind(lease.process.identity(), lease.nonce)
            if resume(info.thread) == 0xFFFFFFFF: raise C.WinError(C.get_last_error())
        except Exception:
            if job:
                terminate(job, 125)
                if lease.process: lease.process.wait(2); close(lease.process.handle)
                close(job)
            if lease.state == State.Reserved: lease.transition(State.Failed)
            elif lease.state == State.Bound: lease.transition(State.Terminating)
            lease.transition(State.Cleaned)
            lease.group_empty = True
            raise
        finally:
            if info: close(info.thread)

    def exited(self, lease):
        lease.validate(lease.nonce)
        return wait(lease.process.handle, 0) == 0

    def live_members(self, lease):
        lease.validate(lease.nonce)
        return active_count(lease.process.job)

    def cleanup(self, lease, nonce):
        with lease.lock:
            if lease.state == State.Cleaned: raise Diagnostic('AlreadyCleaned')
            lease.validate(nonce)
            if lease.state not in (State.Terminating, State.Failed): lease.transition(State.Terminating)
            deadline = time.monotonic() + self.grace_period
            while active_count(lease.process.job) and time.monotonic() < deadline:
                time.sleep(.01)
            if active_count(lease.process.job):
                lease.validate(nonce)
                checked(terminate(lease.process.job, 124))
                lease.signals.append({'signal':'TerminateJobObject','pgid':lease.identity.pgid,'at':time.monotonic()})
            deadline = time.monotonic() + 5
            while active_count(lease.process.job):
                if time.monotonic() >= deadline: raise Diagnostic('ProcessIsolationLost')
                time.sleep(.01)
            lease.process.wait(2)
            close(lease.process.handle)
            close(lease.process.job)
            lease.group_empty = True
            lease.cleaned_at = time.monotonic()
            lease.transition(State.Cleaned)

    def ensure_cleaned(self, lease):
        if lease.state == State.Cleaned: return
        if lease.process is None:
            lease.transition(State.Failed); lease.transition(State.Cleaned)
            return
        self.cleanup(lease, lease.nonce)
