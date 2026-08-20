//! alors — a CLI for your project.
//!
//! v0 entry point. Enforces the strict cwd rule (a `tasks.alors` must exist in the
//! current directory), parses it, and resolves/executes the requested task.

mod complete;
mod executor;
mod loader;
mod parser;
mod resolver;
mod validate;

use std::path::{Path, PathBuf};
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
    println!("  alors                              list the tasks in tasks.alors");
    println!("  alors <task> [args...]             run a task (namespaced: alors docker build)");
    println!("  alors --taskfile <path> <task>     run a task from that file, in its directory");
    println!("  alors --help                       show this help");
    println!("  alors --help-ai                    usage notes for AI agents");
    println!("  alors --version                    print the version");
    println!("  alors --completion-script <shell>  print a shell completion script (zsh)");
    println!("  alors --complete [words...]        completion candidates, for those scripts");
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
- `alors --taskfile <path> <task>`   run a task from a taskfile elsewhere; the
  task runs in THAT file's directory, exactly as `cd $(dirname path) && alors
  <task>` would. Recognized before the task name only — after it, `--taskfile`
  is just one of the task's arguments.

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

/// Split a leading `--taskfile <path>` off the arguments.
///
/// The flag is recognized in first position only. Everything after the task
/// name belongs to the task — a `--taskfile` there is one of its arguments, not
/// a flag — which is what keeps the resolver's rule intact: once a task is
/// selected, remaining tokens are arguments only.
fn take_taskfile(args: &[String]) -> Result<(Option<PathBuf>, &[String]), String> {
    match args.first().map(String::as_str) {
        Some("--taskfile") => match args.get(1) {
            Some(path) => Ok((Some(PathBuf::from(path)), &args[2..])),
            None => Err("--taskfile needs a path".to_string()),
        },
        _ => Ok((None, args)),
    }
}

/// Move to the taskfile's own directory, so `--taskfile X <task>` means exactly
/// `cd $(dirname X) && alors <task>`.
///
/// A body is a shell script full of paths relative to its project — `cargo
/// build`, `cmake -S . -B build`, `cp target/release/alors`. Running another
/// project's commands from here would apply them to *this* directory, quietly
/// and wrongly. It is also what the loader already does with imports: they
/// resolve relative to the importing file, never to the current directory.
fn enter_taskfile_dir(taskfile_path: &Path) -> Result<(), String> {
    match taskfile_path.parent() {
        // No parent component means the file is already in the current
        // directory — the ordinary case, and nothing to do.
        Some(dir) if !dir.as_os_str().is_empty() => std::env::set_current_dir(dir)
            .map_err(|e| format!("cannot enter `{}`: {}", dir.display(), e)),
        _ => Ok(()),
    }
}

/// Best-effort, silent load of a valid taskfile, for the paths that must work
/// with or without one: help (which the taskfile only enriches with a listing)
/// and completion (which must never spew errors into a shell prompt). Any
/// problem — missing file, parse error, validation error — just means `None`.
fn load_quietly(path: &Path) -> Option<parser::Taskfile> {
    if !path.exists() {
        return None;
    }
    let taskfile = loader::load(path).ok()?;
    validate::validate(&taskfile).is_ok().then_some(taskfile)
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    // `--taskfile <path>` comes off the front first: it decides *which* file
    // every later step reads, so everything downstream sees the same argument
    // list it would have seen from inside that project.
    let (named_taskfile, args) = match take_taskfile(&argv) {
        Ok(split) => split,
        Err(e) => {
            eprintln!("alors: {}", e);
            exit(2);
        }
    };
    let first = args.first().map(String::as_str);
    let ai_help = first == Some("--help-ai");
    let help_requested = ai_help || first == Some("--help");

    // Like help, the version never depends on the taskfile. The value comes from
    // Cargo.toml at compile time, so it cannot drift from the release tag.
    if first == Some("--version") {
        println!("alors {}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }

    // Printing a completion script is likewise taskfile-independent: the script
    // is static, and everything project-specific reaches it later, at <TAB>
    // time, through `--complete`.
    if first == Some("--completion-script") {
        let shell = args.get(1).map(String::as_str);
        match shell.and_then(complete::script) {
            Some(script) => {
                print!("{}", script);
                exit(0);
            }
            None => {
                match shell {
                    Some(s) => eprintln!("alors: no completion script for `{}`", s),
                    None => eprintln!("alors: --completion-script needs a shell"),
                }
                eprintln!("  supported shells: {}", complete::SHELLS.join(", "));
                exit(2);
            }
        }
    }

    let default_path = PathBuf::from("tasks.alors");
    let taskfile_path: &Path = named_taskfile.as_deref().unwrap_or(&default_path);

    // Help comes first, before the taskfile is required to exist (or be valid):
    // a loadable taskfile only adds the task listing to the output. Help is
    // flag-only (`--help`); the bare word `help` is an ordinary task name, so
    // a task you named `help` is invoked like any other.
    if help_requested {
        if ai_help {
            // The AI notes describe the tool itself; task discovery is `alors`.
            print_help_ai();
        } else {
            print_help(load_quietly(taskfile_path).as_ref());
        }
        exit(0);
    }

    // A completion script calls this on every <TAB>, so it never fails and never
    // prints a diagnostic: outside a project, or on a broken taskfile, there is
    // simply nothing to suggest.
    if first == Some("--complete") {
        // The words being completed are their own little command line, and may
        // carry a `--taskfile` of their own: complete against the file the user
        // is pointing at, not the one in this directory.
        let words = &args[1..];
        let pointed_at = take_taskfile(words).ok().and_then(|(path, _)| path);
        let taskfile = load_quietly(pointed_at.as_deref().unwrap_or(taskfile_path));
        for candidate in complete::candidates(taskfile.as_ref(), words) {
            println!("{}", candidate);
        }
        exit(0);
    }

    if !taskfile_path.exists() {
        match named_taskfile {
            Some(ref path) => eprintln!("alors: no taskfile at `{}`", path.display()),
            None => {
                eprintln!("alors: no `tasks.alors` in the current directory");
                eprintln!("  run `alors --help` to see how to get started");
            }
        }
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

    match resolver::resolve(&taskfile, args) {
        Ok(resolved) => {
            // Everything above read the file from where the user is standing;
            // from here on the task runs beside its own taskfile.
            if let Err(e) = enter_taskfile_dir(taskfile_path) {
                eprintln!("alors: {}", e);
                exit(2);
            }
            let code = executor::execute(&taskfile, &resolved);
            exit(code);
        }
        Err(e) => {
            eprintln!("alors: {}", e);
            exit(1);
        }
    }
}
