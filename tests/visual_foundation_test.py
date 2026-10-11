"""Migration, strict comparison, candidate promotion and real v2 runner tests."""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import numpy as np
from PIL import Image

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts'))
import migrate_visual_cases as migration
import visual_compare as comparator
import visual_manifest as manifest
from run_visual_regression import capture_exit_code,evaluate,run
from run_primary_case import promote
from visual_report import write_report

CONFIG={'algorithm':'rgba8_normalized','exact_hash':False,'allow_degraded_compare':False,'ssim_min':.99,'mae_max':.01,'diff_percent_max':.01}

class FoundationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.compiled=manifest.validator()
        cls.tmp=tempfile.TemporaryDirectory(prefix='aether-v2-launcher-');cls.addClassCleanup(cls.tmp.cleanup)
        cls.fake=Path(cls.tmp.name)/('fake.exe' if os.name=='nt' else 'fake')
        if os.name=='nt':subprocess.run(['rustc',str(ROOT/'tests/fake_launcher_windows.rs'),'-o',str(cls.fake)],check=True)
        else:
            cls.fake.write_text('#!'+sys.executable+'\n'+'''import json,os,sys,shutil
from pathlib import Path
os.write(int(os.environ['AETHER_READY_FD']),b'R')
if os.environ.get('FAKE_FAIL'):sys.exit(17)
if '--screenshot' in sys.argv and not os.environ.get('FAKE_NO_IMG'):
    shutil.copyfile(os.environ['FAKE_CAPTURE'],sys.argv[sys.argv.index('--screenshot')+1])
if os.environ.get('FAKE_DEVICE'):shutil.copyfile(os.environ['FAKE_DEVICE'],os.environ['AETHER_DEVICE_METADATA_PATH'])
''');cls.fake.chmod(0o755)

    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='aether-v2-test-');self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
        (self.root/'scenes').mkdir();(self.root/'tests/reference').mkdir(parents=True)
        (self.root/'scenes/ok.ron').write_text('fixture')
        self.case=migration.convert({'scenes':[{'name':'ok','scene':'scenes/ok.ron','frames':1,'width':16,'height':16}]},self.root)[0][0]
        self.matrix=self.root/'tests/matrix.json';self.matrix.write_text(json.dumps([self.case]))
        self.image=self.root/'image.png';Image.new('RGBA',(16,16),(24,48,72,255)).save(self.image)
        device=self.root/'device.json';device.write_text(json.dumps({'adapter':'CPU process fixture','driver':'software-test','os':sys.platform,'runner_version':'v2-fixture'}))
        self.env=dict(os.environ,AETHER_VISUAL_VALIDATOR=str(self.compiled),FAKE_ROOT=str(self.root),FAKE_CAPTURE=str(self.image),FAKE_DEVICE=str(device),AETHER_TERMINATION_GRACE='.05')
        subprocess.run(['git','init','-q',str(self.root)],check=True)
        subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','--allow-empty','-qm','fixture'],cwd=self.root,check=True)

    def run_case(self,name='capture',capture=False):
        with mock.patch.dict(os.environ,self.env):
            records,cancelled=run(self.matrix,['ok'],self.root/name,str(self.fake),self.root,capture)
        self.assertEqual(cancelled,0);return records[0],self.root/name/'ok'

    def test_identical_rgba_exact_hash_and_ssim(self):
        r=comparator.compare(self.image,self.image,dict(CONFIG,exact_hash=True),self.root/'diff.png')
        self.assertEqual(r['status'],'pass');self.assertEqual(r['ssim'],1);self.assertEqual(r['mae'],0);self.assertEqual(r['diff_percent'],0);self.assertEqual(r['rgba_hashes'][0],r['rgba_hashes'][1])

    def test_diff_is_pixel_fraction_rgba_and_mae_is_normalized(self):
        changed=np.array(Image.open(self.image));changed[0,0,3]=253
        output=self.root/'changed.png';Image.fromarray(changed,'RGBA').save(output)
        r=comparator.compare(self.image,output,CONFIG,self.root/'diff.png')
        self.assertAlmostEqual(r['diff_percent'],1/256);self.assertAlmostEqual(r['mae'],2/(255*16*16*4))
        changed[0,0,3]=254;Image.fromarray(changed,'RGBA').save(output)
        r=comparator.compare(self.image,output,CONFIG,self.root/'diff.png');self.assertEqual(r['diff_percent'],0)

    def test_regression_dimension_and_format_fail(self):
        output=self.root/'bad.png';Image.new('RGBA',(16,16),(255,255,255,255)).save(output)
        self.assertEqual(comparator.compare(self.image,output,CONFIG,self.root/'diff.png')['status'],'regression')
        Image.new('RGBA',(17,16)).save(output)
        with self.assertRaisesRegex(ValueError,'DimensionMismatch'):comparator.compare(self.image,output,CONFIG,self.root/'diff.png')
        Image.new('RGB',(16,16)).save(output)
        with self.assertRaisesRegex(ValueError,'FormatMismatch'):comparator.compare(self.image,output,CONFIG,self.root/'diff.png')

    def test_unavailable_ssim_and_degraded_are_blocked(self):
        small=self.root/'small.png';Image.new('RGBA',(4,4),(0,0,0,255)).save(small)
        self.assertEqual(comparator.compare(small,small,CONFIG,self.root/'diff.png')['status'],'error')
        self.assertEqual(comparator.compare(small,small,dict(CONFIG,allow_degraded_compare=True),self.root/'diff.png')['status'],'degraded')

    def test_migration_preserves_original_and_count(self):
        source=self.root/'tests/visual-matrix.json';old={'defaults':{'freeze_time':True},'scenes':[{'name':'ok','scene':'scenes/ok.ron','frames':1,'threshold':.995}]}
        original=json.dumps(old).encode();source.write_bytes(original)
        with mock.patch.dict(os.environ,self.env):count=migration.migrate(source,self.root/'tests/v2.json',self.root/'migration.jsonl',self.root)
        self.assertEqual(count,1);self.assertEqual(source.with_suffix('.v1.bak').read_bytes(),original)
        data=json.loads(source.read_text());self.assertEqual(data[0]['compare']['ssim_min'],.995);self.assertEqual(data[0]['time']['simulation_time'],0)
        self.assertIsNone(data[0]['reference']['sha256']);self.assertEqual(json.loads((self.root/'migration.jsonl').read_text())['status'],'migrated')

    def test_batch_failure_does_not_replace_input(self):
        source=self.root/'tests/visual-matrix.json';source.write_text(json.dumps({'scenes':[{'name':'ok','scene':'scenes/ok.ron'},{'name':'escape','scene':'../secret'}]}));old=source.read_bytes()
        with mock.patch.dict(os.environ,self.env),self.assertRaises(ValueError):migration.migrate(source,self.root/'tests/v2.json',self.root/'migration.jsonl',self.root)
        self.assertEqual(source.read_bytes(),old);self.assertFalse(source.with_suffix('.v1.bak').exists());self.assertEqual(json.loads((self.root/'migration.jsonl').read_text())['status'],'failed')

    def test_unknown_v1_field_and_legacy_threshold_fail_closed(self):
        for field in [{'unknown':True},{'threshold':.95}]:
            source=self.root/'tests/legacy.json';source.write_text(json.dumps({'scenes':[dict(name='ok',scene='scenes/ok.ron',**field)]}));old=source.read_bytes()
            with mock.patch.dict(os.environ,self.env),self.assertRaises(ValueError):migration.migrate(source,self.root/'tests/v2.json',self.root/'log',self.root)
            self.assertEqual(source.read_bytes(),old)
            (self.root/'tests/v2.json.pending').unlink(missing_ok=True)

    def test_matrix_has_only_v2_and_backup_has_exact_migration_ids(self):
        data=json.loads((ROOT/'tests/visual-matrix.json').read_text());old=json.loads((ROOT/'tests/visual-matrix.v1.bak').read_text())
        current_ids=[c['id'] for c in data];migrated_ids=[c['name'] for c in old['scenes']]
        self.assertEqual(len(migrated_ids),26);self.assertTrue(set(migrated_ids).issubset(current_ids))
        self.assertEqual(len(current_ids),len(set(current_ids)))
        self.assertTrue(all(c['manifest_version']==2 for c in data))
        self.assertEqual(len(manifest.materialize(ROOT/'tests/visual-matrix.json')),len(data))

    def test_v1_unknown_and_invalid_paths_rejected_before_spawn(self):
        for value in [{'scenes':[]},[dict(self.case,unknown=True)],[dict(self.case,scene='scenes/../secret')]]:
            self.matrix.write_text(json.dumps(value))
            with mock.patch.dict(os.environ,self.env),self.assertRaises(ValueError):self.run_case()
            self.assertFalse((self.root/'capture').exists())

    def test_pending_reference_and_human_review_are_blocked(self):
        r,d=self.run_case();self.assertEqual(r['status'],'reference-pending');self.assertFalse((self.root/'tests/reference/ok.png').exists())
        r,d=self.run_case('candidate',True);self.assertEqual(r['status'],'reference-pending')
        with mock.patch.dict(os.environ,self.env):promote(d,self.root,'ok','fixture review only')
        r,d=self.run_case('comparison');self.assertEqual(r['metrics']['status'],'pass');self.assertEqual(r['status'],'needs-review')
        text=(d/'report.html').read_text();self.assertIn('reference.png',text);self.assertIn('output.png',text);self.assertIn('diff.png',text);self.assertIn('launcher.log',text);self.assertIn('graph.txt',text)

    def test_promote_requires_reason_and_unchanged_candidate(self):
        r,d=self.run_case(capture=True)
        with self.assertRaises(ValueError):promote(d,self.root,'ok','')
        (d/'output.png').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError,'candidate image changed'):promote(d,self.root,'ok','review')
        self.assertFalse((self.root/'tests/reference-manifest.json').exists())

    def test_promotion_replace_failure_rolls_back(self):
        r,d=self.run_case(capture=True);reference=self.root/'tests/reference/ok.png';reference.write_bytes(b'old')
        actual=os.replace;calls=[]
        def replace(a,b):
            calls.append((a,b))
            if len(calls)==2:raise OSError('replace failed')
            return actual(a,b)
        with mock.patch.dict(os.environ,self.env),mock.patch('run_primary_case.os.replace',side_effect=replace),self.assertRaises(OSError):promote(d,self.root,'ok','review')
        self.assertEqual(reference.read_bytes(),b'old');self.assertFalse((self.root/'tests/reference-manifest.json').exists())

    def test_missing_metadata_cannot_make_candidate(self):
        with mock.patch.dict(os.environ,self.env):
            cases=manifest.execution_cases(self.matrix,self.root/'unused',self.root)
        directory=self.root/'output';directory.mkdir();shutil.copyfile(self.image,directory/'output.png')
        record={'diagnostic':None,'group_empty':True,'metadata':{'device':'not queried'},'exit_code':0}
        r=evaluate(cases[0],directory,record,self.root,True);self.assertEqual(r['status'],'error');self.assertFalse((directory/'candidate.json').exists())

    def test_metal_backend_with_unavailable_driver_version_keeps_truthful_candidate_provenance(self):
        device=self.root/'device.json'
        device.write_text(json.dumps({'adapter':'Apple GPU','backend':'Metal','driver':'','driver_info':'','os':'macos','runner_version':'v2-fixture'}))
        record,d=self.run_case('metal-candidate',capture=True)
        candidate=json.loads((d/'candidate.json').read_text())
        self.assertEqual(record['status'],'reference-pending')
        self.assertEqual(candidate['candidate_reference']['driver'],'Metal backend (driver version unavailable)')
        self.assertEqual(candidate['candidate_reference']['adapter'],'Apple GPU')

    def test_html_machine_status_does_not_hide_degraded(self):
        write_report(self.root,{'id':'control','status':'degraded','evidence_kind':'CPU fixture'})
        self.assertIn('degraded',(self.root/'report.html').read_text());self.assertEqual(json.loads((self.root/'aggregate.json').read_text())['status'],'degraded')

    def test_candidate_batch_exit_code_accepts_capture_and_rejects_runtime_errors(self):
        self.assertEqual(capture_exit_code([{'status':'reference-pending'}],False,True),0)
        self.assertEqual(capture_exit_code([{'status':'error'}],False,True),1)
        self.assertEqual(capture_exit_code([],False,True),1)
        self.assertEqual(capture_exit_code([],17,True),145)
        self.assertEqual(capture_exit_code([{'status':'reference-pending'}],False,False),1)

    def test_variants_get_distinct_files_and_expectations(self):
        self.case['variants']=[{'id':name,'launcher_args_append':['--particles',state],'time_override':None,'camera_override':None,'benchmark_override':None,'expected_result':{'kind':'Render','diagnostics':[],'fallbacks':[],'metrics':[],'hashes':[],'probes':[],'graph':None}} for name,state in [('on','on'),('off','off')]]
        self.matrix.write_text(json.dumps([self.case]))
        with mock.patch.dict(os.environ,self.env):cases=manifest.execution_cases(self.matrix,self.root/'outputs',self.root)
        self.assertEqual([c['id'] for c in cases],['ok__on','ok__off']);self.assertNotEqual(cases[0]['output'],cases[1]['output'])
        self.assertEqual(cases[0]['visual_case']['expected_result']['kind'],'Render')

if __name__=='__main__':unittest.main(verbosity=2)
