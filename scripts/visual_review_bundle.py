"""Collect existing visual candidates into a reviewable, hash-bound bundle."""
import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import re
import shutil
import sys

from PIL import Image, ImageDraw

from visual_manifest import ROOT

STATUSES = {'pass', 'fail', 'insufficient'}
ID_PATTERN = re.compile(r'^[A-Za-z0-9_-]+$')


def group_for(case_id):
    match = re.match(r'^t(\d+)(?:_|$)', case_id, re.IGNORECASE)
    return f'T{match.group(1)}' if match else 'T0'


def _load_candidates(matrix_path, reports_dir):
    cases = json.loads(Path(matrix_path).read_text(encoding='utf-8'))
    if not isinstance(cases, list) or len({c.get('id') for c in cases if isinstance(c, dict)}) != len(cases):
        raise ValueError('matrix must contain unique case IDs')
    known = {case['id']: case for case in cases}
    latest = {}
    report_root = Path(reports_dir)
    paths = list(report_root.glob('**/candidate.json')) + list(report_root.glob('**/aggregate.json'))
    for candidate_path in paths:
        if candidate_path.name == 'candidate.json' and (candidate_path.parent / 'promotion.json').is_file():
            continue
        candidate = json.loads(candidate_path.read_text(encoding='utf-8'))
        case_id = candidate.get('id')
        if case_id not in known or not ID_PATTERN.fullmatch(case_id):
            continue
        image = candidate_path.parent / 'output.png'
        if not image.is_file() or candidate.get('status') not in {'reference-pending', 'needs-review'}:
            continue
        launcher = candidate.get('launcher', {})
        if launcher.get('diagnostic') is not None or launcher.get('exit_code') != 0 or launcher.get('group_empty') is not True:
            continue
        sha = hashlib.sha256(image.read_bytes()).hexdigest()
        candidate_ref = candidate.get('candidate_reference')
        ref = candidate_ref or candidate.get('case', {}).get('reference', {})
        if ref.get('sha256') not in (None, sha) or candidate.get('case', {}).get('id') != case_id:
            continue
        item = {'id': case_id, 'group': group_for(case_id), 'sha256': sha,
                'source_commit': ref.get('source_commit'), 'candidate_dir': str(candidate_path.parent.resolve()),
                'image_path': str(image.resolve()), 'report_path': str((candidate_path.parent / 'report.html').resolve()),
                'criteria': known[case_id].get('criteria', []), 'scene': known[case_id].get('scene'),
                'runtime_status': candidate.get('status'), 'exit_code': launcher.get('exit_code'),
                'group_empty': launcher.get('group_empty'),
                'promotable': candidate_path.name == 'candidate.json' and bool(candidate_ref)}
        previous = latest.get(case_id)
        same_run = previous is not None and item['candidate_dir'] == previous['candidate_dir']
        candidate_wins_same_run = same_run and candidate_path.name == 'candidate.json' and not previous['promotable']
        newer_run = previous is not None and not same_run and candidate_path.stat().st_mtime_ns > previous['_mtime']
        if previous is None or candidate_wins_same_run or newer_run:
            item['_mtime'] = candidate_path.stat().st_mtime_ns
            latest[case_id] = item
    result = sorted(latest.values(), key=lambda item: (item['group'], item['id']))
    for item in result:
        item.pop('_mtime')
    return result


