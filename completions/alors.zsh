#compdef alors
# zsh completion for alors — https://github.com/Wenke-D/alors
#
# Install it once, into a directory on your $fpath:
#     alors --completion-script zsh > ~/.zfunc/_alors
# with `fpath+=(~/.zfunc)` before `compinit` in ~/.zshrc.

_alors() {
    # Hand alors every word typed so far, the one under the cursor last (it may
    # be empty), and let the binary itself decide what fits there.
    local -a candidates
    candidates=( ${(f)"$(command alors --complete \
        "${(@)words[2,CURRENT-1]}" "${words[CURRENT]}" 2>/dev/null)"} )
    candidates=( ${candidates:#} )

    if (( ${#candidates} )); then
        compadd -a candidates
    else
        # Nothing to offer: the word is a task argument, usually a file name.
        _files
    fi
}

_alors "$@"
