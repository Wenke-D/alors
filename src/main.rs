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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let viafile_path = Path::new("viafile");
    if !viafile_path.exists() {
        eprintln!("via: no `viafile` in the current directory");
        exit(2);
    }

    // Read the root viafile and resolve its imports into one merged model.
    let viafile = match loader::load(viafile_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("via: {}", e);
            exit(2);
        }
    };

    // Phase 1: validate the whole dependency graph before running anything.
    if let Err(errors) = validate::validate(&viafile) {
        for e in &errors {
            eprintln!("via: {}", e);
        }
        exit(2);
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
