# viafile syntax highlighting

Syntax highlighting for **`viafile`** — the tasks file used by
[**via**](https://github.com/Wenke-D/via), a small command-line tool that gathers
your project's commands (configure, build, test, deploy…) into one CLI you run as
`via <task>`.

If you keep a `viafile` in your project, this extension colorizes it so it's easy
to read and edit.

## What it highlights

- Comments (`# …`)
- Task names and `::` namespace separators
- Parameters
- The `:` separator and the dependency references that follow it
- `{{name}}` interpolation and `$name` / `${name}` variables inside task bodies

## When it activates

Automatically, for any file:

- named `viafile` (or matching `*viafile`, e.g. `example-viafile`), or
- with a `.viafile` extension.

## Example

```viafile
# configure the CMake build tree
configure:
    cmake -S . -B build

# build depends on configure — it runs first
build: configure
    cmake --build build

# a task with a parameter, used as {{name}}
test name:
    ctest --test-dir build -R {{name}}
```

Run it with `via build`, `via test MySuite`, and so on.

## About via

`via` is a project-local task runner in the spirit of
[`make`](https://www.gnu.org/software/make/) and
[`just`](https://github.com/casey/just), with first-class **subcommands**
(`via docker build`). It's a single, dependency-free binary.

**Project & docs:** https://github.com/Wenke-D/via

## License

[MIT](./LICENSE)
