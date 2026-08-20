//! End-to-end tests: these spawn the real binary, where every other test file
//! drives the modules directly. What only shows up here is the wiring — flag
//! parsing, exit codes, which directory a body actually runs in.
//!
//! `--taskfile` is what makes them cheap: one fixtures directory can hold many
//! taskfiles side by side, instead of one directory per case.

use std::path::Path;
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/basic.alors";

/// Run the binary from the crate root, the way a user standing in the project
/// would.
fn alors(args: &[&str]) -> Output {
    alors_in(env!("CARGO_MANIFEST_DIR"), args)
}

/// Run it from somewhere else — for the cases that are *about* the directory.
fn alors_in(dir: impl AsRef<Path>, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_alors"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the alors binary runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("stdout is utf-8")
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("stderr is utf-8")
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("the process exited normally")
}

#[test]
fn a_body_runs_beside_its_own_taskfile() {
    // The whole point of --taskfile: `alors --taskfile X` is `cd $(dirname X)`,
    // so a body's relative paths mean what they mean in *its* project.
    let out = alors(&["--taskfile", FIXTURE, "where"]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert!(
        Path::new(stdout(&out).trim()).ends_with("tests/fixtures"),
        "body ran in {:?}, not the fixture's directory",
        stdout(&out).trim()
    );
}

#[test]
fn a_pointed_at_taskfile_runs_tasks_normally() {
    let out = alors(&["--taskfile", FIXTURE, "greet", "world"]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "hello world");

    let out = alors(&["--taskfile", FIXTURE, "ns", "sub"]);
    assert_eq!(stdout(&out).trim(), "in the namespace");

    // The bare namespace falls back to its same-named default task.
    let out = alors(&["--taskfile", FIXTURE, "ns"]);
    assert_eq!(stdout(&out).trim(), "the namespace default");
}

#[test]
fn a_flag_after_the_task_name_is_one_of_its_arguments() {
    // Flags are recognized before the task only. Past it the resolver's rule
    // holds: remaining tokens are arguments, whatever they look like.
    let out = alors(&["--taskfile", FIXTURE, "greet", "--taskfile"]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "hello --taskfile");
}

#[test]
fn a_missing_taskfile_names_the_path_that_was_asked_for() {
    let out = alors(&["--taskfile", "nope.alors", "where"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("no taskfile at `nope.alors`"), "{}", stderr(&out));
}

#[test]
fn taskfile_without_a_path_is_an_error() {
    let out = alors(&["--taskfile"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("needs a path"), "{}", stderr(&out));
}

#[test]
fn a_directory_without_a_taskfile_points_at_help() {
    // tests/fixtures holds taskfiles, but nothing named `tasks.alors`.
    let out = alors_in("tests/fixtures", &[]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("no `tasks.alors`"), "{}", stderr(&out));
    assert!(stderr(&out).contains("--help"), "{}", stderr(&out));
}

#[test]
fn help_and_version_work_without_any_taskfile() {
    let out = alors_in("tests/fixtures", &["--help"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("Usage:"));

    let out = alors_in("tests/fixtures", &["--version"]);
    assert_eq!(code(&out), 0);
    assert_eq!(stdout(&out).trim(), format!("alors {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn completion_reads_the_taskfile_it_is_pointed_at() {
    let out = alors(&["--complete", "--taskfile", FIXTURE, ""]);
    assert_eq!(code(&out), 0);
    let listed = stdout(&out);
    let names: Vec<&str> = listed.lines().map(str::trim).collect();
    assert_eq!(names, vec!["greet", "ns", "where"]);

    let out = alors(&["--complete", "--taskfile", FIXTURE, "ns", ""]);
    assert_eq!(stdout(&out).trim(), "sub");
}

#[test]
fn completion_stays_silent_and_successful_outside_a_project() {
    // A completion function must never spew into the prompt.
    let out = alors_in("tests/fixtures", &["--complete", ""]);
    assert_eq!(code(&out), 0);
    assert_eq!(stdout(&out), "");
    assert_eq!(stderr(&out), "");
}

#[test]
fn the_completion_script_is_printed_for_a_shell_we_ship() {
    let out = alors(&["--completion-script", "zsh"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).starts_with("#compdef alors"), "{}", stdout(&out));

    let out = alors(&["--completion-script", "bash"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("supported shells: zsh"), "{}", stderr(&out));
}
