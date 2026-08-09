//! via — a CLI for your project.
//!
//! v0 entry point. Enforces the strict cwd rule (a `viafile` must exist in the
//! current directory), parses it, and resolves/executes the requested task.

mod executor;
mod loader;
mod parser;
mod resolver;
mod validate;

use std::path::Path;
use std::process::exit;

fn print_listing(viafile: &parser::Viafile) {
    let top = viafile.children(&[]);
    if top.is_empty() {
        println!("viafile has no tasks.");
        return;
    }
    println!("Available commands:");
    for name in &top {
        let path = vec![name.clone()];
        let is_task = viafile.get(&path).is_some();
        let is_ns = viafile.is_namespace(&path);
        let marker = match (is_task, is_ns) {
            (true, true) => format!("{} (+ subcommands)", name),
            (false, true) => format!("{} <subcommand>", name),
            _ => name.clone(),
        };
        if let Some(r) = viafile.get(&path) {
            if !r.params.is_empty() {
                let ps: Vec<String> = r
                    .params
                    .iter()
                    .map(|p| if p.optional { format!("[{}]", p.name) } else { p.name.clone() })
                    .collect();
                println!("  {}  {}", marker, ps.join(" "));
                continue;
            }
        }
        println!("  {}", marker);
    }
}

fn print_help(viafile: Option<&parser::Viafile>) {
    println!("via — a CLI for your project");
    println!();
    println!("Usage:");
    println!("  via                     list the tasks in the viafile");
    println!("  via <task> [args...]    run a task (namespaced: via docker build)");
    println!("  via help | --help | -h  show this help");
    println!("  via --help-ai           usage notes for AI agents (machine-oriented)");
    println!();
    println!("via reads the `viafile` in the current directory.");
    if let Some(v) = viafile {
        println!();
        print_listing(v);
    }
}

fn print_help_ai() {
    println!(
        r#"# via — usage notes for AI agents

via is a project-local CLI. It reads the `viafile` in the current working
directory (never parent directories) and runs the named task from it.
Source & docs: https://github.com/Wenke-D/via

## Invoking
- `via`                    list available tasks in this project
- `via <task> [args...]`   run one task; positional args fill its parameters
- `via <ns> <task>`        run a namespaced task (defined as `ns::task`)

Exactly ONE task per invocation. Tokens after the task are its arguments or
a subcommand path — never additional tasks. To run things in sequence,
declare dependencies in the viafile instead.

## Exit codes
- 0  success
- 1  resolution error (unknown task, wrong argument count)
- 2  environment error (missing viafile, parse error, validation error)
- otherwise: the exit code of the failing shell command

## viafile syntax (when reading or writing one)
- `name:` then an indented body of shell commands defines a task.
- `name param1 [param2]:` declares positional parameters; `[x]` is optional.
  Use them in the body as `{{{{param}}}}` or `$param`.
- `name := value` (top level) defines a constant: an immutable literal usable
  as `{{{{name}}}}` in bodies and dependency arguments. File-local (imports
  neither see nor leak it); a task param with the same name takes precedence.
  No logic in values — computation belongs in shell code inside task bodies.
- `name: dep1, dep2 arg` — comma-separated dependencies run first, in order,
  deps-first; a dep may carry fixed positional args. Same dep + same args
  runs at most once per invocation.
- `ns::name:` groups a task under a namespace (invoked `via ns name`).
  A plain `ns:` task alongside it is the namespace default for `via ns`.
- `import "file.via"` merges tasks flat; `import "file.via" as ns` nests
  the whole file under a namespace.
- Each body runs in a single `sh` with `set -e` (fail-fast; `cd` and
  variables persist across lines). Start a body with `set +e` to tolerate
  failures.
- The whole file is validated before anything runs: name clashes, unknown
  deps, wrong arg counts, and cycles are up-front errors.

Run `via` (no arguments) to list this project's tasks."#
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ai_help = args.first().map(String::as_str) == Some("--help-ai");
    let help_requested = ai_help
        || matches!(
            args.first().map(String::as_str),
            Some("help" | "--help" | "-h")
        );
    let show_help = |viafile: Option<&parser::Viafile>| {
        if ai_help {
            // The AI notes describe the tool itself; task discovery is `via`.
            print_help_ai();
        } else {
            print_help(viafile);
        }
    };

    let viafile_path = Path::new("viafile");
    if !viafile_path.exists() {
        // Help still works without a viafile — just without the task listing.
        if help_requested {
            show_help(None);
            exit(0);
        }
        eprintln!("via: no `viafile` in the current directory");
        exit(2);
    }

    // Read the root viafile and resolve its imports into one merged model.
    let viafile = match loader::load(viafile_path) {
        Ok(v) => v,
        Err(e) => {
            if help_requested {
                show_help(None);
                exit(0);
            }
            eprintln!("via: {}", e);
            exit(2);
        }
    };

    // Phase 1: validate the whole dependency graph before running anything.
    if let Err(errors) = validate::validate(&viafile) {
        if help_requested {
            show_help(None);
            exit(0);
        }
        for e in &errors {
            eprintln!("via: {}", e);
        }
        exit(2);
    }

    if help_requested {
        // The flags always mean help; the bare word defers to a task the
        // viafile legitimately named `help`, mirroring the `import:` rule.
        let help_path = vec!["help".to_string()];
        let user_defined_help = args[0] == "help"
            && (viafile.get(&help_path).is_some() || viafile.is_namespace(&help_path));
        if !user_defined_help {
            show_help(Some(&viafile));
            exit(0);
        }
    }

    if args.is_empty() {
        print_listing(&viafile);
        exit(0);
    }

    match resolver::resolve(&viafile, &args) {
        Ok(resolved) => {
            let code = executor::execute(&viafile, &resolved);
            exit(code);
        }
        Err(e) => {
            eprintln!("via: {}", e);
            exit(1);
        }
    }
}
