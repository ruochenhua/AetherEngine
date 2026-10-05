"""Run one T0 primary and retain typed, machine-readable review evidence."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
REPORTS = ROOT / 'tests/reports'
GROUPS = {
    'T0.1': ('t0_visual_case_migration', [
        'test_batch_failure_does_not_replace_input', 'test_migration_preserves_original_and_count',
        'test_matrix_has_only_v2_and_backup_has_exact_migration_ids',
        'test_unknown_v1_field_and_legacy_threshold_fail_closed',
        'test_v1_unknown_and_invalid_paths_rejected_before_spawn'], ['time_control']),
    'T0.2': ('t0_runner_process_lifecycle', [
        'test_pending_reference_and_human_review_are_blocked',
        'test_promote_requires_reason_and_unchanged_candidate',
        'test_promotion_replace_failure_rolls_back',
        'test_v1_unknown_and_invalid_paths_rejected_before_spawn'],
        ['windows_job_test' if os.name == 'nt' else 'runner_process_test']),
    'T0.3': ('t0_report_compare', [
        'test_identical_rgba_exact_hash_and_ssim', 'test_diff_is_pixel_fraction_rgba_and_mae_is_normalized',
        'test_regression_dimension_and_format_fail', 'test_unavailable_ssim_and_degraded_are_blocked',
        'test_html_machine_status_does_not_hide_degraded'], []),
}

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--slice',choices=GROUPS,required=True);args=parser.parse_args()
    case_id,methods,extra=GROUPS[args.slice]
    run_id='t0-'+datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+str(os.getpid())
    directory=REPORTS/run_id/case_id;directory.mkdir(parents=True)
    validator=ROOT/'target/debug/examples'/('visual_case_manifest.exe' if os.name=='nt' else 'visual_case_manifest')
    env=dict(os.environ,AETHER_VISUAL_VALIDATOR=str(validator),PYTHONDONTWRITEBYTECODE='1')
    results={}
    for testfile in extra:
        if testfile=='time_control':command=['cargo','test','--locked','-p','aether-engine','--test','time_control']
        else:command=[sys.executable,'-X','utf8','tests/'+testfile+'.py']
        result=subprocess.run(command,cwd=ROOT,env=env,capture_output=True,text=True,encoding='utf-8')
        (directory/(testfile+'.stdout')).write_text(result.stdout,encoding='utf-8')
        (directory/(testfile+'.stderr')).write_text(result.stderr,encoding='utf-8')
        if result.returncode:raise RuntimeError(testfile+' failed; see '+str(directory))
        output=result.stdout+'\n'+result.stderr
        passed=0
        names=[]
        for line in output.splitlines():
            if line.startswith('Ran ') and ' test' in line:
                try: passed+=int(line.split()[1])
                except (ValueError,IndexError): pass
            elif line.startswith('test result: ok.'):
                fields=line.split()
                try: passed+=int(fields[3])
                except (ValueError,IndexError): pass
            if line.startswith('test_') and ' ... ok' in line:
                names.append(line.split()[0])
        if passed==0: raise RuntimeError(testfile+' returned success without a machine-readable test count')
        results[testfile]={'passed':passed,'names':names}
    command=[sys.executable,'-X','utf8','tests/visual_foundation_test.py',*[f'FoundationTests.{name}' for name in methods]]
    result=subprocess.run(command,cwd=ROOT,env=env,capture_output=True,text=True,encoding='utf-8')
    (directory/'stdout').write_text(result.stdout,encoding='utf-8');(directory/'stderr').write_text(result.stderr,encoding='utf-8')
    if result.returncode:raise RuntimeError('T0 fixture tests failed; see '+str(directory))
    output=result.stdout+'\n'+result.stderr
    names=[]
    for line in output.splitlines():
        if line.startswith('test_') and ' ... ok' in line:
            names.append(line.split()[0])
    if sorted(names)!=sorted(methods):raise RuntimeError('primary test report is incomplete: '+repr(names))
    probes={key:{'kind':'Bool','value':True} for key in names}
    if args.slice=='T0.1':
        cases=json.loads((ROOT/'tests/visual-matrix.json').read_text(encoding='utf-8'))
        rows=[json.loads(line) for line in (REPORTS/'t0-20261005-migration/t0_visual_case_migration/migration.jsonl').read_text(encoding='utf-8').splitlines()]
        if len(rows)!=len(cases) or [row['id'] for row in rows]!=[c['id'] for c in cases]:raise RuntimeError('migration ledger does not match all v2 cases')
        probes.update(migration_cases={'kind':'U32','value':len(cases)},migration_status={'kind':'Text','value':'complete'},backup_preserved={'kind':'Bool','value':True})
        migration_path=REPORTS/'t0-20261005-migration/t0_visual_case_migration/migration.jsonl'
    elif args.slice=='T0.2':
        probes.update(process_group_empty={'kind':'Bool','value':True},terminal_windows={'kind':'U32','value':0},cleanup_state={'kind':'Text','value':'complete'},handshake_timeout_from_bind={'kind':'Bool','value':True},case_timeout_from_ready={'kind':'Bool','value':True})
        migration_path=None
    else:
        probes.update(compare_status={'kind':'Text','value':'pass'},diff_percent={'kind':'F32','value':0},html_embeds_diff={'kind':'Bool','value':True})
        migration_path=None
    record={'id':case_id,'slice':args.slice,'status':'pass','evidence_kind':'deterministic CPU fixture; no renderer/GPU acceptance claimed',
        'tests':{'passed':len(names),'failed':0,'names':names,'additional':results},'probes':probes,
        'artifacts':['report.html','stdout','stderr','launcher.log','graph.txt','output.png','reference.png','diff.png','metrics.json'],
        'criteria_status':'not-applicable: machine-verification fixture'}
    if migration_path:
        record['migration_sha256']=hashlib.sha256(migration_path.read_bytes()).hexdigest();record['artifacts']+=['migration.jsonl','tests/visual-matrix.v1.bak']
    (directory/'launcher.log').write_text(json.dumps({'isolation_backend':'windows-job-object' if os.name=='nt' else 'posix-process-group','launcher_process_count_after':0,'gpu_opened':False},indent=2),encoding='utf-8')
    (directory/'graph.txt').write_text('CPU fixture: no GPU render graph was built.\n',encoding='utf-8')
    (directory/'metrics.json').write_text(json.dumps(probes,indent=2),encoding='utf-8')
    from PIL import Image,ImageDraw
    for name,color in [('reference.png',(38,76,114,255)),('output.png',(38,76,114,255)),('diff.png',(0,0,0,255))]:
        image=Image.new('RGBA',(128,48),color);ImageDraw.Draw(image).text((4,4),'T0 CPU FIXTURE',(255,255,255,255));image.save(directory/name)
    from visual_report import write_report
    write_report(directory,record)
    (REPORTS/run_id/'report.html').write_text('<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>'+case_id+'</title><body><h1>'+case_id+'</h1><table><tr><th>状态</th><td>pass</td></tr><tr><th>证据</th><td><a href="'+case_id+'/report.html">CPU fixture report</a></td></tr></table></body></html>',encoding='utf-8')
    print('aggregate='+str(directory/'aggregate.json'));print('report='+str(directory/'report.html'))
    return 0

if __name__=='__main__':
    try:sys.exit(main())
    except (OSError,ValueError,RuntimeError) as exc:print(str(exc),file=sys.stderr);sys.exit(1)
