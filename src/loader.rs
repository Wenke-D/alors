//! The import loader: turns a root `tasks.alors` plus its `import` directives into a
//! single merged [`Taskfile`] for the rest of the pipeline (validate/resolve/run).
//!
//! Design, matching alors's "everything explicit, checked up front" stance:
//!   - **Pure parser, IO here.** [`crate::parser::parse`] stays filesystem-free
//!     and only *records* imports; this module does the reads and the merge.
//!   - **Two import shapes.** `import "x"` merges flat (imported tasks keep their
//!     names); `import "x" as ns` nests the whole file under `ns` — every
//!     imported task's path *and its internal dependency references* are
//!     prefixed, so the CLI form (`alors ns task`) and dep form (`ns::task`) stay
//!     the single unified path alors already uses.
//!   - **All clashes are errors.** Any two files defining the same final task
//!     name is a hard error, reported before anything runs. Namespacing
//!     (`as ns`) is how you disambiguate.
//!   - **Transitive + cycle-safe.** Imported files may import in turn; a file
//!     reachable from itself is reported as an import cycle, not followed.
//!
//! Paths in an `import` are resolved relative to the directory of the file that
//! contains the directive.

use crate::parser::{self, Task, Taskfile};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum LoadError {
    /// A file (root or imported) could not be read. `from` names the `import`
    /// directive that referenced it, when there is one.
    Read {
        path: String,
        err: String,
        from: Option<(String, usize)>,
    },
    /// A parse error within a specific file.
    Parse {
        source: String,
        line: usize,
        message: String,
    },
    /// `import` directives form a cycle; `chain` is the offending file loop.
    Cycle { chain: Vec<String> },
    /// The same final task name is defined in two places.
    Clash {
        name: String,
        first: (String, usize),
        second: (String, usize),
    },
    /// A task outside an `as` import lives *inside* the namespace that import
    /// introduced. An `as` namespace is sealed: its tasks must all come from
    /// the one import. (Giving the namespace a bare default task is still fine.)
    SealedNamespace {
        namespace: String,
        import_site: (String, usize),
        task: String,
        task_site: (String, usize),
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Read { path, err, from } => {
                write!(f, "cannot read `{}`: {}", path, err)?;
                if let Some((label, line)) = from {
                    write!(f, " (imported from {}:{})", label, line)?;
                }
                Ok(())
            }
            LoadError::Parse {
                source,
                line,
                message,
            } => write!(f, "{}:{}: {}", source, line, message),
            LoadError::Cycle { chain } => {
                write!(f, "import cycle: {}", chain.join(" -> "))
            }
            LoadError::Clash {
                name,
                first,
                second,
            } => write!(
                f,
                "task `{}` is defined in both {}:{} and {}:{}",
                name, first.0, first.1, second.0, second.1
            ),
            LoadError::SealedNamespace {
                namespace,
                import_site,
                task,
                task_site,
            } => write!(
                f,
                "{}:{}: `{}` cannot be added to namespace `{}`, which is filled by the import at {}:{}; \
                 an `as` namespace's tasks must all come from that one import \
                 (a bare `{}:` default task is allowed, but new sub-tasks are not)",
                task_site.0,
                task_site.1,
                task,
                namespace,
                import_site.0,
                import_site.1,
                namespace
            ),
        }
    }
}

/// Load `path` and everything it imports into one merged [`Taskfile`].
pub fn load(path: &Path) -> Result<Taskfile, LoadError> {
    let mut merged = Taskfile::default();
    let mut stack: Vec<(PathBuf, String)> = Vec::new();
    load_into(path, None, &mut merged, &mut stack)?;
    Ok(merged)
}

