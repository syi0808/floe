#!/usr/bin/env python3
"""Self-tests of the plan's READ-ONLY helper logic, not Floe application tests."""
import hashlib,unittest,json,subprocess,sys,tempfile
from pathlib import Path
from verify_baseline import inspect_anchor
from check_architecture import graph_errors, production_deps
class AnchorTests(unittest.TestCase):
    def make(self):
        raw=b'header\nfn sample() {}\n'
        return raw,dict(id='S00',path='sample.rs',start_line=2,end_line=2,needle='fn sample',git_blob_sha=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest())
    def test_match(self):
        b,a=self.make();self.assertEqual(inspect_anchor(b,a)['status'],'PASS')
    def test_blob_drift(self):
        b,a=self.make();self.assertEqual(inspect_anchor(b+b'\n',a)['status'],'BLOB_MISMATCH')
    def test_range_drift(self):
        b,a=self.make();a['end_line']=1;a['start_line']=1;self.assertEqual(inspect_anchor(b,a)['status'],'RANGE_REBASE_REQUIRED')
    def test_missing_needle(self):
        b,a=self.make();a['needle']='absent';self.assertEqual(inspect_anchor(b,a)['status'],'ANCHOR_NOT_FOUND')
    def test_eof_window(self):
        b,a=self.make();a['end_line']=90;self.assertEqual(inspect_anchor(b,a)['status'],'PASS')
class GraphTests(unittest.TestCase):
    def test_dag(self):self.assertEqual(graph_errors({'a':{'b'},'b':set()},[]),[])
    def test_cycle(self):self.assertTrue(graph_errors({'a':{'b'},'b':{'a'}},[]))
    def test_transitive(self):self.assertTrue(graph_errors({'a':{'b'},'b':{'c'},'c':set()},[('a','c')]))
    def test_workspace_alias(self):self.assertEqual(production_deps({'dependencies':{'x':{'workspace':True}}},{'x':{'package':'floe-test'}}),['floe-test'])
    def test_target_build(self):self.assertEqual(production_deps({'target':{'cfg(foo)':{'build-dependencies':{'x':{'package':'floe-x'}}}}},{}),['floe-x'])
    def test_dev_excluded(self):self.assertEqual(production_deps({'dev-dependencies':{'x':'1'}},{}),[])
class MigrationCliTests(unittest.TestCase):
    def run_check(self, packages, mode):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'Cargo.toml').write_text('[workspace]\nmembers = []\n')
            for name,path,deps in packages:
                dest=root/path;dest.mkdir(parents=True)
                text=f'[package]\nname = "{name}"\nversion = "0.1.0"\n[dependencies]\n'
                text+=''.join(f'{dep} = "0.1"\n' for dep in deps)
                (dest/'Cargo.toml').write_text(text)
            result=subprocess.run([sys.executable,str(Path(__file__).with_name('check_architecture.py')),str(root),'--mode',mode],capture_output=True,text=True)
            return result,json.loads(result.stdout)
    def test_migration_allows_pinned_legacy_name_paths(self):
        result,report=self.run_check([('floe-ffi','crates/floe-ffi',['floe-core']),('floe-core','crates/floe-core',[])],'migration')
        self.assertEqual(result.returncode,0,report)
        self.assertTrue(report['warnings'])
    def test_final_rejects_legacy_name_paths(self):
        result,report=self.run_check([('floe-ffi','crates/floe-ffi',[])],'final')
        self.assertNotEqual(result.returncode,0)
        self.assertTrue(any('wrong directory' in e for e in report['errors']))
    def test_target_must_not_depend_on_unmigrated_same_name(self):
        result,report=self.run_check([('floe-conversation','crates/modules/conversation',['floe-agent-contract']),('floe-agent-contract','crates/floe-agent-contract',[])],'migration')
        self.assertNotEqual(result.returncode,0)
        self.assertTrue(any('unmigrated/legacy' in e for e in report['errors']))
if __name__=='__main__':unittest.main()
