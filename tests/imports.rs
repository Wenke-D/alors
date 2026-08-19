//! End-to-end tests for the import loader: flat vs. namespaced merge, clash
//! detection, missing files, and import cycles. These exercise real filesystem
//! reads, so each test builds a throwaway directory of task files.

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
        let dir = std::env::temp_dir().join(format!("alors-imports-{}-{}", std::process::id(), n));
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
    sb.write("tasks.alors", "import \"lib.alors\"\nbuild:\n    echo build\n");
    sb.write("lib.alors", "lint:\n    echo lint\n");

    let v = loader::load(&sb.path("tasks.alors")).expect("load ok");
    assert!(v.get(&path(&["build"])).is_some());
    assert!(v.get(&path(&["lint"])).is_some(), "imported task keeps its name");
}

#[test]
fn namespaced_import_prefixes_paths_and_internal_deps() {
    let sb = Sandbox::new();
    sb.write("tasks.alors", "import \"docker.alors\" as docker\n");
    sb.write(
        "docker.alors",
        "build:\n    echo build\nrelease: build\n    echo release\n",
    );

    let v = loader::load(&sb.path("tasks.alors")).expect("load ok");

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
    sb.write("tasks.alors", "import \"lib.alors\"\nbuild:\n    echo local\n");
    sb.write("lib.alors", "build:\n    echo imported\n");

    let err = loader::load(&sb.path("tasks.alors")).expect_err("should clash");
    match err {
        LoadError::Clash { name, .. } => assert_eq!(name, "build"),
        other => panic!("expected clash, got {:?}", other),
    }
}

#[test]
fn namespacing_avoids_what_would_otherwise_clash() {
    let sb = Sandbox::new();
    sb.write("tasks.alors", "import \"lib.alors\" as lib\nbuild:\n    echo local\n");
    sb.write("lib.alors", "build:\n    echo imported\n");

    let v = loader::load(&sb.path("tasks.alors")).expect("namespacing disambiguates");
    assert!(v.get(&path(&["build"])).is_some());
    assert!(v.get(&path(&["lib", "build"])).is_some());
}

#[test]
fn missing_import_reports_the_directive_site() {
    let sb = Sandbox::new();
    sb.write("tasks.alors", "import \"nope.alors\"\nbuild:\n    echo build\n");

    let err = loader::load(&sb.path("tasks.alors")).expect_err("missing file");
    match err {
        LoadError::Read { from: Some((_, line)), .. } => assert_eq!(line, 1),
        other => panic!("expected read error pointing at the import, got {:?}", other),
    }
}

#[test]
fn import_cycle_is_detected() {
    let sb = Sandbox::new();
    sb.write("tasks.alors", "import \"a.alors\"\n");
    sb.write("a.alors", "import \"b.alors\"\nag:\n    echo a\n");
    sb.write("b.alors", "import \"a.alors\"\nbg:\n    echo b\n");

    let err = loader::load(&sb.path("tasks.alors")).expect_err("cycle");
    assert!(matches!(err, LoadError::Cycle { .. }), "got {:?}", err);
}

#[test]
fn parent_may_give_an_as_namespace_a_default_task() {
    let sb = Sandbox::new();
    // A bare `ci` task (path == the namespace) is allowed: it gives the
    // namespace an action without reaching inside it.
    sb.write("tasks.alors", "import \"sub.alors\" as ci\nci: ci::test, ci::build\n");
    sb.write("sub.alors", "test:\n    echo t\nbuild:\n    echo b\n");

    let v = loader::load(&sb.path("tasks.alors")).expect("default task is allowed");
    // `ci` is both a task and a namespace.
    assert!(v.get(&path(&["ci"])).is_some());
    assert!(v.get(&path(&["ci", "test"])).is_some());
}

