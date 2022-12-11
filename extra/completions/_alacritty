#compdef alacritty

autoload -U is-at-least

_alacritty() {
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext="$curcontext" state line
    _arguments "${_arguments_options[@]}" \
'--embed=[X11 window ID to embed Alacritty within (decimal or hexadecimal with "0x" prefix)]:EMBED: ' \
'--config-file=[Specify alternative configuration file \[default: $XDG_CONFIG_HOME/alacritty/alacritty.yml\]]:CONFIG_FILE:_files' \
'--socket=[Path for IPC socket creation]:SOCKET:_files' \
'*-o+[Override configuration file options \[example: cursor.style=Beam\]]:OPTION: ' \
'*--option=[Override configuration file options \[example: cursor.style=Beam\]]:OPTION: ' \
'--working-directory=[Start the shell in the specified working directory]:WORKING_DIRECTORY:_files' \
'*-e+[Command and args to execute (must be last argument)]:COMMAND: ' \
'*--command=[Command and args to execute (must be last argument)]:COMMAND: ' \
'-T+[Defines the window title \[default: Alacritty\]]:TITLE: ' \
'--title=[Defines the window title \[default: Alacritty\]]:TITLE: ' \
'--class=[Defines window class/app_id on X11/Wayland \[default: Alacritty\]]:general> | <general>,<instance: ' \
'-h[Print help information]' \
'--help[Print help information]' \
'-V[Print version information]' \
'--version[Print version information]' \
'--print-events[Print all events to stdout]' \
'--ref-test[Generates ref test]' \
'(-v)*-q[Reduces the level of verbosity (the min level is -qq)]' \
'(-q)*-v[Increases the level of verbosity (the max level is -vvv)]' \
'--hold[Remain open after child process exit]' \
":: :_alacritty_commands" \
"*::: :->alacritty" \
&& ret=0
    case $state in
    (alacritty)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:alacritty-command-$line[1]:"
        case $line[1] in
            (msg)
_arguments "${_arguments_options[@]}" \
'-s+[IPC socket connection path override]:SOCKET:_files' \
'--socket=[IPC socket connection path override]:SOCKET:_files' \
'-h[Print help information]' \
'--help[Print help information]' \
":: :_alacritty__msg_commands" \
"*::: :->msg" \
&& ret=0

    case $state in
    (msg)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:alacritty-msg-command-$line[1]:"
        case $line[1] in
            (create-window)
_arguments "${_arguments_options[@]}" \
'--working-directory=[Start the shell in the specified working directory]:WORKING_DIRECTORY:_files' \
'*-e+[Command and args to execute (must be last argument)]:COMMAND: ' \
'*--command=[Command and args to execute (must be last argument)]:COMMAND: ' \
'-T+[Defines the window title \[default: Alacritty\]]:TITLE: ' \
'--title=[Defines the window title \[default: Alacritty\]]:TITLE: ' \
'--class=[Defines window class/app_id on X11/Wayland \[default: Alacritty\]]:general> | <general>,<instance: ' \
'--hold[Remain open after child process exit]' \
'-h[Print help information]' \
'--help[Print help information]' \
&& ret=0
;;
(config)
_arguments "${_arguments_options[@]}" \
'-w+[Window ID for the new config]:WINDOW_ID: ' \
'--window-id=[Window ID for the new config]:WINDOW_ID: ' \
'()-r[Clear all runtime configuration changes]' \
'()--reset[Clear all runtime configuration changes]' \
'-h[Print help information]' \
'--help[Print help information]' \
'*::options -- Configuration file options \[example\: cursor.style=Beam\]:' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" \
'*::subcommand -- The subcommand whose help message to display:' \
&& ret=0
;;
        esac
    ;;
esac
;;
(help)
_arguments "${_arguments_options[@]}" \
'*::subcommand -- The subcommand whose help message to display:' \
&& ret=0
;;
        esac
    ;;
esac
}

(( $+functions[_alacritty_commands] )) ||
_alacritty_commands() {
    local commands; commands=(
'msg:Send a message to the Alacritty socket' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'alacritty commands' commands "$@"
}
(( $+functions[_alacritty__msg__config_commands] )) ||
_alacritty__msg__config_commands() {
    local commands; commands=()
    _describe -t commands 'alacritty msg config commands' commands "$@"
}
(( $+functions[_alacritty__msg__create-window_commands] )) ||
_alacritty__msg__create-window_commands() {
    local commands; commands=()
    _describe -t commands 'alacritty msg create-window commands' commands "$@"
}
(( $+functions[_alacritty__help_commands] )) ||
_alacritty__help_commands() {
    local commands; commands=()
    _describe -t commands 'alacritty help commands' commands "$@"
}
(( $+functions[_alacritty__msg__help_commands] )) ||
_alacritty__msg__help_commands() {
    local commands; commands=()
    _describe -t commands 'alacritty msg help commands' commands "$@"
}
(( $+functions[_alacritty__msg_commands] )) ||
_alacritty__msg_commands() {
    local commands; commands=(
'create-window:Create a new window in the same Alacritty process' \
'config:Update the Alacritty configuration' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'alacritty msg commands' commands "$@"
}

_alacritty "$@"
