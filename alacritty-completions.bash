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
    opts="-h --help -V --version --live-config-reload --no-live-config-reload --print-events -q -qq -v -vv -vvv --ref-test -e --command --config-file -d --dimensions -t --title --working-directory"

    # If `--command` or `-e` is used, stop completing
    for i in "${!COMP_WORDS[@]}"; do
        echo "${COMP_WORDS[i]}" >> ./testfile
        if [[ "${COMP_WORDS[i]}" == "--command" ]] \
            || [[ "${COMP_WORDS[i]}" == "-e" ]] \
            && [[ "${#COMP_WORDS[@]}" -gt "$(($i + 2))" ]]
        then
            return 0
        fi
    done

    # Make sure the Y dimension isn't completed
    if [[ "${prevprev}" == "--dimensions" ]] || [[ "${prevprev}" == "-d" ]]; then
        return 0
    fi

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
        --dimensions | -d | --title | -t)
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
