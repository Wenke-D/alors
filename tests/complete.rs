#[path = "../src/complete.rs"]
mod complete;
#[path = "../src/parser.rs"]
mod parser;

use complete::{candidates, script, SHELLS};

const SAMPLE: &str = r#"
build:
    cmake --build

build::release:
    make --preset release

docker::build:
    docker build -t app .

docker::push:
    docker push app

greet name:
    echo hello {{name}}
"#;

fn vf(src: &str) -> parser::Taskfile {
    parser::parse(src).expect("parse ok")
}

/// The words a shell hands us: everything typed after `alors`, the word under
/// the cursor last (empty when the cursor sits after a space).
fn words(w: &[&str]) -> Vec<String> {
    w.iter().map(|s| s.to_string()).collect()
}

fn complete_in(src: &str, w: &[&str]) -> Vec<String> {
    candidates(Some(&vf(src)), &words(w))
}

#[test]
fn empty_word_offers_every_top_level_name() {
    // `docker` is a namespace with no task of its own — still a valid next word.
    assert_eq!(
        complete_in(SAMPLE, &[""]),
        vec!["build", "docker", "greet"]
    );
}

#[test]
fn no_words_at_all_is_the_same_as_an_empty_word() {
    assert_eq!(candidates(Some(&vf(SAMPLE)), &[]), vec!["build", "docker", "greet"]);
}

#[test]
fn candidates_are_filtered_by_the_word_being_typed() {
    assert_eq!(complete_in(SAMPLE, &["d"]), vec!["docker"]);
    assert_eq!(complete_in(SAMPLE, &["zzz"]), Vec::<String>::new());
}

#[test]
fn a_typed_namespace_offers_its_subcommands() {
    assert_eq!(complete_in(SAMPLE, &["docker", ""]), vec!["build", "push"]);
    assert_eq!(complete_in(SAMPLE, &["docker", "p"]), vec!["push"]);
    // `build` is both a task and a namespace.
    assert_eq!(complete_in(SAMPLE, &["build", ""]), vec!["release"]);
}

#[test]
fn past_a_leaf_task_there_is_nothing_to_suggest() {
    // The next word is an argument — the script falls back to file names.
    assert!(complete_in(SAMPLE, &["greet", ""]).is_empty());
    assert!(complete_in(SAMPLE, &["docker", "push", ""]).is_empty());
    assert!(complete_in(SAMPLE, &["nope", ""]).is_empty());
}

#[test]
fn a_leading_dash_offers_flags() {
    assert_eq!(
        complete_in(SAMPLE, &["-"]),
        vec!["--help", "--help-ai", "--version", "--completion-script", "--complete"]
    );
    assert_eq!(complete_in(SAMPLE, &["--he"]), vec!["--help", "--help-ai"]);
    // Only in first position: a task never takes flags.
    assert!(complete_in(SAMPLE, &["docker", "-"]).is_empty());
}

#[test]
fn completion_script_flag_offers_its_shells() {
    assert_eq!(complete_in(SAMPLE, &["--completion-script", ""]), SHELLS.to_vec());
    assert_eq!(complete_in(SAMPLE, &["--completion-script", "z"]), vec!["zsh"]);
    assert!(complete_in(SAMPLE, &["--completion-script", "zsh", ""]).is_empty());
}

#[test]
fn other_flags_stand_alone() {
    assert!(complete_in(SAMPLE, &["--version", ""]).is_empty());
    assert!(complete_in(SAMPLE, &["--help", "b"]).is_empty());
}

#[test]
fn without_a_taskfile_only_flags_complete() {
    // Outside a project <TAB> must stay quiet rather than error.
    assert!(candidates(None, &words(&[""])).is_empty());
    assert!(candidates(None, &words(&["docker", ""])).is_empty());
    assert_eq!(candidates(None, &words(&["--v"])), vec!["--version"]);
}

#[test]
fn every_supported_shell_ships_a_script() {
    for shell in SHELLS {
        let script = script(shell).unwrap_or_else(|| panic!("no script for {}", shell));
        // Each script must actually speak the protocol it is half of.
        assert!(script.contains("alors --complete"), "{} script is out of date", shell);
    }
    assert!(script("bash").is_none());
}