def _contact_sheet(items, target):
    thumb_w, thumb_h, label_h = 320, 180, 34
    columns = min(3, max(1, len(items)))
    rows = (len(items) + columns - 1) // columns
    sheet = Image.new('RGB', (columns * thumb_w, rows * (thumb_h + label_h)), '#e8edf3')
    draw = ImageDraw.Draw(sheet)
    for index, item in enumerate(items):
        with Image.open(item['image_path']) as source:
            source = source.convert('RGB')
            source.thumbnail((thumb_w, thumb_h))
            x = (index % columns) * thumb_w
            y = (index // columns) * (thumb_h + label_h)
            sheet.paste(source, (x + (thumb_w - source.width) // 2, y + (thumb_h - source.height) // 2))
            draw.text((x + 8, y + thumb_h + 8), item['id'][:44], fill='#172033')
    target.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(target, format='PNG')


def build(matrix_path, reports_dir, output_dir):
    output_dir = Path(output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    assets = output_dir / 'images'
    assets.mkdir(exist_ok=True)
    entries = _load_candidates(matrix_path, reports_dir)
    ledger_path = output_dir / 'review-ledger.json'
    ledger = json.loads(ledger_path.read_text(encoding='utf-8')) if ledger_path.exists() else {}
    groups = {}
    cards = []
    esc = html.escape
    for item in entries:
        group = item['group']
        groups.setdefault(group, []).append(item)
        image_name = f"{item['id']}--{item['sha256'][:12]}.png"
        shutil.copyfile(item['image_path'], assets / image_name)
        item['bundle_image'] = f'images/{image_name}'
        review = ledger.get(item['id'], {})
        current_review = review if review.get('sha256') == item['sha256'] else {}
        report = Path(item['report_path'])
        try:
            report_link = os.path.relpath(report, output_dir).replace(os.sep, '/')
            report_anchor = f'<a href="{esc(report_link)}">原始报告</a>' if report.is_file() else '<span>原始报告缺失</span>'
        except ValueError:
            report_anchor = '<span>原始报告路径不可访问</span>'
        criteria = ''.join(f'<li>{esc(str(value))}</li>' for value in item['criteria'])
        status = esc(current_review.get('status', '未审核'))
        note = esc(current_review.get('note', ''))
        promotion = '可进入人工批准晋升流程' if item['promotable'] else '仅供审核（缺少 promote 来源记录）'
        cards.append(f'<article class="card" data-group="{esc(group)}" data-status="{status}"><h3>{esc(item["id"])} <small>{status}</small></h3><a href="{esc(item["bundle_image"])}"><img src="{esc(item["bundle_image"])}" alt="{esc(item["id"])}"></a><p>{esc(str(item["scene"]))} · commit {esc(str(item["source_commit"]))}</p><ul>{criteria}</ul><p>sha256 <code>{esc(item["sha256"])}</code></p><p>{report_anchor} · launcher exit {esc(str(item["exit_code"]))} · process group empty {esc(str(item["group_empty"]))}</p><p>{promotion}</p><p>审核记录：{status} {note}</p></article>')
    for group, members in groups.items():
        _contact_sheet(members, output_dir / f'contact-{group}.png')
    previous = json.loads((output_dir / 'bundle.json').read_text(encoding='utf-8')) if (output_dir / 'bundle.json').exists() else {}
    if previous.get('candidates'):
        old_by_id = {item['id']: item for item in previous['candidates']}
        for item in entries:
            old = old_by_id.get(item['id'])
            if old and old['sha256'] == item['sha256']:
                item.update({k: v for k, v in old.items() if k == 'bundle_image'})
    payload = {'schema_version': 1, 'candidate_count': len(entries), 'groups': sorted(groups), 'candidates': entries}
    (output_dir / 'bundle.json').write_text(json.dumps(payload, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    group_options = ''.join(f'<option value="{esc(group)}">{esc(group)}</option>' for group in sorted(groups))
    contact_sheets = ''.join(f'<section><h2>{esc(group)}</h2><img class="sheet" src="contact-{esc(group)}.png" alt="{esc(group)} contact sheet"></section>' for group in sorted(groups))
    page = '<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>视觉候选审核包</title><style>body{font:15px/1.5 system-ui;color:#172033;background:#f4f6f9;margin:0}main{max-width:1300px;margin:auto;padding:24px}header{position:sticky;top:0;background:#f4f6f9ee;padding:12px;border-bottom:1px solid #ccd3df;z-index:1}.sheets{display:flex;gap:20px;overflow:auto}.sheet{max-height:230px}.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(360px,1fr));gap:16px}.card{background:white;padding:16px;border:1px solid #d7dde6;border-radius:10px;overflow-wrap:anywhere}.card img{width:100%;image-rendering:auto}small{font-weight:normal;color:#526174}code{font-size:12px;word-break:break-all}</style><main><header><h1>视觉候选审核包</h1><p>候选图由机器采集；通过状态必须由人工写入 review-ledger.json。此页面不会自动批准图像。</p><label>分组 <select id="group"><option value="">全部</option>'+group_options+'</select></label> <label>审核状态 <select id="status"><option value="">全部</option><option>未审核</option><option>pass</option><option>fail</option><option>insufficient</option></select></label><p>'+str(len(entries))+' 个最新有效候选 · bundle.json 与 review-ledger.json 为机器/审核记录</p></header><div class="sheets">'+contact_sheets+'</div><section class="cards">'+''.join(cards)+'</section></main><script>function filter(){for(const c of document.querySelectorAll(".card")){c.hidden=(document.querySelector("#group").value&&c.dataset.group!==document.querySelector("#group").value)||(document.querySelector("#status").value&&c.dataset.status!==document.querySelector("#status").value)}}document.querySelector("#group").onchange=filter;document.querySelector("#status").onchange=filter;</script></html>'
    (output_dir / 'index.html').write_text(page, encoding='utf-8')
    return payload


def _bundle_candidates(output_dir):
    bundle = json.loads((Path(output_dir) / 'bundle.json').read_text(encoding='utf-8'))
    return {item['id']: item for item in bundle['candidates']}


def record_review(output_dir, case_id, sha256, status, note):
    if status not in STATUSES:
        raise ValueError('review status must be pass, fail, or insufficient')
    candidates = _bundle_candidates(output_dir)
    candidate = candidates.get(case_id)
    if candidate is None:
        raise ValueError('case is not present in the current review bundle')
    if candidate['sha256'] != sha256:
        raise ValueError('candidate hash mismatch; rebuild the bundle and review the current image')
    if not note.strip():
        raise ValueError('review note is required')
    path = Path(output_dir) / 'review-ledger.json'
    ledger = json.loads(path.read_text(encoding='utf-8')) if path.exists() else {}
    ledger[case_id] = {'sha256': sha256, 'source_commit': candidate.get('source_commit'), 'status': status,
                       'note': note.strip(), 'reviewed_at_utc': __import__('datetime').datetime.now(__import__('datetime').timezone.utc).isoformat()}
    path.write_text(json.dumps(ledger, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    return ledger[case_id]


def approved_candidates(output_dir):
    candidates = _bundle_candidates(output_dir)
    ledger_path = Path(output_dir) / 'review-ledger.json'
    ledger = json.loads(ledger_path.read_text(encoding='utf-8')) if ledger_path.exists() else {}
    approved = [case_id for case_id, item in candidates.items()
                if ledger.get(case_id, {}).get('status') == 'pass' and ledger[case_id].get('sha256') == item['sha256']
                and ledger[case_id].get('note', '').strip() and item.get('promotable')]
    if not approved:
        raise ValueError('no current candidate has an explicit human pass')
    return sorted(approved)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--matrix', type=Path, default=ROOT / 'tests/visual-matrix.json')
    parser.add_argument('--reports', type=Path, default=ROOT / 'tests/reports')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--review-case')
    parser.add_argument('--sha256')
    parser.add_argument('--status', choices=sorted(STATUSES))
    parser.add_argument('--note', default='')
    parser.add_argument('--promote-approved', action='store_true')
    parser.add_argument('--reason')
    args = parser.parse_args()
    if args.review_case:
        print(json.dumps(record_review(args.out, args.review_case, args.sha256 or '', args.status or '', args.note), ensure_ascii=False))
        result = build(args.matrix, args.reports, args.out)
        print(f"bundle={args.out.resolve() / 'index.html'} candidates={result['candidate_count']}")
    else:
        result = build(args.matrix, args.reports, args.out)
        print(f"bundle={args.out.resolve() / 'index.html'} candidates={result['candidate_count']}")
    if args.promote_approved:
        if not args.reason or not args.reason.strip():
            raise ValueError('--promote-approved requires --reason')
        from run_primary_case import promote
        for case_id in approved_candidates(args.out):
            candidate = _bundle_candidates(args.out)[case_id]
            promote(candidate['candidate_dir'], ROOT, case_id, args.reason)
            print(f'promoted={case_id}')
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
