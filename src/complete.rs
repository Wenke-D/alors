//! Shell completion: the scripts alors ships, and the candidate lookup they
//! call back into (`alors --complete`).
//!
//! The scripts are deliberately dumb pipes. Rather than teaching each shell the
//! task model, a script collects the words typed so far and asks the binary
//! what fits next — so a new flag, or a task added to `tasks.alors` a second
//! ago, needs no regenerating of what the user already installed.

use crate::parser::Taskfile;

/// Every flag alors accepts, in help order. Offered only when the word being
/// completed starts with `-` in first position: flags are never task arguments.
const FLAGS: &[&str] = &[
    "--taskfile",
    "--help",
    "--help-ai",
    "--version",
    "--completion-script",
    "--complete",
];

/// The shells `--completion-script` can emit a script for.
pub const SHELLS: &[&str] = &["zsh"];

/// The completion script for `shell`, or `None` if it isn't one we ship.
pub fn script(shell: &str) -> Option<&'static str> {
    match shell {
        "zsh" => Some(include_str!("../completions/alors.zsh")),
        _ => None,
    }
}

/// Candidates for the word currently being completed.
///
/// `words` is everything typed after `alors`, the word under the cursor last —
/// empty when the cursor sits after a space. The result is already filtered by
/// that word, so a script can offer it verbatim. An empty result means "nothing
/// to suggest here", which scripts turn into ordinary file completion: past a
/// task's name, the remaining words are its arguments, and those are usually
/// paths.
///
/// `taskfile` is whichever file those words point at — the one in the current
/// directory, or the one named by a leading `--taskfile`, which the caller
/// resolves before calling.
pub fn candidates(taskfile: Option<&Taskfile>, words: &[String]) -> Vec<String> {
    let (cur, typed) = match words.split_last() {
        Some((cur, typed)) => (cur.as_str(), typed),
        None => ("", &[] as &[String]),
    };

    // Flags only make sense in first position, and only once the user has
    // committed to one by typing `-`.
    if typed.is_empty() && cur.starts_with('-') {
        return matching(FLAGS.iter().copied(), cur);
    }

    // `--taskfile` takes a path. Offering nothing hands the word to the shell's
    // own file completion, which is better at paths than we would be.
    if typed == ["--taskfile"] {
        return Vec::new();
    }

    // Past that path, the words read as an ordinary task path — against the
    // file they point at, which the caller has already loaded for us.
    let typed = match typed {
        [flag, _path, rest @ ..] if flag == "--taskfile" => rest,
        _ => typed,
    };

    // `--completion-script` is the one flag that takes a value: the shell name.
    if typed.first().map(String::as_str) == Some("--completion-script") {
        return if typed.len() == 1 {
            matching(SHELLS.iter().copied(), cur)
        } else {
            Vec::new()
        };
    }

    // Every other flag stands alone — nothing follows it.
    if typed.first().is_some_and(|w| w.starts_with('-')) {
        return Vec::new();
    }

    // Otherwise the words typed so far are a task path, and what fits next is
    // whatever sits directly under it: sub-tasks and namespaces.
    match taskfile {
        Some(taskfile) => matching(taskfile.children(typed), cur),
        None => Vec::new(),
    }
}

fn matching<I: IntoIterator<Item = S>, S: AsRef<str>>(items: I, prefix: &str) -> Vec<String> {
    items
        .into_iter()
        .filter(|item| item.as_ref().starts_with(prefix))
        .map(|item| item.as_ref().to_string())
        .collect()
}
