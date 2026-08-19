# alors syntax highlighting

Syntax highlighting for **`.alors`** files — the tasks files used by
[**alors**](https://github.com/Wenke-D/alors), a small command-line tool that
gathers your project's commands (configure, build, test, deploy…) into one CLI
you run as `alors <task>`.

If you keep a `tasks.alors` in your project, this extension colorizes it (and
every imported `.alors` file) so it's easy to read and edit.

## What it highlights

- Comments (`# …`)
- Task names and `::` namespace separators
- Parameters
- The `:` separator and the dependency references that follow it
- `{{name}}` interpolation and `$name` / `${name}` variables inside task bodies

## When it activates

Automatically, for any file with the `.alors` extension — the `tasks.alors`
entry point and the files it imports alike.

## Example

```alors
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

Run it with `alors build`, `alors test MySuite`, and so on.

## About alors

`alors` is a project-local task runner in the spirit of
[`make`](https://www.gnu.org/software/make/) and
[`just`](https://github.com/casey/just), with first-class **subcommands**
(`alors docker build`). It's a single, dependency-free binary.

**Project & docs:** https://github.com/Wenke-D/alors

## License

[MIT](./LICENSE)
