#!/usr/bin/env python3
"""Read-only comparison of plan source anchors to a local Git checkout (Python 3.11+)."""
from __future__ import annotations
import argparse, hashlib, json, re, subprocess, sys
from pathlib import Path

def git(repo: Path, *args: str) -> bytes:
    p = subprocess.run(['git', '-C', str(repo), *args], capture_output=True, check=False)
    if p.returncode:
        raise RuntimeError(p.stderr.decode('utf-8', errors='replace').strip())
    return p.stdout

def inspect_anchor(raw: bytes, anchor: dict) -> dict:
    oid = hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
    lines = raw.decode('utf-8').splitlines()
    start, end = anchor['start_line'], min(anchor['end_line'], len(lines))
    needle = anchor['needle']
    hits = [i+1 for i, line in enumerate(lines) if needle in line]
    in_window = any(start <= i <= end for i in hits)
    expected = anchor.get('git_blob_sha')
    status = 'PASS'
    if expected and oid != expected:
        status = 'BLOB_MISMATCH'
    elif not hits:
        status = 'ANCHOR_NOT_FOUND'
    elif not in_window:
        status = 'RANGE_REBASE_REQUIRED'
    return {'id':anchor['id'], 'path':anchor['path'], 'status':status,
            'actual_blob_sha':oid, 'actual_lines':len(lines), 'anchor_lines':hits,
            'reviewed_window':[anchor['start_line'], anchor['end_line']]}

def main() -> int:
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('repo',type=Path)
    ap.add_argument('--manifest',type=Path,default=Path(__file__).resolve().parents[1]/'data/source-anchors.json')
    ap.add_argument('--anchor',action='append',default=[],help='Restrict to Sxx; may be repeated.')
    ap.add_argument('--check-worktree',action='store_true',help='Also flag changed/missing current files. Useful before the first edit, not after cutover.')
    ap.add_argument('--json-out',type=Path)
    args=ap.parse_args()
    try:
        m=json.loads(args.manifest.read_text(encoding='utf-8'))
        base=m['baseline_commit']
        if not re.fullmatch(r'[0-9a-f]{40}',base):raise ValueError('invalid baseline SHA')
        git(args.repo,'cat-file','-e',base+'^{commit}')
        anchors=[a for a in m['anchors'] if not args.anchor or a['id'] in args.anchor]
        missing=set(args.anchor)-{a['id'] for a in anchors}
        if missing:raise ValueError(f'unknown anchor IDs: {sorted(missing)}')
        cache={}; results=[]
        for a in anchors:
            path=a['path']
            try:
                if path not in cache:cache[path]=git(args.repo,'show',base+':'+path)
                item=inspect_anchor(cache[path],a)
                if args.check_worktree:
                    current=args.repo/path
                    item['worktree']='UNCHANGED' if current.is_file() and current.read_bytes()==cache[path] else 'CHANGED_OR_MISSING'
                results.append(item)
            except (RuntimeError,UnicodeDecodeError,OSError) as exc:
                results.append({'id':a['id'],'path':path,'status':'READ_ERROR','error':str(exc)})
        failed=[r for r in results if r['status']!='PASS' or r.get('worktree')=='CHANGED_OR_MISSING']
        report={'baseline_commit':base,'checked':len(results),'failed':len(failed),'results':results}
        if args.json_out:
            args.json_out.parent.mkdir(parents=True,exist_ok=True)
            args.json_out.write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
        for r in failed:print(json.dumps(r,ensure_ascii=False),file=sys.stderr)
        print(f"Baseline anchors: {len(results)-len(failed)}/{len(results)} passed. No source files changed.")
        return 1 if failed else 0
    except (OSError,ValueError,KeyError,RuntimeError,json.JSONDecodeError) as exc:
        print('Cannot verify baseline: '+str(exc),file=sys.stderr)
        return 2
if __name__=='__main__':sys.exit(main())
