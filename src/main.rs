//! alors — a CLI for your project.
//!
//! v0 entry point. Enforces the strict cwd rule (a `tasks.alors` must exist in the
//! current directory), parses it, and resolves/executes the requested task.

mod executor;
mod loader;
mod parser;
mod resolver;
mod validate;

use std::path::Path;
use std::process::exit;

fn print_listing(taskfile: &parser::Taskfile) {
    let top = taskfile.children(&[]);
    if top.is_empty() {
        println!("tasks.alors has no tasks.");
        return;
    }
    println!("Available commands:");
    for name in &top {
        let path = vec![name.clone()];
        let is_task = taskfile.get(&path).is_some();
        let is_ns = taskfile.is_namespace(&path);
        let marker = match (is_task, is_ns) {
            (true, true) => format!("{} (+ subcommands)", name),
            (false, true) => format!("{} <subcommand>", name),
            _ => name.clone(),
        };
        if let Some(r) = taskfile.get(&path) {
            if !r.params.is_empty() {
                println!("  {}  {}", marker, r.params.join(" "));
                continue;
            }
        }
        println!("  {}", marker);
    }
}

fn print_help(taskfile: Option<&parser::Taskfile>) {
    println!("alors — a CLI for your project");
    println!();
    println!("Usage:");
    println!("  alors                     list the tasks in tasks.alors");
    println!("  alors <task> [args...]    run a task (namespaced: alors docker build)");
    println!("  alors --help              show this help");
    println!("  alors --help-ai           usage notes for AI agents (machine-oriented)");
    println!("  alors --version           print the version");
    println!();
    println!("alors reads the `tasks.alors` file in the current directory.");
    if let Some(v) = taskfile {
        println!();
        print_listing(v);
    }
}

fn print_help_ai() {
    println!(
        r#"# alors — usage notes for AI agents

alors is a project-local CLI. It reads the `tasks.alors` file in the current
working directory (never parent directories) and runs the named task from it.
Source & docs: https://github.com/Wenke-D/alors

## Invoking
- `alors`                    list available tasks in this project
- `alors <task> [args...]`   run one task; positional args fill its parameters
- `alors <ns> <task>`        run a namespaced task (defined as `ns::task`)

Exactly ONE task per invocation. Tokens after the task are its arguments or
a subcommand path — never additional tasks. To run things in sequence,
declare dependencies in tasks.alors instead.

## Exit codes
- 0  success
- 1  resolution error (unknown task, wrong argument count)
- 2  environment error (missing tasks.alors, parse error, validation error)
- otherwise: the exit code of the failing shell command

## tasks.alors syntax (when reading or writing one)
- `name:` then an indented body of shell commands defines a task.
- `name param1 param2:` declares positional parameters — all required, bound
  in order. Use them in the body as `{{{{param}}}}` or `$param`.
- `name := value` (top level) defines a constant: an immutable literal usable
  as `{{{{name}}}}` in bodies and dependency arguments. File-local (imports
  neither see nor leak it); a task param with the same name takes precedence.
  No logic in values — computation belongs in shell code inside task bodies.
- `name: dep1, dep2 arg` — comma-separated dependencies run first, in order,
  deps-first; a dep may carry fixed positional args. Same dep + same args
  runs at most once per invocation.
- `ns::name:` groups a task under a namespace (invoked `alors ns name`).
  A plain `ns:` task alongside it is the namespace default for `alors ns`.
- `import "file.alors"` merges tasks flat; `import "file.alors" as ns` nests
  the whole file under a namespace.
- Each body runs in a single `sh` with `set -e` (fail-fast; `cd` and
  variables persist across lines). Start a body with `set +e` to tolerate
  failures.
- The whole file is validated before anything runs: name clashes, unknown
  deps, wrong arg counts, and cycles are up-front errors.

Run `alors` (no arguments) to list this project's tasks."#
    );
}

/// Best-effort load of a valid taskfile, purely to enrich help output with the
/// task listing. Any problem — missing file, parse error, validation error —
/// just means "no listing"; help itself never depends on the taskfile.
fn load_for_help(path: &Path) -> Option<parser::Taskfile> {
    if !path.exists() {
        return None;
    }
    let taskfile = loader::load(path).ok()?;
    validate::validate(&taskfile).is_ok().then_some(taskfile)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let first = args.first().map(String::as_str);
    let ai_help = first == Some("--help-ai");
    let help_requested = ai_help || first == Some("--help");

    // Like help, the version never depends on the taskfile. The value comes from
    // Cargo.toml at compile time, so it cannot drift from the release tag.
    if first == Some("--version") {
        println!("alors {}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }

    let taskfile_path = Path::new("tasks.alors");

    // Help comes first, before the taskfile is required to exist (or be valid):
    // a loadable taskfile only adds the task listing to the output. Help is
    // flag-only (`--help`); the bare word `help` is an ordinary task name, so
    // a task you named `help` is invoked like any other.
    if help_requested {
        if ai_help {
            // The AI notes describe the tool itself; task discovery is `alors`.
            print_help_ai();
        } else {
            print_help(load_for_help(taskfile_path).as_ref());
        }
        exit(0);
    }

    if !taskfile_path.exists() {
        eprintln!("alors: no `tasks.alors` in the current directory");
        exit(2);
    }

    // Read the root taskfile and resolve its imports into one merged model.
    let taskfile = match loader::load(taskfile_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("alors: {}", e);
            exit(2);
        }
    };

    // Phase 1: validate the whole dependency graph before running anything.
    if let Err(errors) = validate::validate(&taskfile) {
        for e in &errors {
            eprintln!("alors: {}", e);
        }
        exit(2);
    }

    if args.is_empty() {
        print_listing(&taskfile);
        exit(0);
    }

    match resolver::resolve(&taskfile, &args) {
        Ok(resolved) => {
            let code = executor::execute(&taskfile, &resolved);
            exit(code);
        }
        Err(e) => {
            eprintln!("alors: {}", e);
            exit(1);
        }
    }
}
