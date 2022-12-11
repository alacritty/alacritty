complete -c alacritty -n "__fish_use_subcommand" -l embed -d 'X11 window ID to embed Alacritty within (decimal or hexadecimal with "0x" prefix)' -r
complete -c alacritty -n "__fish_use_subcommand" -l config-file -d 'Specify alternative configuration file [default: $XDG_CONFIG_HOME/alacritty/alacritty.yml]' -r -F
complete -c alacritty -n "__fish_use_subcommand" -l socket -d 'Path for IPC socket creation' -r -F
complete -c alacritty -n "__fish_use_subcommand" -s o -l option -d 'Override configuration file options [example: cursor.style=Beam]' -r
complete -c alacritty -n "__fish_use_subcommand" -l working-directory -d 'Start the shell in the specified working directory' -r -F
complete -c alacritty -n "__fish_use_subcommand" -s e -l command -d 'Command and args to execute (must be last argument)' -r
complete -c alacritty -n "__fish_use_subcommand" -s T -l title -d 'Defines the window title [default: Alacritty]' -r
complete -c alacritty -n "__fish_use_subcommand" -l class -d 'Defines window class/app_id on X11/Wayland [default: Alacritty]' -r
complete -c alacritty -n "__fish_use_subcommand" -s h -l help -d 'Print help information'
complete -c alacritty -n "__fish_use_subcommand" -s V -l version -d 'Print version information'
complete -c alacritty -n "__fish_use_subcommand" -l print-events -d 'Print all events to stdout'
complete -c alacritty -n "__fish_use_subcommand" -l ref-test -d 'Generates ref test'
complete -c alacritty -n "__fish_use_subcommand" -s q -d 'Reduces the level of verbosity (the min level is -qq)'
complete -c alacritty -n "__fish_use_subcommand" -s v -d 'Increases the level of verbosity (the max level is -vvv)'
complete -c alacritty -n "__fish_use_subcommand" -l hold -d 'Remain open after child process exit'
complete -c alacritty -n "__fish_use_subcommand" -f -a "msg" -d 'Send a message to the Alacritty socket'
complete -c alacritty -n "__fish_use_subcommand" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c alacritty -n "__fish_seen_subcommand_from msg; and not __fish_seen_subcommand_from create-window; and not __fish_seen_subcommand_from config; and not __fish_seen_subcommand_from help" -s s -l socket -d 'IPC socket connection path override' -r -F
complete -c alacritty -n "__fish_seen_subcommand_from msg; and not __fish_seen_subcommand_from create-window; and not __fish_seen_subcommand_from config; and not __fish_seen_subcommand_from help" -s h -l help -d 'Print help information'
complete -c alacritty -n "__fish_seen_subcommand_from msg; and not __fish_seen_subcommand_from create-window; and not __fish_seen_subcommand_from config; and not __fish_seen_subcommand_from help" -f -a "create-window" -d 'Create a new window in the same Alacritty process'
complete -c alacritty -n "__fish_seen_subcommand_from msg; and not __fish_seen_subcommand_from create-window; and not __fish_seen_subcommand_from config; and not __fish_seen_subcommand_from help" -f -a "config" -d 'Update the Alacritty configuration'
complete -c alacritty -n "__fish_seen_subcommand_from msg; and not __fish_seen_subcommand_from create-window; and not __fish_seen_subcommand_from config; and not __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from create-window" -l working-directory -d 'Start the shell in the specified working directory' -r -F
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from create-window" -s e -l command -d 'Command and args to execute (must be last argument)' -r
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from create-window" -s T -l title -d 'Defines the window title [default: Alacritty]' -r
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from create-window" -l class -d 'Defines window class/app_id on X11/Wayland [default: Alacritty]' -r
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from create-window" -l hold -d 'Remain open after child process exit'
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from create-window" -s h -l help -d 'Print help information'
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from config" -s w -l window-id -d 'Window ID for the new config' -r
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from config" -s r -l reset -d 'Clear all runtime configuration changes'
complete -c alacritty -n "__fish_seen_subcommand_from msg; and __fish_seen_subcommand_from config" -s h -l help -d 'Print help information'
