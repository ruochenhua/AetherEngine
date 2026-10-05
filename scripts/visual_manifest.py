"""Invoke the production Rust schema before constructing any process argv."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
from functools import lru_cache

ROOT = Path(__file__).resolve().parents[1]

@lru_cache(maxsize=1)
def validator():
    if os.environ.get('AETHER_VISUAL_VALIDATOR'):
        return Path(os.environ['AETHER_VISUAL_VALIDATOR']).resolve()
    target = json.loads(subprocess.check_output(['cargo','metadata','--no-deps','--format-version','1'], cwd=ROOT))['target_directory']
    binary = Path(target)/'debug/examples'/('visual_case_manifest.exe' if os.name=='nt' else 'visual_case_manifest')
    # Build each invocation: Cargo's freshness check avoids using a stale schema.
    subprocess.run(['cargo','build','--locked','-p','aether-engine','--example','visual_case_manifest'],cwd=ROOT,check=True)
    return binary

def materialize(path, root=ROOT, lexical=False):
    argv=[str(validator()),str(Path(path).resolve()),str(Path(root).resolve())]
    if lexical:argv.append('--lexical')
    result = subprocess.run(argv,capture_output=True,text=True,encoding='utf-8',timeout=120)
    if result.returncode:
        raise ValueError(result.stderr.strip())
    return json.loads(result.stdout)

def validate_case(case,root=ROOT,lexical=False):
    clean={k:v for k,v in case.items() if k!='expected_result'}
    with tempfile.NamedTemporaryFile(mode='w',suffix='.json',delete=False,encoding='utf-8') as f:
        json.dump([clean],f);temporary=Path(f.name)
    try:materialize(temporary,root,lexical)
    finally:temporary.unlink()

def execution_cases(path, output_dir, root=ROOT):
    source=json.loads(Path(path).read_text(encoding='utf-8'))
    if not isinstance(source,list) or any(not isinstance(case,dict) or case.get('manifest_version')!=2 for case in source):
        raise ValueError('VisualCase v2 required; migrate the v1 matrix before running')
    if any(not isinstance(case.get('launcher_args'),list) or not all(isinstance(arg,str) for arg in case['launcher_args']) for case in source):
        raise ValueError('launcher_args must be an array of strings')
    if len({case.get('id') for case in source})!=len(source):raise ValueError('case IDs must be unique')
    result=[]
    for c in materialize(path, root):
        if c['camera']['override'] is not None:
            raise ValueError('UnsupportedCameraOverride: no silent scene-camera substitution')
        render, clock = c['render'], c['time']
        output=str(Path(output_dir).resolve()/c['id']/'output.png')
        owned={'--scene','--screenshot','--exit-after-frames','--width','--height','--time-mode','--simulation-time','--freeze-time','--fixed-dt','--max-substeps','--max-seek-steps'}
        if any(arg.split('=',1)[0] in owned for arg in c['launcher_args']):
            raise ValueError('launcher_args duplicates a schema-owned argument')
        args=c['launcher_args']+['--screenshot',output,'--exit-after-frames',str(render['frames']),
            '--width',str(render['width']),'--height',str(render['height']),
            '--time-mode',clock['mode'].replace('_','-'),'--simulation-time',str(clock['simulation_time']),
            '--fixed-dt',str(clock['fixed_dt']),'--max-substeps',str(clock['max_substeps']),
            '--max-seek-steps',str(clock['max_seek_steps'])]
        result.append({'id':c['id'],'scene':c['scene'],'launcher_args':args,'output':output,'visual_case':c})
    return result
