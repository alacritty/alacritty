#/usr/bin/env bash

# Load completion function
complete -F _alacritty alacritty

# Completion function
_alacritty()
{
    local cur prev prevprev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    prevprev="${COMP_WORDS[COMP_CWORD-2]}"
    opts="-h --help -V --version --print-events -q -qq -v -vv -vvv --ref-test --hold -e --command --config-file -o --option -t --title --embed --class --working-directory"

    # If `--command` or `-e` is used, stop completing
    for i in "${!COMP_WORDS[@]}"; do
        if [[ "${COMP_WORDS[i]}" == "--command" ]] \
            || [[ "${COMP_WORDS[i]}" == "-e" ]] \
            && [[ "${#COMP_WORDS[@]}" -gt "$(($i + 2))" ]]
        then
            return 0
        fi
    done

    # Match the previous word
    case "${prev}" in
        --command | -e)
            # Complete all commands in $PATH
            COMPREPLY=( $(compgen -c -- "${cur}") )
            return 0;;
        --config-file)
            # Path based completion
            local IFS=$'\n'
            compopt -o filenames
            COMPREPLY=( $(compgen -f -- "${cur}") )
            return 0;;
        --class | --title | -t)
            # Don't complete here
            return 0;;
        --working-directory)
            # Directory completion
            local IFS=$'\n'
            compopt -o filenames
            COMPREPLY=( $(compgen -d -- "${cur}") )
            return 0;;
    esac

    # Show all flags if there was no previous word
    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
    return 0
}
