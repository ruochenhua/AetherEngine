"""Transactional v1-to-v2 migration; missing reference provenance stays pending."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys

from visual_manifest import materialize

REFERENCE_FIELDS = ['sha256','source_commit','os','driver','adapter','runner_version','width','height','format']

def convert(data, root):
    if set(data)-{'defaults','scenes'} or not isinstance(data.get('scenes'),list):
        raise ValueError('invalid v1 manifest')
    defaults=data.get('defaults',{})
    allowed={'name','scene','frames','width','height','debug_mode','ssao','ssr','threshold','launcher_args','simulation_time','time_mode','freeze_time','criteria'}
    if set(defaults)-{'frames','width','height','threshold','no_gui_overlay','freeze_time','simulation_time','time_mode'}:
        raise ValueError('unknown v1 default')
    cases, rows=[],[]
    for item in data['scenes']:
        if set(item)-allowed: raise ValueError('unknown v1 field')
        pick=lambda key,default:item.get(key,defaults.get(key,default))
        name=item['name']; args=list(item.get('launcher_args',[]))
        if item.get('debug_mode'): args+=['--debug-mode',str(item['debug_mode'])]
        for flag in ('ssao','ssr'):
            if item.get(flag): args+=['--'+flag]
        target=pick('simulation_time',0)
        mode=pick('time_mode','seek' if pick('freeze_time',True) or 'simulation_time' in item else 'wall_clock').replace('-','_')
        ref=Path(root)/f'tests/reference/{name}.png'
        cases.append({'manifest_version':2,'id':name,'scene':item['scene'],'launcher_args':args,
            'render':{'width':pick('width',1280),'height':pick('height',720),'frames':pick('frames',60),'no_gui_overlay':pick('no_gui_overlay',True),'png':'rgba8'},
            'time':{'mode':mode,'simulation_time':target,'fixed_dt':0.016666667,'max_substeps':4,'max_seek_steps':4096},
            'camera':{'source':'scene','override':None},
            'reference':dict(path=f'tests/reference/{name}.png',**{key:None for key in REFERENCE_FIELDS}),
            'compare':{'algorithm':'rgba8_normalized','exact_hash':False,'allow_degraded_compare':False,'ssim_min':pick('threshold',.99),'mae_max':.01,'diff_percent_max':.01},
            'criteria':item.get('criteria',['Review the scene output against its intended visual criteria.']),
            'benchmark':None,'variants':None})
        rows.append({'id':name,'status':'migrated','reference_status':'reference-pending',
            'existing_png_sha256':hashlib.sha256(ref.read_bytes()).hexdigest() if ref.is_file() else None,
            'provenance_reason':'historical adapter/driver/source metadata unavailable; no promotion',
            'ssim_min':pick('threshold',.99),'threshold_source':'explicit' if 'threshold' in item else 'defaults' if 'threshold' in defaults else 'current-default-.99'})
    return cases,rows

def migrate(source, output, log, root):
    source,output,log=map(Path,(source,output,log)); root=Path(root)
    original=source.read_bytes()
    backup=source.with_suffix('.v1.bak')
    temp=output.with_name(output.name+'.pending')
    log.parent.mkdir(parents=True,exist_ok=True)
    if backup.exists() or temp.exists(): raise ValueError('migration backup or pending output already exists')
    try:
        cases,rows=convert(json.loads(original),root)
        temp.write_text(json.dumps(cases,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
        materialize(temp,root)
        log.write_text(''.join(json.dumps(row,ensure_ascii=False)+'\n' for row in rows),encoding='utf-8')
        # All validation and durable evidence precede the replacement.
        os.replace(source,backup)
        try: os.replace(temp,source)
        except Exception:
            os.replace(backup,source)
            raise
        return len(cases)
    except Exception as exc:
        log.write_text(json.dumps({'status':'failed','error':str(exc),'input_sha256':hashlib.sha256(original).hexdigest()})+'\n',encoding='utf-8')
        raise

def main():
    p=argparse.ArgumentParser();p.add_argument('--input',required=True);p.add_argument('--output',required=True);p.add_argument('--migration-log',required=True);p.add_argument('--project-root',default=str(Path(__file__).resolve().parents[1]))
    a=p.parse_args()
    print('migrated_cases='+str(migrate(a.input,a.output,a.migration_log,a.project_root)))

if __name__=='__main__':
    try:main()
    except (ValueError,OSError) as exc:print(str(exc),file=sys.stderr);sys.exit(1)
