# viafile syntax (VS Code)

Syntax highlighting for `viafile`, the tasks file for via — a CLI for your project.

Highlights:

- comments (`# ...` at column 0)
- task names and `::` namespace separators
- parameters and the optional `?` marker
- the `:` separator and dependency references after it
- `{{name}}` interpolation and `$name` / `${name}` in task bodies

Applies to files named `viafile` (and anything matching `*viafile`, e.g.
`example-viafile`) or with a `.viafile` extension.

## Try it without installing

Open this folder in VS Code and press **F5** ("Run Extension"). In the
Extension Development Host that opens, open a `viafile` — it will be highlighted.

## Install locally

Symlink (or copy) this folder into your VS Code extensions directory, then
reload the window:

```
ln -s "$PWD" ~/.vscode/extensions/viafile-0.0.1
```

## Package as a .vsix

```
npm install -g @vscode/vsce
vsce package
```

This is grammar-only — no runtime code, so there is nothing to compile.
