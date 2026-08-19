//! The CLI resolver: implements the strict `alors` parse model.
//!
//! Rules (final spec):
//!   1. One task per invocation.
//!   2. Greedy path descent: a token matching a sub-task/namespace is ALWAYS
//!      path (it shadows any same-named argument). No `--` escape.
//!   3. Once a task is selected, remaining tokens are arguments only.
//!   4. Each argument must fill a declared positional parameter, else hard error.
//!   5. A namespace's default task is the same-named plain task. If absent,
//!      bare invocation of the namespace is an error listing its sub-tasks.
//!   6. `--` is illegal anywhere.

use crate::parser::{Task, Taskfile};

#[derive(Debug)]
pub enum ResolveError {
    /// `--` was used.
    DashDashForbidden,
    /// A token during path descent matched nothing.
    UnknownCommand {
        token: String,
        scope: Vec<String>,
        available: Vec<String>,
    },
    /// Bare namespace with no default task.
    NamespaceNeedsSubcommand {
        path: Vec<String>,
        available: Vec<String>,
    },
    /// Extra token that is neither a subcommand nor an accepted parameter.
    NotSubcommandNorParam {
        token: String,
        task: String,
        takes_args: bool,
    },
    /// Required parameters were not supplied.
    MissingParams {
        task: String,
        missing: Vec<String>,
    },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::DashDashForbidden => {
                write!(f, "`--` is not allowed in alors invocations")
            }
            ResolveError::UnknownCommand {
                token,
                scope,
                available,
            } => {
                let where_ = if scope.is_empty() {
                    "".to_string()
                } else {
                    format!(" in `{}`", scope.join(" "))
                };
                write!(
                    f,
                    "`{}` is not a known command{}. available: {}",
                    token,
                    where_,
                    if available.is_empty() {
                        "(none)".to_string()
                    } else {
                        available.join(", ")
                    }
                )
            }
            ResolveError::NamespaceNeedsSubcommand { path, available } => write!(
                f,
                "`{}` is a namespace, choose a subcommand: {}",
                path.join(" "),
                available.join(", ")
            ),
            ResolveError::NotSubcommandNorParam {
                token,
                task,
                takes_args,
            } => {
                if *takes_args {
                    write!(
                        f,
                        "`{}` is not a subcommand, and `{}` has no more parameters to fill",
                        token, task
                    )
                } else {
                    write!(
                        f,
                        "`{}` is not a subcommand, and `{}` takes no arguments",
                        token, task
                    )
                }
            }
            ResolveError::MissingParams { task, missing } => write!(
                f,
                "`{}` is missing required argument(s): {}",
                task,
                missing.join(", ")
            ),
        }
    }
}

/// A fully resolved invocation: which task to run and the argument bindings.
#[derive(Debug)]
pub struct Resolved<'a> {
    pub task: &'a Task,
    /// param name -> value
    pub args: Vec<(String, String)>,
}

/// Resolve CLI tokens against the parsed taskfile.
pub fn resolve<'a>(taskfile: &'a Taskfile, tokens: &[String]) -> Result<Resolved<'a>, ResolveError> {
    // Rule 6: `--` is illegal anywhere.
    if tokens.iter().any(|t| t == "--") {
        return Err(ResolveError::DashDashForbidden);
    }

    let mut path: Vec<String> = Vec::new();
    let mut idx = 0;

    // Phase 1: greedy path descent.
    loop {
        let next = tokens.get(idx);

        match next {
            Some(tok) => {
                let mut candidate = path.clone();
                candidate.push(tok.clone());

                let is_task = taskfile.get(&candidate).is_some();
                let is_ns = taskfile.is_namespace(&candidate);

                if is_task || is_ns {
                    // Token is part of the path (path always wins). Descend.
                    path.push(tok.clone());
                    idx += 1;

                    if is_task && !is_ns {
                        // Pure task (leaf). Stop descent, go to args phase.
                        break;
                    }
                    if is_task && is_ns {
                        // Name is both a task (default) and a namespace.
                        // Peek: does the NEXT token descend further into the namespace?
                        if let Some(peek) = tokens.get(idx) {
                            let mut deeper = path.clone();
                            deeper.push(peek.clone());
                            if taskfile.get(&deeper).is_some() || taskfile.is_namespace(&deeper) {
                                // Continue descending; path wins.
                                continue;
                            }
                        }
                        // Next token isn't a sub-path -> select this default task,
                        // remaining tokens become its args.
                        break;
                    }
                    // Pure namespace: keep descending.
                    continue;
                } else {
                    // Token matches nothing at this scope.
                    if path.is_empty() {
                        // First token unknown -> unknown top-level command.
                        return Err(ResolveError::UnknownCommand {
                            token: tok.clone(),
                            scope: vec![],
                            available: taskfile.children(&[]),
                        });
                    }
                    // We're mid-path at a namespace with no task here, or at a
                    // task already (handled above). If current path is a task,
                    // we wouldn't be here. So current path is a namespace ->
                    // it needs a default task to absorb this token as an arg.
                    if taskfile.get(&path).is_some() {
                        // Has default task; token is an argument. Stop descent.
                        break;
                    }
                    // Namespace with no default task but an extra token given.
                    return Err(ResolveError::UnknownCommand {
                        token: tok.clone(),
                        scope: path.clone(),
                        available: taskfile.children(&path),
                    });
                }
            }
            None => {
                // Ran out of tokens during descent.
                break;
            }
        }
    }

    // After Phase 1, `path` is either a task or a bare namespace.
    let task = match taskfile.get(&path) {
        Some(r) => r,
        None => {
            // Bare namespace (or empty). Needs a subcommand.
            if path.is_empty() {
                // `alors` with no args and no top-level default -> list everything.
                return Err(ResolveError::NamespaceNeedsSubcommand {
                    path: vec![],
                    available: taskfile.children(&[]),
                });
            }
            return Err(ResolveError::NamespaceNeedsSubcommand {
                path: path.clone(),
                available: taskfile.children(&path),
            });
        }
    };

    // Phase 2: remaining tokens are arguments only (no path resolution).
    let rest = &tokens[idx..];
    let task_name = task.display_name();

    if rest.len() > task.params.len() {
        // Too many args. Report the first offending token.
        let offending = &rest[task.params.len()];
        return Err(ResolveError::NotSubcommandNorParam {
            token: offending.clone(),
            task: task_name,
            takes_args: !task.params.is_empty(),
        });
    }

    // Bind args positionally.
    let mut args = Vec::new();
    for (k, param) in task.params.iter().enumerate() {
        if let Some(val) = rest.get(k) {
            args.push((param.clone(), val.clone()));
        } else {
            // Missing params. Collect all of them for a good message.
            return Err(ResolveError::MissingParams {
                task: task_name,
                missing: task.params[k..].to_vec(),
            });
        }
    }

    Ok(Resolved { task, args })
}
