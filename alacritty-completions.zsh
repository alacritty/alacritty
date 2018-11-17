#compdef alacritty

_alacritty() {
    local context curcontext="$curcontext" state line
    typeset -A opt_args

    _arguments \
        "(-h --help)"{-h,--help}"[Prints help information]" \
        "(-V --version)"{-V,--version}"[Prints version information]" \
        "(--no-live-config-reload)--live-config-reload[Enable automatic config reloading]" \
        "(--live-config-reload)--no-live-config-reload[Disable automatic config reloading]" \
        "(--persistent-logging)--persistent-logging[Keep the log file after quitting Alacritty]" \
        "--print-events[Print all events to stdout]" \
        {-q,-qq}"[Reduces the level of verbosity (min is -qq)]" \
        {-v,-vv,-vvv}"[Increases the level of verbosity (max is -vvv)]" \
        "--ref-test[Generates ref test]" \
        "--config-file[Specify an alternative config file]:file:_files" \
        "(-d --dimensions)"{-d,--dimensions}"[Window dimensions]:dimensions:_guard '<->' width: :_guard '<->' length" \
        "--title[Defines the window title]:title:" \
        "--working-directory[Start shell in specified directory]:directory:_dir_list" \
        "(-e --command)"{-e,--command}"[Execute command (must be last arg)]:program: _command_names -e:*::program arguments: _normal"
}

_alacritty "$@"
