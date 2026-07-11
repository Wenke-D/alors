# via — a CLI for your project

Every project accumulates commands: configure, compile, test, package, clean.
`via` gathers them into a single command-line tool for your project. You write
the commands once as named **tasks** in a `viafile`, and run them with
`via <task>`.

It follows in the footsteps of [`make`](https://www.gnu.org/software/make/)
(originally written by Stuart Feldman at Bell Labs; GNU Make today) and
[`just`](https://github.com/casey/just) (by Casey Rodarmor) — the same idea of a
project-local file full of named tasks — and owes both a great deal. `via`
keeps that core idea and makes a few different choices around how tasks are named
and invoked (see [Subcommands](#subcommands)).

## Install

Download the binary for your platform from the latest release, make it
executable, and drop it on your `PATH`:

**Linux (x86_64)**

```
curl -L -o via https://github.com/Wenke-D/via/releases/latest/download/via-linux-x86_64
chmod +x via && sudo mv via /usr/local/bin/
```

**macOS (Apple Silicon)**

```
curl -L -o via https://github.com/Wenke-D/via/releases/latest/download/via-macos-arm64
chmod +x via && sudo mv via /usr/local/bin/
```

The Linux binary is statically linked (musl), so it runs on any distribution
with nothing else to install. On macOS, Gatekeeper blocks unsigned downloads —
clear the quarantine flag with `xattr -d com.apple.quarantine via` (or right-click
→ Open the first time).

Prefer to build it yourself? `cargo build --release` (or `via install`, which
builds and installs to `/usr/local/bin`).

## Features

- **Tasks in a `viafile`.** Define a command once; run it as `via <task>`.
- **Parameters.** A task can take positional arguments: `via test MySuite`.
- **Sequencing.** A task can depend on other tasks, which run first, in order.
- **Subcommands.** Tasks can be grouped into namespaces: `via docker build`.
- **Validated up front.** The whole file is checked before anything runs —
  unknown dependencies and dependency cycles are reported, not discovered
  mid-run.
- **One file, current directory.** `via` reads the `viafile` in the directory you
  run it from. No searching up the tree.

## Defining a viafile

A `viafile` is a list of tasks. Each task is a name ending in `:`, followed by an
indented body — the shell commands to run.

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
via configure        # runs: cmake -S . -B build
via clean            # runs: rm -rf build
via                  # with no task, lists everything available
```

A task can take a parameter, used in the body as `{{name}}` (or `$name`):

```
test name:
    ctest --test-dir build -R {{name}}
```

```
via test MySuite     # runs: ctest --test-dir build -R MySuite
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
via build            # runs configure, then compile
```

### Passing arguments to a sequenced task

Because dependencies are comma-separated, one can carry its own positional
arguments — written as whitespace after the task name. So a sequence can invoke
a parameterized task with a value fixed right in the file:

```
test name:
    ctest --test-dir build -R {{name}}

# `via ci` runs `test smoke`, then `lint`
ci: test smoke, lint
```

An argument may also be a `{{param}}` of the **declaring** task, forwarding its
own value down the chain (use `"quotes"` for a value with spaces):

```
# `via check integration` runs `test integration`, then echoes
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
via: dependency cycle: build -> compile -> build
```

## Subcommands

Tasks can be grouped into namespaces with `::`. A task named `docker::build`
becomes the subcommand `via docker build`:

```
docker::build:
    docker build -t app .

docker::push:
    docker push app
```

```
via docker build     # runs docker::build
via docker push      # runs docker::push
via docker           # lists the docker subcommands
```

A namespace can also have a **default** task — the plain task with the same name.
Given both `build:` and `build::release:`, `via build` runs the default and
`via build release` runs the sub-task.

### One task per invocation

This is the main place `via` differs from `just`. In `just` you can run several
tasks at once — `just configure compile test`. In `via` an invocation always
selects **exactly one** task; the tokens after it are arguments or a subcommand
path, never additional tasks. Running things in sequence is expressed *in the
viafile* as dependencies (`build: configure, compile`), not assembled on the
command line.

A consequence: a token that matches a subcommand is always treated as part of
the path, so it can never be mistaken for an argument.

## Imports

A viafile can pull tasks in from other files with `import`, so shared tasks
live in one place and projects compose them. The path is **quoted** and resolved
**relative to the importing file's directory**:

```
import "ci/common.viafile"
import "docker.viafile" as docker

build: lint, docker::build
    echo "everything built"
```

There are two shapes:

- **Flat** — `import "ci/common.viafile"` merges the imported tasks under their
  own names. A `lint:` over there becomes `via lint` here.
- **Namespaced** — `import "docker.viafile" as docker` nests the *whole* file
  under a namespace. Its `build:` becomes `via docker build` (and `docker::build`
  when referenced as a dependency). The imported file's own internal
  dependencies are rewritten to match, so it doesn't need to know it was
  namespaced — the importer decides the layout.

Once merged, imported tasks are ordinary tasks: you can depend on them
(`build: lint, docker::build`) and invoke them just like local ones.

An `as` namespace can also be given an **action** — a default task — by defining
the bare name in the importing file. It runs on `via docker`, while the
subcommands still work:

```
import "docker.viafile" as docker

# `via docker` builds then pushes; `via docker build` / `via docker push` still work
docker: docker::build, docker::push
```

A few rules, in keeping with via's "checked up front" stance:

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

`via` is short — a project's CLI is something you type all day, so the name
should stay out of the way. It's also a real word: Latin for "way" or "road",
and in English "by way of". Each invocation then reads like routing a command
through your project — you get there *via* the project's own tasks, so
`via build` is "build, by way of this project" and `via test` its tests.

## Motivation

I reach for a `justfile` in most of my projects, but I kept wanting real
**subcommands** — grouping related tasks under a namespace like
`via docker build` instead of flat names. That itch is what `via` scratches.

I mostly write Python and C++, but a small, fast, single-binary CLI is a better
fit for **Rust** — no runtime to ship and nothing to install alongside it. I
don't know Rust's exact syntax, though, so I built this by vibe-coding with
[Claude](https://www.anthropic.com/claude): I described the behavior and the
design choices I wanted, and Claude wrote and explained the Rust. Credit where
it's due — `via` was written with Claude.
