"""Capture candidate provenance, then explicitly promote in a separate run."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

from run_visual_regression import ROOT,launcher_binary,run
from visual_compare import rgba
from visual_report import write_report
from visual_manifest import validate_case

def promote(directory,root,case_id,reason):
    if not reason or not reason.strip():raise ValueError('promotion requires a review reason')
    directory=Path(directory);root=Path(root)
    candidate=json.loads((directory/'candidate.json').read_text(encoding='utf-8'))
    if candidate['id']!=case_id or candidate['status']!='reference-pending':raise ValueError('invalid candidate')
    rec=candidate['launcher']
    if rec['diagnostic'] is not None or rec['exit_code']!=0 or not rec['group_empty']:raise ValueError('candidate runtime failed')
    output=directory/'output.png';reference=candidate['candidate_reference']
    if hashlib.sha256(output.read_bytes()).hexdigest()!=reference['sha256']:raise ValueError('candidate image changed')
    validate_case(candidate['case'],root)
    validate_case(dict(candidate['case'],reference=reference),root,lexical=True)
    pixels=rgba(output)
    if list(pixels.shape[:2])!=[reference['height'],reference['width']]:raise ValueError('candidate dimensions changed')
    target=(root/reference['path']).resolve()
    ref_root=(root/'tests/reference').resolve()
    try:ref_root.relative_to(root.resolve())
    except ValueError:raise ValueError('reference directory escapes project root') from None
    try:target.relative_to(ref_root)
    except ValueError:raise ValueError('reference path escape') from None
    index_path=root/'tests/reference-manifest.json'
    index=json.loads(index_path.read_text(encoding='utf-8')) if index_path.exists() else {}
    index[case_id]=reference
    # Roll back PNG and index if the second replace fails.
    old_image=target.read_bytes() if target.exists() else None
    old_index=index_path.read_bytes() if index_path.exists() else None
    staged=[]
    try:
        for path,content in [(target,output.read_bytes()),(index_path,(json.dumps(index,indent=2)+'\n').encode())]:
            with tempfile.NamedTemporaryFile(dir=path.parent,delete=False) as f:f.write(content);f.flush();os.fsync(f.fileno());staged.append(Path(f.name))
        os.replace(staged[0],target);os.replace(staged[1],index_path)
    except Exception:
        if old_image is None:target.unlink(missing_ok=True)
        else:target.write_bytes(old_image)
        if old_index is None:index_path.unlink(missing_ok=True)
        else:index_path.write_bytes(old_index)
        raise
    finally:
        for p in staged:p.unlink(missing_ok=True)
    candidate.update(status='accepted-reference',criteria_status='reviewed',review_reason=reason)
    (directory/'promotion.json').write_text(json.dumps(candidate,indent=2),encoding='utf-8')
    write_report(directory,candidate)

def main():
    p=argparse.ArgumentParser();p.add_argument('--case',required=True);p.add_argument('--run-id',required=True);p.add_argument('--reason')
    action=p.add_mutually_exclusive_group();action.add_argument('--capture-reference',action='store_true');action.add_argument('--promote',action='store_true');a=p.parse_args()
    if Path(a.run_id).name!=a.run_id or Path(a.case).name!=a.case or a.run_id in ('.','..') or a.case in ('.','..'):raise ValueError('invalid run/case id')
    directory=ROOT/'tests/reports'/a.run_id/a.case
    if a.promote:promote(directory,ROOT,a.case,a.reason);return 0
    if a.reason:raise ValueError('reason is valid only for promotion')
    records,cancelled=run(ROOT/'tests/visual-matrix.json',[a.case],directory.parent,launcher_binary(ROOT),capture_reference=a.capture_reference)
    print('report='+str(directory/'report.html'))
    return 128+cancelled if cancelled else int(not a.capture_reference and records[0]['status']!='pass' or records[0]['status']=='error')

if __name__=='__main__':
    try:sys.exit(main())
    except (ValueError,OSError,RuntimeError) as exc:print(str(exc),file=sys.stderr);sys.exit(1)
