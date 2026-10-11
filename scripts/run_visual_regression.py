"""v2 regression coordinator. Pending/degraded/review states never return pass."""
import argparse
import atexit
from datetime import datetime,timezone
import hashlib
import html
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

from process_lease import Supervisor
from runner_process import Runner
from visual_manifest import ROOT,execution_cases,validate_case
from visual_compare import compare,reference_valid
from visual_report import write_report

def stamp():return datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')

def device_driver_label(device):
    driver=' '.join(part for part in (device.get('driver'),device.get('driver_info')) if isinstance(part,str) and part.strip())
    if driver:return driver
    backend=device.get('backend')
    return f'{backend} backend (driver version unavailable)' if isinstance(backend,str) and backend.strip() else None

def launcher_binary(root):
    if os.environ.get('AETHER_LAUNCHER_BIN'):return str(Path(os.environ['AETHER_LAUNCHER_BIN']).resolve())
    subprocess.run(['cargo','build','--locked','-p','aether-launcher','--release'],cwd=root,check=True)
    target=json.loads(subprocess.check_output(['cargo','metadata','--no-deps','--format-version','1'],cwd=root))['target_directory']
    return str(Path(target)/'release'/('aether-launcher.exe' if os.name=='nt' else 'aether-launcher'))

def evaluate(case,directory,run_record,root,capture_reference=False,graph_output=False):
    visual=case['visual_case'];directory=Path(directory)
    record={'id':case['id'],'status':'error','case':visual,'launcher':run_record,'criteria_status':'needs-review','metrics':None}
    if graph_output:
        graph_path=directory/'render-graph.json'
        try:
            graph=json.loads(graph_path.read_text(encoding='utf-8'))
            if graph.get('schema_version')!=1 or not isinstance(graph.get('passes'),list) or not isinstance(graph.get('resource_edges'),list):
                raise ValueError('invalid render graph artifact schema')
            record['graph_artifact']={'path':graph_path.name,'pass_count':len(graph['passes']),'resource_edge_count':len(graph['resource_edges'])}
            (directory/'graph.txt').write_text('render graph artifact: '+graph_path.name+'\n'+json.dumps(graph,indent=2)+'\n',encoding='utf-8')
        except (OSError,ValueError,json.JSONDecodeError) as exc:
            record['error']='InvalidRenderGraphArtifact: '+str(exc)
    else:
        # Full render graph inspection is not substituted by a pass-name list.
        (directory/'graph.txt').write_text('GPU resource graph unavailable; no graph predicate is declared for this matrix case.\n',encoding='utf-8')
    if 'error' in record:
        pass
    elif run_record['diagnostic'] is not None or not run_record['group_empty']:
        record['error']=run_record['diagnostic'] or 'orphan_process'
    elif not (directory/'output.png').is_file():record['error']='MissingImage'
    elif capture_reference:
        device=run_record['metadata']['device']
        driver_label=device_driver_label(device) if isinstance(device,dict) else None
        if not isinstance(device,dict) or not all(device.get(k) for k in ['adapter','os','runner_version']) or not driver_label:
            record['error']='IncompleteDeviceProvenance'
        else:
            commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
            ref=dict(visual['reference'],sha256=hashlib.sha256((directory/'output.png').read_bytes()).hexdigest(),source_commit=commit,
                os=device['os'],adapter=device['adapter'],driver=driver_label,runner_version=device['runner_version'],
                width=visual['render']['width'],height=visual['render']['height'],format='png-rgba8')
            record.update(status='reference-pending',candidate_reference=ref)
            (directory/'candidate.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
    else:
        index_path=Path(root)/'tests/reference-manifest.json'
        index=json.loads(index_path.read_text(encoding='utf-8')) if index_path.exists() else {}
        if visual['id'] in index:visual=dict(visual,reference=index[visual['id']]);record['case']=visual
        try:
            validate_case(visual,root)
            validity=reference_valid(visual,root)
            if validity!='valid':record['status']=validity
            else:
                shutil.copyfile(Path(root)/visual['reference']['path'],directory/'reference.png')
                metrics=compare(directory/'reference.png',directory/'output.png',visual['compare'],directory/'diff.png')
                record['metrics']=metrics
                record['status']=metrics['status'] if metrics['status']!='pass' else 'needs-review'
        except (ValueError,OSError) as exc:record['error']=str(exc)
    (directory/'metrics.json').write_text(json.dumps(record['metrics'],indent=2),encoding='utf-8')
    write_report(directory,record)
    return record

def run(matrix,ids,run_dir,launcher,root=ROOT,capture_reference=False,settle=0,graph_output=False):
    run_dir=Path(run_dir)
    cases=execution_cases(matrix,run_dir,root)
    known={c['id'] for c in cases}
    if ids and not set(ids)<=known:raise ValueError('unknown case ID')
    cases=[c for c in cases if not ids or c['id'] in ids]
    if graph_output:
        for case in cases:
            case['launcher_args'] += ['--render-graph-output',str(run_dir/case['id']/'render-graph.json')]
    # All schema/path/platform checks happen before creating a report directory.
    supervisor=Supervisor(float(os.environ.get('AETHER_TERMINATION_GRACE','5')))
    runner=Runner(supervisor,float(os.environ.get('AETHER_HANDSHAKE_TIMEOUT','2')),float(os.environ.get('AETHER_CASE_TIMEOUT','120')))
    run_dir.mkdir(parents=True,exist_ok=False)
    atexit.register(runner.cleanup)
    old={sig:signal.signal(sig,runner.cancel) for sig in (signal.SIGINT,signal.SIGTERM)}
    records=[]
    try:
        for case in cases:
            if runner.cancelled:break
            rec=runner.run_case(launcher,case,run_dir)
            records.append(evaluate(case,run_dir/case['id'],rec,root,capture_reference,graph_output))
            if rec['diagnostic']=='ProcessIsolationLost':break
            until=time.monotonic()+settle
            while not runner.cancelled and time.monotonic()<until:time.sleep(min(.05,max(0,until-time.monotonic())))
    finally:
        runner.cleanup();atexit.unregister(runner.cleanup)
        for sig,handler in old.items():signal.signal(sig,handler)
    return records,runner.cancelled

def capture_exit_code(records,cancelled,capture_reference):
    if cancelled:return 128+cancelled
    if not records:return 1
    accepted_status='reference-pending' if capture_reference else 'pass'
    return int(any(record['status']!=accepted_status for record in records))

def main():
    p=argparse.ArgumentParser();p.add_argument('--scene',action='append',default=[]);p.add_argument('--report');p.add_argument('--matrix',default='tests/visual-matrix.json')
    p.add_argument('--capture-reference',action='store_true',help='capture provenance-checked candidates without promoting them')
    p.add_argument('--update-references',action='store_true');p.add_argument('--reason','--reference-reason');a=p.parse_args()
    if a.update_references:raise ValueError('Reference updates require separate --capture-reference and --promote --reason commands')
    if a.reason:raise ValueError('--reason is only accepted during explicit promotion')
    name=a.report or 'visual-'+stamp();
    if not name or Path(name).name!=name:raise ValueError('invalid report name')
    run_id=name+'-'+str(os.getpid());run_dir=ROOT/'tests/reports'/run_id
    records,cancelled=run(a.matrix,a.scene,run_dir,launcher_binary(ROOT),capture_reference=a.capture_reference,settle=float(os.environ.get('AETHER_REGRESSION_SETTLE_SECONDS','0')))
    rows=''.join('<tr><td>'+html.escape(r['id'])+'</td><td>'+html.escape(r['status'])+'</td><td><a href="'+run_id+'/'+html.escape(r['id'])+'/report.html">report</a></td></tr>' for r in records)
    notice='<p>Candidate capture only: images remain unapproved until a human reviews and explicitly promotes them.</p>' if a.capture_reference else ''
    report=ROOT/'tests/reports'/(name+'.html');report.write_text('<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>Visual regression v2</title><body><h1>Visual regression v2</h1>'+notice+'<table><tr><th>Case</th><th>Status</th><th>Evidence</th></tr>'+rows+'</table></body></html>',encoding='utf-8')
    print('report='+str(report));print('run_id='+run_id)
    return capture_exit_code(records,cancelled,a.capture_reference)

if __name__=='__main__':
    try:sys.exit(main())
    except (ValueError,OSError,RuntimeError) as exc:print(str(exc),file=sys.stderr);sys.exit(1)
