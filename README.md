# alors — a CLI for your project

Every project accumulates commands: configure, compile, test, package, clean.
`alors` gathers them into a single command-line tool for your project. You write
the commands once as named **tasks** in a `tasks.alors` file, and run them with
`alors <task>`.

It follows in the footsteps of [`make`](https://www.gnu.org/software/make/)
(originally written by Stuart Feldman at Bell Labs; GNU Make today) and
[`just`](https://github.com/casey/just) (by Casey Rodarmor) — the same idea of a
project-local file full of named tasks — and owes both a great deal. `alors`
keeps that core idea and makes a few different choices around how tasks are named
and invoked (see [Subcommands](#subcommands)).

## Install

Download the binary for your platform from the latest release, make it
executable, and drop it on your `PATH`:

**Linux (x86_64)**

```
curl -L -o alors https://github.com/Wenke-D/alors/releases/latest/download/alors-linux-x86_64
chmod +x alors && sudo mv alors /usr/local/bin/
```

**macOS (Apple Silicon)**

```
curl -L -o alors https://github.com/Wenke-D/alors/releases/latest/download/alors-macos-arm64
chmod +x alors && sudo mv alors /usr/local/bin/
```

The Linux binary is statically linked (musl), so it runs on any distribution
with nothing else to install. On macOS, Gatekeeper blocks unsigned downloads —
clear the quarantine flag with `xattr -d com.apple.quarantine alors` (or
right-click → Open the first time).

Prefer to build it yourself? `cargo build --release` (or `alors install`, which
builds and installs to `/usr/local/bin`).

## Shell completion

`alors` can complete your project's own task names — including namespaces, so
`alors docker <TAB>` offers that namespace's subcommands. The completion is
live: it reads the `tasks.alors` of the directory you're standing in, so a task
you add is completable immediately, with nothing to regenerate.

**zsh**

```
mkdir -p ~/.zfunc
alors --completion-script zsh > ~/.zfunc/_alors
```

and make sure `~/.zshrc` has, before `compinit` runs:

```zsh
fpath+=(~/.zfunc)
autoload -Uz compinit && compinit
```

Then open a new shell (or `exec zsh`).

The script itself holds no knowledge of your tasks: at every `<TAB>` it hands
the words you've typed to `alors --complete` and offers back what that prints.
Where `alors` has nothing to suggest — past a task name, where the remaining
words are that task's arguments — completion falls back to file names.

## Features

- **Tasks in a `tasks.alors` file.** Define a command once; run it as
  `alors <task>`.
- **Parameters.** A task can take positional arguments: `alors test MySuite`.
- **Constants.** `version := 1.4.2` shares a literal value across tasks.
- **Sequencing.** A task can depend on other tasks, which run first, in order.
- **Subcommands.** Tasks can be grouped into namespaces: `alors docker build`.
- **Shell completion.** `<TAB>` completes your project's own task names —
  see [Shell completion](#shell-completion).
- **Validated up front.** The whole file is checked before anything runs —
  unknown dependencies and dependency cycles are reported, not discovered
  mid-run.
- **One file, current directory.** `alors` reads the `tasks.alors` in the
  directory you run it from. No searching up the tree — or point it somewhere
  explicitly with [`--taskfile`](#running-a-taskfile-elsewhere).

## Defining tasks

A `tasks.alors` file is a list of tasks. Each task is a name ending in `:`,
followed by an indented body — the shell commands to run.

```
# configure the CMake build tree
configure:
    cmake -S . -B build

# compile what's already configured
compile:
    cmake --build build

# wipe the build tree
clean:
    rm -rf build
```

Run a task by name:

```
alors configure      # runs: cmake -S . -B build
alors clean          # runs: rm -rf build
alors                # with no task, lists everything available
alors --help         # usage + the task list
alors --help-ai      # usage notes written for AI agents
alors --version      # print the version
```

> Help lives behind `--help` only. The bare word `help` is an ordinary task
> name — if your file defines a `help` task, `alors help` runs it, same as
> the `import:` rule below.

A task can take a parameter, used in the body as `{{name}}` (or `$name`):

```
test name:
    ctest --test-dir build -R {{name}}
```

```
alors test MySuite   # runs: ctest --test-dir build -R MySuite
```

### How a body runs

A task's body runs as a **single shell** (`sh`), so `cd` and variables persist
from one line to the next:

```
build:
    cd frontend
    npm install         # runs inside ./frontend
```

The body is also **fail-fast**: it runs under `set -e`, so the first command
that fails stops the task — the same "stop on first failure" rule that applies
to dependencies. To let a body keep going after a failure, start it with
`set +e`, or ignore a single command with `cmd || true`:

```
check:
    set +e              # this task tolerates failures
    diff a b
    echo "compared"
```

## Constants

A top-level `name := value` line defines a **constant** — a value shared by the
tasks of that file, used as `{{name}}` just like a parameter:

```
version := 1.4.2
image := registry.example.com/app

build:
    docker build -t {{image}}:{{version}} .

push: build
    docker push {{image}}:{{version}}
```

Constants are deliberately *not* variables:

- **Immutable.** Defining the same name twice is an error. The value is a
  literal — it may not reference params or other constants. Any logic belongs
  in shell code inside a task body.
- **Params win.** If a task declares a param with the same name, `{{name}}` in
  that task means the param. So a constant can serve as a default that a task's
  own arguments override.
- **File-local.** A constant applies to the tasks written in the same file.
  Imports neither see nor leak constants, so two files can each have their own
  `version` without clashing.

A constant also works as a **dependency argument**, which is where it shines —
one line controls what every sequence passes along:

```
arch := arm64

build target:
    cmake --preset {{target}}

release: build {{arch}}
```

## Sequencing tasks

A task can list other tasks to run first, **comma-separated**, after its `:` —
so you can compose small tasks into a larger one:

```
configure:
    cmake -S . -B build

compile:
    cmake --build build

# build has no body of its own; it just runs configure, then compile
build: configure, compile
```

```
alors build          # runs configure, then compile
```

### Passing arguments to a sequenced task

Because dependencies are comma-separated, one can carry its own positional
arguments — written as whitespace after the task name. So a sequence can invoke
a parameterized task with a value fixed right in the file:

```
test name:
    ctest --test-dir build -R {{name}}

# `alors ci` runs `test smoke`, then `lint`
ci: test smoke, lint
```

An argument may also be a `{{param}}` of the **declaring** task, forwarding its
own value down the chain (use `"quotes"` for a value with spaces):

```
# `alors check integration` runs `test integration`, then echoes
check suite: test {{suite}}
    echo "checked {{suite}}"
```

Dependencies run **deps-first**, and the task's own body runs last. The same task
with the **same** arguments runs **at most once** per invocation; the same task
with **different** arguments runs once for each — so `all: build x86, build arm`
builds both. The first failure stops the sequence. Everything is checked before
anything runs, so a wrong argument count or a cycle is a clear up-front error, not
a mid-run surprise:

```
alors: dependency cycle: build -> compile -> build
```

## Subcommands

Tasks can be grouped into namespaces with `::`. A task named `docker::build`
becomes the subcommand `alors docker build`:

```
docker::build:
    docker build -t app .

docker::push:
    docker push app
```

```
alors docker build   # runs docker::build
alors docker push    # runs docker::push
alors docker         # lists the docker subcommands
```

A namespace can also have a **default** task — the plain task with the same name.
Given both `build:` and `build::release:`, `alors build` runs the default and
`alors build release` runs the sub-task.

### One task per invocation

This is the main place `alors` differs from `just`. In `just` you can run
several tasks at once — `just configure compile test`. In `alors` an invocation
always selects **exactly one** task; the tokens after it are arguments or a
subcommand path, never additional tasks. Running things in sequence is expressed
*in the file* as dependencies (`build: configure, compile`), not assembled on
the command line.

A consequence: a token that matches a subcommand is always treated as part of
the path, so it can never be mistaken for an argument.

## Imports

A `tasks.alors` file can pull tasks in from other `.alors` files with `import`,
so shared tasks live in one place and projects compose them. Every file uses the
same `.alors` extension — `tasks.alors` is simply the well-known entry point.
The path is **quoted** and resolved **relative to the importing file's
directory**:

```
import "ci/common.alors"
import "docker.alors" as docker

build: lint, docker::build
    echo "everything built"
```

There are two shapes:

- **Flat** — `import "ci/common.alors"` merges the imported tasks under their
  own names. A `lint:` over there becomes `alors lint` here.
- **Namespaced** — `import "docker.alors" as docker` nests the *whole* file
  under a namespace. Its `build:` becomes `alors docker build` (and
  `docker::build` when referenced as a dependency). The imported file's own
  internal dependencies are rewritten to match, so it doesn't need to know it
  was namespaced — the importer decides the layout.

Once merged, imported tasks are ordinary tasks: you can depend on them
(`build: lint, docker::build`) and invoke them just like local ones.

An `as` namespace can also be given an **action** — a default task — by defining
the bare name in the importing file. It runs on `alors docker`, while the
subcommands still work:

```
import "docker.alors" as docker

# `alors docker` builds then pushes; `alors docker build` / `alors docker push` still work
docker: docker::build, docker::push
```

A few rules, in keeping with alors's "checked up front" stance:

- **Every name is unique.** If two files define the same final task name it's a
  hard error, reported before anything runs. Namespacing with `as` is how you
  disambiguate.
- **Imports are transitive.** An imported file may import in turn; an import
  cycle is reported, not followed.
- **The local file doesn't win.** There's no override — a clash is always an
  error, whether between the root and an import or between two imports.
- **An `as` namespace is sealed.** Exactly one import fills it. The importing
  file may give it a default task (the bare `docker:` above) but may not add new
  sub-tasks to it or override its members, and two imports can't share one
  namespace. The contents of a namespace come from one place.

> A line is read as an import only when it's the word `import` followed by a
> quoted path (`import "x"`). That keeps a task you legitimately *named* `import`
> (written `import:`) from being mistaken for a directive.

## Running a taskfile elsewhere

`alors --taskfile <path> <task>` runs a task from a taskfile that isn't in the
current directory:

```
alors --taskfile ../other-project/tasks.alors build
```

The rule is one sentence, with no exceptions:

> `alors --taskfile X <task>` does exactly what `cd $(dirname X) && alors <task>`
> would do.

**The task runs in its own project's directory**, not yours. That isn't a side
effect, it's the point: a body is a shell script full of paths that mean
something relative to its project — `cargo build`, `cmake -S . -B build`,
`cp target/release/app`. Running another project's commands here would apply
them to *this* directory, quietly and wrongly. (It's also what imports already
do: an `import` path resolves relative to the importing file, never to your
current directory.)

Two consequences worth knowing:

- **Relative paths in arguments** are read by the body, so they're relative to
  the taskfile's directory, not yours. Pass absolute paths across projects.
- **The file needn't be called `tasks.alors`.** `--taskfile` takes the path you
  give it, so a directory can hold many of them —
  `alors --taskfile tests/fixtures/imports.alors build` — which is how this
  project's own end-to-end tests are written.

The flag is recognized **before the task name only**. After it, everything is
the task's arguments, `--taskfile` included — so `alors greet --taskfile` passes
the literal string `--taskfile` to `greet`. That keeps the one rule that makes
invocations readable: once a task is selected, the rest are its arguments.

`alors` still never *searches* for a taskfile. It reads the one in the current
directory, or exactly the file you named — never something found by looking
around.

## Releasing

Releases are built by GitHub Actions
([`.github/workflows/release.yml`](.github/workflows/release.yml)). Each target
builds **natively on its own runner** — Linux x86_64 on Ubuntu, macOS Apple
Silicon on a macOS runner — so there's no cross-compiling, Docker, or extra
toolchain. To cut a release, bump the version in `Cargo.toml`, commit, then push
a matching tag:

```
git tag v0.1.0
git push origin v0.1.0
```

CI builds both binaries and attaches them to the GitHub Release for the tag,
which is what the [Install](#install) links point at.

---

_Status: v0 prototype._

## The name

`alors` is French — the little word that announces the next move: "so", "well
then", "right then". That's exactly the register of a project CLI. Each
invocation reads like turning to the project and getting on with it:
`alors build` is "right then — build", `alors test` "well, test it",
`alors deploy` "so, deploy". You don't need the French to use it (plenty of
beloved tools carry French names — Vite, Vue), but once you hear it, every
command has a small narrative beat in front of it.

## Motivation

I reach for a `justfile` in most of my projects, but I kept wanting real
**subcommands** — grouping related tasks under a namespace like
`alors docker build` instead of flat names. That itch is what `alors` scratches.

I mostly write Python and C++, but a small, fast, single-binary CLI is a better
fit for **Rust** — no runtime to ship and nothing to install alongside it. I
don't know Rust's exact syntax, though, so I built this by vibe-coding with
[Claude](https://www.anthropic.com/claude): I described the behavior and the
design choices I wanted, and Claude wrote and explained the Rust. Credit where
it's due — `alors` was written with Claude.