#[test]
fn parent_may_not_extend_a_sealed_as_namespace() {
    let sb = Sandbox::new();
    sb.write(
        "tasks.alors",
        "import \"sub.alors\" as ci\nci::deploy:\n    echo deploy\n",
    );
    sb.write("sub.alors", "test:\n    echo t\n");

    let err = loader::load(&sb.path("tasks.alors")).expect_err("extend should be sealed");
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
    sb.write("tasks.alors", "import \"sub.alors\" as ci\nci::test:\n    echo mine\n");
    sb.write("sub.alors", "test:\n    echo t\n");

    // Redefining an imported task is forbidden too (caught as a seal violation).
    let err = loader::load(&sb.path("tasks.alors")).expect_err("redefine should be forbidden");
    assert!(
        matches!(err, LoadError::SealedNamespace { .. } | LoadError::Clash { .. }),
        "got {:?}",
        err
    );
}

#[test]
fn two_imports_cannot_fill_the_same_namespace() {
    let sb = Sandbox::new();
    sb.write("tasks.alors", "import \"a.alors\" as ci\nimport \"b.alors\" as ci\n");
    sb.write("a.alors", "test:\n    echo a\n");
    sb.write("b.alors", "build:\n    echo b\n");

    let err = loader::load(&sb.path("tasks.alors")).expect_err("one import per as-namespace");
    assert!(matches!(err, LoadError::SealedNamespace { .. }), "got {:?}", err);
}

#[test]
fn imports_nest_transitively() {
    let sb = Sandbox::new();
    sb.write("tasks.alors", "import \"a.alors\" as outer\n");
    sb.write("a.alors", "import \"b.alors\" as inner\n");
    sb.write("b.alors", "build:\n    echo deep\n");

    let v = loader::load(&sb.path("tasks.alors")).expect("load ok");
    assert!(
        v.get(&path(&["outer", "inner", "build"])).is_some(),
        "transitive `as` prefixes stack: outer::inner::build"
    );
}

#[test]
fn imports_resolve_relative_to_the_importing_file() {
    let sb = Sandbox::new();
    sb.write("tasks.alors", "import \"sub/child.alors\"\n");
    // child.alors imports a sibling using a path relative to sub/, not the root.
    sb.write("sub/child.alors", "import \"helper.alors\"\n");
    sb.write("sub/helper.alors", "help:\n    echo help\n");

    let v = loader::load(&sb.path("tasks.alors")).expect("relative resolution");
    assert!(v.get(&path(&["help"])).is_some());
}

#[test]
fn constants_are_file_local_across_imports() {
    let sb = Sandbox::new();
    // Root and import each define their own `version`; each file's tasks see
    // only their own value, and an unknown name is NOT filled by the other file.
    sb.write(
        "tasks.alors",
        "import \"lib.alors\"\nversion := root-1\nshow:\n    echo {{version}} {{libonly}}\n",
    );
    sb.write(
        "lib.alors",
        "version := lib-2\nlibonly := L\nlibshow:\n    echo {{version}}\n",
    );

    let v = loader::load(&sb.path("tasks.alors")).expect("load ok");
    let show = v.get(&path(&["show"])).expect("show exists");
    assert_eq!(show.body, vec!["echo root-1 {{libonly}}".to_string()]);
    let libshow = v.get(&path(&["libshow"])).expect("libshow exists");
    assert_eq!(libshow.body, vec!["echo lib-2".to_string()]);
}

#[test]
fn same_constant_in_two_files_is_not_a_clash() {
    let sb = Sandbox::new();
    sb.write("tasks.alors", "import \"lib.alors\"\nx := 1\na:\n    echo {{x}}\n");
    sb.write("lib.alors", "x := 2\nb:\n    echo {{x}}\n");

    let v = loader::load(&sb.path("tasks.alors")).expect("file-local constants never clash");
    assert_eq!(v.get(&path(&["a"])).unwrap().body, vec!["echo 1".to_string()]);
    assert_eq!(v.get(&path(&["b"])).unwrap().body, vec!["echo 2".to_string()]);
}
