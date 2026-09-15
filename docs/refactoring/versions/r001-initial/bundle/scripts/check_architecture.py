#!/usr/bin/env python3
"""Check the approved Rust internal production dependency DAG; read-only, Python 3.11+.
This is not a Dart/Go import analyzer and does not prove semantic isolation.
"""
from __future__ import annotations
import argparse,json,sys,tomllib
from pathlib import Path

def graph_errors(graph:dict[str,set[str]], forbidden:list[tuple[str,str]]) -> list[str]:
    errors=[]; marks={}; stack=[]
    def visit(n):
        if marks.get(n)==1:
            errors.append('cycle: '+' -> '.join(stack[stack.index(n):]+[n]));return
        if marks.get(n)==2:return
        marks[n]=1;stack.append(n)
        for nxt in graph.get(n,()):visit(nxt)
        stack.pop();marks[n]=2
    for n in graph:visit(n)
    for src,dst in forbidden:
        pending=[(src,[src])];seen=set()
        while pending:
            n,path=pending.pop()
            if n in seen:continue
            seen.add(n)
            if n==dst and n!=src:
                errors.append('forbidden path: '+' -> '.join(path));break
            for nxt in graph.get(n,()):pending.append((nxt,path+[nxt]))
    return errors

def production_deps(doc:dict, workspace:dict) -> list[str]:
    tables=[doc.get('dependencies',{}),doc.get('build-dependencies',{})]
    for target in doc.get('target',{}).values():
        tables.extend([target.get('dependencies',{}),target.get('build-dependencies',{})])
    names=[]
    for table in tables:
        for alias, spec in table.items():
            if isinstance(spec,dict) and spec.get('workspace'):
                base=workspace.get(alias,{})
                spec={**(base if isinstance(base,dict) else {}),**spec}
            names.append(spec.get('package',alias) if isinstance(spec,dict) else alias)
    return names

def main()->int:
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('repo',type=Path,nargs='?')
    ap.add_argument('--policy',type=Path,default=Path(__file__).resolve().parents[1]/'data/module-dependencies.json')
    ap.add_argument('--policy-only',action='store_true')
    ap.add_argument('--mode',choices=['final','migration'],default='final')
    ap.add_argument('--json-out',type=Path)
    args=ap.parse_args()
    try:
        policy=json.loads(args.policy.read_text(encoding='utf-8'))
        entries=policy['target']; byid={e['id']:e for e in entries}
        expected={e['package']:e for e in entries}
        allowed={e['package']:{byid[d]['package'] for d in e['allowed_internal_dependencies']} for e in entries}
        generic=[e['package'] for e in entries if e['group'] in ('runtime','modules')]
        forbidden=[(p,'floe-experts-builtin') for p in generic]
        forbidden += [('floe-inference','floe-connections'),('floe-connections','floe-inference')]
        forbidden += [(e['package'],d) for e in entries if e['group']=='modules' for d in ['floe-vault','floe-provider-adapters','floe-ffi','floe-app']]
        errors=graph_errors(allowed,forbidden); warnings=[]; graph=allowed
        if not args.policy_only:
            if args.repo is None:raise ValueError('repo is required without --policy-only')
            root=tomllib.loads((args.repo/'Cargo.toml').read_text())
            ws=root.get('workspace',{}).get('dependencies',{})
            docs={}; paths={}
            for path in (args.repo/'crates').rglob('Cargo.toml'):
                doc=tomllib.loads(path.read_text())
                if 'package' not in doc:continue
                name=doc['package']['name']
                if name in docs:errors.append('duplicate package: '+name)
                docs[name]=doc;paths[name]=str(path.parent.relative_to(args.repo))
            if args.mode=='final':
                for name in set(expected)-set(docs):errors.append('missing target crate: '+name)
                for name in set(docs)-set(expected):errors.append('unexpected/legacy crate: '+name)
            else:
                warnings.append('migration mode is NOT the final architecture gate; old roots may remain temporarily')
            graph={}
            baseline_paths={name:'crates/'+name for name in policy.get('current',{}).get('dependencies',{})}
            legacy=set()
            for name in docs:
                if name not in expected or paths[name]!=expected[name]['path']:
                    legacy.add(name)
            for name,doc in docs.items():
                deps={d for d in production_deps(doc,ws) if d in docs or d.startswith('floe-')}
                graph[name]=deps
                if name not in expected:continue
                relocated=paths[name]==expected[name]['path']
                if not relocated:
                    if args.mode=='migration' and paths[name]==baseline_paths.get(name):
                        warnings.append(f'{name}: approved baseline path retained temporarily ({paths[name]})')
                        continue
                    errors.append(f'{name}: wrong directory {paths[name]}')
                for dep in deps-allowed[name]:errors.append(f'{name}: disallowed direct dependency {dep}')
                if relocated:
                    for dep in deps & legacy:
                        errors.append(f'{name}: target crate depends on unmigrated/legacy crate {dep}')
            errors+=graph_errors(graph,forbidden)
        report={'mode':'policy-only' if args.policy_only else args.mode,
                'nodes':len(graph),'edges':sum(map(len,graph.values())),
                'errors':errors,'warnings':warnings,
                'scope':'internal normal + build dependencies, including all declared target tables; excludes dev dependencies and source-level semantic checks'}
        if args.json_out:
            args.json_out.parent.mkdir(parents=True,exist_ok=True)
            args.json_out.write_text(json.dumps(report,ensure_ascii=False,indent=2))
        print(json.dumps(report,ensure_ascii=False,indent=2))
        return 1 if errors else 0
    except (OSError,KeyError,ValueError,tomllib.TOMLDecodeError) as exc:
        print('Cannot check architecture: '+str(exc),file=sys.stderr);return 2
if __name__=='__main__':sys.exit(main())
