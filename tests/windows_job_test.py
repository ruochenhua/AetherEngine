"""Real Windows job lifecycle tests, including descendants and lost identities."""
import ctypes
from dataclasses import replace
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts'))

@unittest.skipUnless(os.name=='nt','Windows backend')
class JobTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp=tempfile.TemporaryDirectory(prefix='aether-job-fixture-')
        cls.binary=Path(cls.tmp.name)/'fake launcher.exe'
        subprocess.run(['rustc',str(ROOT/'tests/fake_launcher_windows.rs'),'-o',str(cls.binary)],check=True)
        cls.addClassCleanup(cls.tmp.cleanup)

    def setUp(self):
        import process_lease,runner_process,windows_job
        self.api,self.runner,self.native=process_lease,runner_process,windows_job
        self.tmp=tempfile.TemporaryDirectory(prefix='aether-job-test-');self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name);self.env=dict(os.environ,FAKE_ROOT=str(self.root))

    def command(self,modes,*options):
        path=self.root/'cases.json'
        path.write_text(json.dumps([{'id':m,'scene':m,'launcher_args':['literal ; $(no shell)']} for m in modes]))
        return [sys.executable,'-X','utf8',str(ROOT/'scripts/runner_process.py'),'--launcher',str(self.binary),'--cases',str(path),'--report-dir',str(self.root/'reports'),'--handshake-timeout','2','--case-timeout','.5','--grace-period','.05',*options]

    def run_modes(self,modes,*options):
        r=subprocess.run(self.command(modes,*options),env=self.env,capture_output=True,text=True,timeout=20)
        return r

    def log(self,mode):return json.loads((self.root/'reports'/mode/'launcher.log').read_text())

    def assert_gone(self,r,mode):
        self.assertTrue(r['group_empty']);self.assertEqual(r['states'][-1],'Cleaned')
        opener=self.native.api('OpenProcess',[ctypes.c_ulong,ctypes.c_int,ctypes.c_ulong],ctypes.c_void_p)
        pids=[r['pid']]
        marker=self.root/(mode+'.descendant')
        if marker.exists():pids.append(int(marker.read_text()))
        for pid in pids:
            h=opener(0x100000,False,pid)
            if h:
                try:self.assertEqual(self.native.wait(h,0),0)
                finally:self.native.close(h)

    def test_success_and_exact_argv(self):
        r=self.run_modes(['ok']);self.assertEqual(r.returncode,0,r.stderr)
        rec=self.log('ok');self.assertEqual(rec['states'],['Reserved','Bound','Ready','Terminating','Cleaned'])
        self.assertEqual((self.root/'ok.argv').read_text().splitlines(),rec['argv'])
        self.assertEqual((self.root/'reports/ok/stdout').read_text(),'stdout-ok\n')
        self.assert_gone(rec,'ok')

    def test_handshake_timeout_and_serial_cleanup(self):
        r=self.run_modes(['hang','ok'],'--handshake-timeout','.4');self.assertEqual(r.returncode,1,r.stderr)
        first,last=self.log('hang'),self.log('ok')
        self.assertEqual(first['diagnostic'],'ChildHandshakeTimeout')
        self.assertLessEqual(first['cleaned_at'],last['spawned_at']);self.assert_gone(first,'hang');self.assert_gone(last,'ok')

    def test_panic_timeout_orphan_and_early(self):
        for mode,diagnostic,code in [('panic','ChildExit',17),('timeout','CaseTimeout',124),('orphan',None,0),('early','ChildExit',19),('signal','ChildExit',134)]:
            with self.subTest(mode=mode):
                self.run_modes([mode]);r=self.log(mode)
                self.assertEqual(r['diagnostic'],diagnostic);self.assertEqual(r['exit_code'],code);self.assert_gone(r,mode)
                shutil.rmtree(self.root/'reports')

    def test_case_timer_begins_at_ready(self):
        r=self.run_modes(['delayed']);self.assertEqual(r.returncode,0,r.stderr)
        rec=self.log('delayed');self.assertGreater(rec['ready_at']-rec['bound_at'],.55)
        self.assertLess(rec['cleaned_at']-rec['ready_at'],.5)

    def test_ready_exit_race(self):
        sup=self.api.Supervisor(.05);runner=self.runner.Runner(sup,2,2);directory=self.root/'reports';directory.mkdir()
        original=sup.exited
        def exited(lease):
            until=time.monotonic()+4
            while not original(lease):
                if time.monotonic()>until:raise AssertionError('fixture did not exit')
                time.sleep(.01)
            return True
        with mock.patch.dict(os.environ,self.env),mock.patch.object(sup,'exited',side_effect=exited):
            r=runner.run_case(str(self.binary),{'id':'ok','scene':'ok','launcher_args':[]},directory)
        self.assertIsNone(r['diagnostic']);self.assert_gone(r,'ok')

    def test_nonce_identity_race_duplicate_cleanup(self):
        sup=self.api.Supervisor(.05);lease=self.api.ProvisionalLease(str(self.binary))
        with open(os.devnull,'wb') as sink:
            sup.spawn(lease,[str(self.binary),'--scene','timeout'],sink,sink,dict(self.env,AETHER_READY_PATH=str(self.root/'ready')),None)
        try:
            identity=lease.identity
            for nonce in ['wrong']:
                with self.assertRaisesRegex(self.api.Diagnostic,'ProcessIsolationLost'):sup.cleanup(lease,nonce)
            for field in ['start_time','pgid','pid']:
                values={field:'stale' if field=='start_time' else getattr(identity,field)+1}
                lease.identity=replace(identity,**values)
                with mock.patch.object(self.native,'terminate') as kill:
                    with self.assertRaisesRegex(self.api.Diagnostic,'ProcessIsolationLost'):sup.cleanup(lease,lease.nonce)
                    kill.assert_not_called()
            lease.identity=identity;sup.cleanup(lease,lease.nonce)
            with self.assertRaisesRegex(self.api.Diagnostic,'AlreadyCleaned'):sup.cleanup(lease,lease.nonce)
            sup.ensure_cleaned(lease)
        finally:
            lease.identity=identity;sup.ensure_cleaned(lease)

    def test_bind_failure_never_executes(self):
        sup=self.api.Supervisor(.05);lease=self.api.ProvisionalLease(str(self.binary))
        with open(os.devnull,'wb') as sink,mock.patch.object(lease,'bind',side_effect=self.api.Diagnostic('BindFailure')):
            with self.assertRaisesRegex(self.api.Diagnostic,'BindFailure'):
                sup.spawn(lease,[str(self.binary),'--scene','ok'],sink,sink,self.env,None)
        self.assertFalse((self.root/'ok.started').exists());self.assertEqual(lease.state,self.api.State.Cleaned)

    def test_spawn_failure(self):
        sup=self.api.Supervisor(.05);lease=self.api.ProvisionalLease(str(self.root/'missing.exe'))
        with open(os.devnull,'wb') as sink:
            with self.assertRaisesRegex(self.api.Diagnostic,'SpawnFailure'):sup.spawn(lease,[lease.expected_binary],sink,sink,self.env,None)
        self.assertEqual(lease.state,self.api.State.Cleaned)

    def test_isolation_setup_failure_never_executes(self):
        sup=self.api.Supervisor(.05);lease=self.api.ProvisionalLease(str(self.binary))
        with open(os.devnull,'wb') as sink,mock.patch.object(self.native,'spawn_suspended',side_effect=OSError('not supported')):
            with self.assertRaises(OSError):sup.spawn(lease,[str(self.binary),'--scene','ok'],sink,sink,self.env,None)
        self.assertFalse((self.root/'ok.started').exists());self.assertEqual(lease.state,self.api.State.Cleaned)

    def test_supervisor_crash_closes_job(self):
        p=subprocess.Popen(self.command(['timeout','ok'],'--case-timeout','8'),env=self.env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
        try:
            deadline=time.monotonic()+5;marker=self.root/'timeout.descendant'
            while not marker.exists() and time.monotonic()<deadline:time.sleep(.01)
            self.assertTrue(marker.exists());pid=int(marker.read_text());p.kill();p.communicate(timeout=5)
            opener=self.native.api('OpenProcess',[ctypes.c_ulong,ctypes.c_int,ctypes.c_ulong],ctypes.c_void_p)
            h=opener(0x100000,False,pid)
            if h:
                try:self.assertEqual(self.native.wait(h,5000),0)
                finally:self.native.close(h)
            self.assertFalse((self.root/'ok.started').exists())
        finally:
            if p.poll() is None:p.kill();p.wait()
            p.stderr.close()

    def test_selection_preserves_manifest_order(self):
        r=self.run_modes(['first','excluded','last'],'--case','last','--case','first')
        self.assertEqual(r.returncode,0,r.stderr);self.assertFalse((self.root/'excluded.started').exists())
        self.assertLessEqual(self.log('first')['cleaned_at'],self.log('last')['spawned_at'])

if __name__=='__main__':unittest.main(verbosity=2)