/// Parse `path`, merge its own tasks into `merged`, then recurse into its
/// imports. `from` is the `import` directive that pulled `path` in (for error
/// messages); `stack` holds the canonical paths currently being loaded, so a
/// file that imports (transitively) back into itself is caught as a cycle.
fn load_into(
    path: &Path,
    from: Option<(String, usize)>,
    merged: &mut Taskfile,
    stack: &mut Vec<(PathBuf, String)>,
) -> Result<(), LoadError> {
    let label = path.display().to_string();

    // Canonicalize for cycle detection. This also surfaces a missing import as
    // a clean read error rather than a panic deeper in.
    let canon = std::fs::canonicalize(path).map_err(|e| LoadError::Read {
        path: label.clone(),
        err: e.to_string(),
        from: from.clone(),
    })?;
    if stack.iter().any(|(c, _)| c == &canon) {
        let mut chain: Vec<String> = stack.iter().map(|(_, l)| l.clone()).collect();
        chain.push(label);
        return Err(LoadError::Cycle { chain });
    }

    let src = std::fs::read_to_string(path).map_err(|e| LoadError::Read {
        path: label.clone(),
        err: e.to_string(),
        from: from.clone(),
    })?;
    let parsed = parser::parse(&src).map_err(|e| LoadError::Parse {
        source: label.clone(),
        line: e.line,
        message: e.message,
    })?;

    stack.push((canon, label.clone()));

    // This file's own tasks, stamped with its label for later error messages.
    for (_, mut task) in parsed.tasks {
        task.source = label.clone();
        merge_one(merged, task)?;
    }

    // Imported files, resolved relative to this file's directory.
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    for import in &parsed.imports {
        let child = base.join(&import.path);
        let mut sub = Taskfile::default();
        load_into(&child, Some((label.clone(), import.line)), &mut sub, stack)?;
        if let Some(ns) = &import.alias {
            apply_namespace(&mut sub, ns);
            // The `as` namespace is sealed: nothing already merged at this level
            // may live *inside* it. Catches both extending it with a new sub-task
            // and (redundantly with the clash check) redefining one of its tasks.
            // A bare `ns` default task has path == ns, so it is *not* "inside".
            if let Some(intruder) = merged.tasks.values().find(|r| strictly_under(&r.path, ns)) {
                return Err(LoadError::SealedNamespace {
                    namespace: ns.join("::"),
                    import_site: (label.clone(), import.line),
                    task: intruder.path.join("::"),
                    task_site: (intruder.source.clone(), intruder.line),
                });
            }
        }
        for (_, task) in sub.tasks {
            merge_one(merged, task)?;
        }
    }

    stack.pop();
    Ok(())
}

/// Insert `task` into `merged`, treating any name collision as a hard error.
fn merge_one(merged: &mut Taskfile, task: Task) -> Result<(), LoadError> {
    let key = task.path.join("::");
    if let Some(existing) = merged.tasks.get(&key) {
        return Err(LoadError::Clash {
            name: key,
            first: (existing.source.clone(), existing.line),
            second: (task.source.clone(), task.line),
        });
    }
    merged.tasks.insert(key, task);
    Ok(())
}

/// Nest every task in `sub` under namespace `ns`: prefix each task's path
/// and each of its dependency references. Because an imported file's deps point
/// only within its own (sub)tree, prefixing them uniformly keeps them resolving.
fn apply_namespace(sub: &mut Taskfile, ns: &[String]) {
    let old = std::mem::take(&mut sub.tasks);
    for (_, mut task) in old {
        task.path = prefixed(ns, &task.path);
        for dep in &mut task.deps {
            // Only the target path is namespaced; args are values, not paths.
            dep.path = prefixed(ns, &dep.path);
        }
        sub.tasks.insert(task.path.join("::"), task);
    }
}

fn prefixed(ns: &[String], path: &[String]) -> Vec<String> {
    let mut out = ns.to_vec();
    out.extend_from_slice(path);
    out
}

/// True if `path` is strictly inside namespace `ns` (a descendant, not `ns`
/// itself). `["ci","test"]` is under `["ci"]`; `["ci"]` is not.
fn strictly_under(path: &[String], ns: &[String]) -> bool {
    path.len() > ns.len() && path.starts_with(ns)
}
