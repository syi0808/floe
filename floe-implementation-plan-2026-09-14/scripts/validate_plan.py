#!/usr/bin/env python3
"""Validate plan references, approved module DAG, and work-package ordering. Not Floe tests."""
from __future__ import annotations
import argparse,json,re,sys
from pathlib import Path
from graphlib import TopologicalSorter,CycleError

def validate(root:Path)->dict:
    errors=[]
    anchors=json.loads((root/'data/source-anchors.json').read_text())
    packages=json.loads((root/'data/work-packages.json').read_text())['packages']
    tests=json.loads((root/'data/acceptance-tests.json').read_text())['tests']
    policy=json.loads((root/'data/module-dependencies.json').read_text())
    aid={a['id'] for a in anchors['anchors']};pid={p['id'] for p in packages};tid={t['id'] for t in tests};mid={m['id'] for m in policy['target']}
    for title,ids,records in [('anchors',aid,anchors['anchors']),('packages',pid,packages),('tests',tid,tests),('modules',mid,policy['target'])]:
        if len(ids)!=len(records):errors.append('duplicate '+title+' ID')
    consumed=set();tested=set();modules=set();sha_by_path={}
    for a in anchors['anchors']:
        if not re.fullmatch(r'[a-f0-9]{40}',a['git_blob_sha']):errors.append(a['id']+' invalid blob hash')
        if a['start_line']<1 or a['end_line']<a['start_line']:errors.append(a['id']+' invalid window')
        if not a['needle'].strip():errors.append(a['id']+' missing needle')
        if anchors['baseline_commit'] not in a['url']:errors.append(a['id']+' unpinned source')
        old=sha_by_path.setdefault(a['path'],a['git_blob_sha'])
        if old!=a['git_blob_sha']:errors.append('inconsistent source SHA: '+a['path'])
    for p in packages:
        for key,allowed in [('anchors',aid),('dependencies',pid),('tests',tid),('modules',mid)]:
            for value in set(p[key])-allowed:errors.append(p['id']+' unknown '+key+': '+value)
        if p['id'] in p['dependencies']:errors.append('self dependency '+p['id'])
        for key in ['target_files','implementation_steps','preserve','deletion_gate','cutover','commands']:
            if not p[key]:errors.append(p['id']+' empty '+key)
        if not (root/'work-packages'/f"{p['id']}.md").is_file():errors.append(p['id']+' missing document')
        consumed.update(p['anchors']);tested.update(p['tests']);modules.update(p['modules'])
    errors += ['unused anchor '+x for x in sorted(aid-consumed)]
    errors += ['unassigned scenario '+x for x in sorted(tid-tested)]
    errors += ['unassigned target module '+x for x in sorted(mid-modules)]
    try:order=list(TopologicalSorter({p['id']:set(p['dependencies']) for p in packages}).static_order())
    except CycleError as exc:errors.append('work-package cycle: '+str(exc));order=[]
    try:list(TopologicalSorter({m['id']:set(m['allowed_internal_dependencies']) for m in policy['target']}).static_order())
    except CycleError as exc:errors.append('module cycle: '+str(exc))
    if any(t['status']!='not_run' for t in tests):errors.append('application scenarios must not claim execution in a planning artifact')
    return {'scope':'plan structure and references only; no repository checkout/build/host/model execution',
            'anchors':len(aid),'source_files':len(sha_by_path),'packages':len(pid),'scenarios':len(tid),'target_crates':len(mid),
            'allowed_edges':sum(len(m['allowed_internal_dependencies']) for m in policy['target']),
            'execution_order_example':order,'errors':errors}
def main()->int:
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[1]);ap.add_argument('--json-out',type=Path)
    args=ap.parse_args()
    try:report=validate(args.root)
    except (OSError,KeyError,ValueError,json.JSONDecodeError) as exc:print(str(exc),file=sys.stderr);return 2
    if args.json_out:args.json_out.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(report,ensure_ascii=False,indent=2));return int(bool(report['errors']))
if __name__=='__main__':sys.exit(main())
