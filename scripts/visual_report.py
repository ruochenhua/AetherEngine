"""HTML and machine status share the same explicit, fail-closed evidence."""
import html
import json
from pathlib import Path

def write_report(directory,record):
    directory=Path(directory)
    (directory/'aggregate.json').write_text(json.dumps(record,indent=2,ensure_ascii=False),encoding='utf-8')
    esc=lambda value:html.escape(str(value))
    images=''.join(f'<figure><a href="{name}"><img src="{name}" alt="{name}"></a><figcaption>{name}</figcaption></figure>' for name in ['reference.png','output.png','diff.png'] if (directory/name).is_file())
    links=' · '.join(f'<a href="{name}">{name}</a>' for name in ['stdout','stderr','launcher.log','graph.txt','render-graph.json','physics-runtime.json','metrics.json','aggregate.json','process-tree.json','migration.jsonl','device.json'] if (directory/name).is_file())
    text='<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>'+esc(record['id'])+'</title><style>body{font:15px/1.6 system-ui;color:#172033;background:#f5f7fa;margin:0}main{max-width:1160px;margin:auto;padding:28px}.images{display:flex;gap:12px}figure{flex:1;margin:0}img{width:100%;image-rendering:pixelated}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:white;padding:16px;border:1px solid #d8dee9}a{color:#245dbe}table{width:100%;border-collapse:collapse}td,th{text-align:left;padding:10px;border-bottom:1px solid #d8dee9}@media(max-width:700px){.images{display:block}}</style><main><h1>'+esc(record['id'])+'</h1><table><tr><th>状态</th><td>'+esc(record['status'])+'</td></tr><tr><th>证据类型</th><td>'+esc(record.get('evidence_kind','release-launcher'))+'</td></tr></table><p>'+links+'</p><div class="images">'+images+'</div><h2>机器结果与人工条件</h2><pre>'+esc(json.dumps(record,indent=2,ensure_ascii=False))+'</pre></main></html>'
    (directory/'report.html').write_text(text,encoding='utf-8')
