//! End-to-end tests for the import loader: flat vs. namespaced merge, clash
//! detection, missing files, and import cycles. These exercise real filesystem
//! reads, so each test builds a throwaway directory of viafiles.

#[path = "../src/parser.rs"]
mod parser;
#[path = "../src/loader.rs"]
mod loader;
#[path = "../src/validate.rs"]
mod validate;

use loader::LoadError;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A unique temp directory that wipes itself on drop.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("via-imports-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).expect("create sandbox");
        Sandbox { dir }
    }

    /// Write `body` to `name` (relative path) inside the sandbox.
    fn write(&self, name: &str, body: &str) {
        let path = self.dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(path, body).expect("write file");
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn path(segs: &[&str]) -> Vec<String> {
    segs.iter().map(|s| s.to_string()).collect()
}

#[test]
fn flat_import_merges_tasks_keeping_names() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"lib.viafile\"\nbuild:\n    echo build\n");
    sb.write("lib.viafile", "lint:\n    echo lint\n");

    let v = loader::load(&sb.path("viafile")).expect("load ok");
    assert!(v.get(&path(&["build"])).is_some());
    assert!(v.get(&path(&["lint"])).is_some(), "imported task keeps its name");
}

#[test]
fn namespaced_import_prefixes_paths_and_internal_deps() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"docker.viafile\" as docker\n");
    sb.write(
        "docker.viafile",
        "build:\n    echo build\nrelease: build\n    echo release\n",
    );

    let v = loader::load(&sb.path("viafile")).expect("load ok");

    // Both tasks are nested under `docker`.
    assert!(v.get(&path(&["docker", "build"])).is_some());
    let release = v.get(&path(&["docker", "release"])).expect("docker::release exists");

    // The imported file's internal dep `build` was re-prefixed to `docker::build`,
    // so it still resolves within the merged model.
    assert_eq!(
        release.deps,
        vec![parser::Dep { path: path(&["docker", "build"]), args: vec![] }]
    );
    assert!(validate::validate(&v).is_ok(), "namespaced deps resolve");
}

#[test]
fn duplicate_task_across_files_is_a_clash() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"lib.viafile\"\nbuild:\n    echo local\n");
    sb.write("lib.viafile", "build:\n    echo imported\n");

    let err = loader::load(&sb.path("viafile")).expect_err("should clash");
    match err {
        LoadError::Clash { name, .. } => assert_eq!(name, "build"),
        other => panic!("expected clash, got {:?}", other),
    }
}

#[test]
fn namespacing_avoids_what_would_otherwise_clash() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"lib.viafile\" as lib\nbuild:\n    echo local\n");
    sb.write("lib.viafile", "build:\n    echo imported\n");

    let v = loader::load(&sb.path("viafile")).expect("namespacing disambiguates");
    assert!(v.get(&path(&["build"])).is_some());
    assert!(v.get(&path(&["lib", "build"])).is_some());
}

#[test]
fn missing_import_reports_the_directive_site() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"nope.viafile\"\nbuild:\n    echo build\n");

    let err = loader::load(&sb.path("viafile")).expect_err("missing file");
    match err {
        LoadError::Read { from: Some((_, line)), .. } => assert_eq!(line, 1),
        other => panic!("expected read error pointing at the import, got {:?}", other),
    }
}

#[test]
fn import_cycle_is_detected() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"a.viafile\"\n");
    sb.write("a.viafile", "import \"b.viafile\"\nag:\n    echo a\n");
    sb.write("b.viafile", "import \"a.viafile\"\nbg:\n    echo b\n");

    let err = loader::load(&sb.path("viafile")).expect_err("cycle");
    assert!(matches!(err, LoadError::Cycle { .. }), "got {:?}", err);
}

#[test]
fn parent_may_give_an_as_namespace_a_default_task() {
    let sb = Sandbox::new();
    // A bare `ci` task (path == the namespace) is allowed: it gives the
    // namespace an action without reaching inside it.
    sb.write("viafile", "import \"sub.via\" as ci\nci: ci::test, ci::build\n");
    sb.write("sub.via", "test:\n    echo t\nbuild:\n    echo b\n");

    let v = loader::load(&sb.path("viafile")).expect("default task is allowed");
    // `ci` is both a task and a namespace.
    assert!(v.get(&path(&["ci"])).is_some());
    assert!(v.get(&path(&["ci", "test"])).is_some());
}

#[test]
fn parent_may_not_extend_a_sealed_as_namespace() {
    let sb = Sandbox::new();
    sb.write(
        "viafile",
        "import \"sub.via\" as ci\nci::deploy:\n    echo deploy\n",
    );
    sb.write("sub.via", "test:\n    echo t\n");

    let err = loader::load(&sb.path("viafile")).expect_err("extend should be sealed");
    match err {
        LoadError::SealedNamespace { namespace, task, .. } => {
            assert_eq!(namespace, "ci");
            assert_eq!(task, "ci::deploy");
        }
        other => panic!("expected SealedNamespace, got {:?}", other),
    }
}

#[test]
fn parent_may_not_redefine_a_task_in_a_sealed_namespace() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"sub.via\" as ci\nci::test:\n    echo mine\n");
    sb.write("sub.via", "test:\n    echo t\n");

    // Redefining an imported task is forbidden too (caught as a seal violation).
    let err = loader::load(&sb.path("viafile")).expect_err("redefine should be forbidden");
    assert!(
        matches!(err, LoadError::SealedNamespace { .. } | LoadError::Clash { .. }),
        "got {:?}",
        err
    );
}

#[test]
fn two_imports_cannot_fill_the_same_namespace() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"a.via\" as ci\nimport \"b.via\" as ci\n");
    sb.write("a.via", "test:\n    echo a\n");
    sb.write("b.via", "build:\n    echo b\n");

    let err = loader::load(&sb.path("viafile")).expect_err("one import per as-namespace");
    assert!(matches!(err, LoadError::SealedNamespace { .. }), "got {:?}", err);
}

#[test]
fn imports_nest_transitively() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"a.viafile\" as outer\n");
    sb.write("a.viafile", "import \"b.viafile\" as inner\n");
    sb.write("b.viafile", "build:\n    echo deep\n");

    let v = loader::load(&sb.path("viafile")).expect("load ok");
    assert!(
        v.get(&path(&["outer", "inner", "build"])).is_some(),
        "transitive `as` prefixes stack: outer::inner::build"
    );
}

#[test]
fn imports_resolve_relative_to_the_importing_file() {
    let sb = Sandbox::new();
    sb.write("viafile", "import \"sub/child.viafile\"\n");
    // child.viafile imports a sibling using a path relative to sub/, not the root.
    sb.write("sub/child.viafile", "import \"helper.viafile\"\n");
    sb.write("sub/helper.viafile", "help:\n    echo help\n");

    let v = loader::load(&sb.path("viafile")).expect("relative resolution");
    assert!(v.get(&path(&["help"])).is_some());
}
