# Available subcommands
set -l commands msg help

complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -a "msg help"

# Meta
complete -c alacritty \
  -n "not __fish_seen_subcommand_from help" \
  -s "v" \
  -l "version" \
  -d "Prints version information"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from help" \
  -s "h" \
  -l "help" \
  -d "Prints help information"

# Config
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -f \
  -l "config-file" \
  -d "Specify an alternative config file"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -s "t" \
  -l "title" \
  -d "Defines the window title"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -l "class" \
  -d "Defines the window class"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -l "embed" \
  -d "Defines the X11 window ID (as a decimal integer) to embed Alacritty within"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -x \
  -a '(__fish_complete_directories (commandline -ct))' \
  -l "working-directory" \
  -d "Start shell in specified directory"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -l "hold" \
  -d "Remain open after child process exits"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -s "o" \
  -l "option" \
  -d "Override config file options"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -l "socket" \
  -d "Path for IPC socket creation"

# Output
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -l "print-events" \
  -d "Print all events to stdout"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -s "q" \
  -d "Reduces the level of verbosity (min is -qq)"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -s "qq" \
  -d "Reduces the level of verbosity"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -s "v" \
  -d "Increases the level of verbosity"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -s "vv" \
  -d "Increases the level of verbosity"
complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -s "vvv" \
  -d "Increases the level of verbosity"

complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -l "ref-test" \
  -d "Generates ref test"

complete -c alacritty \
  -n "not __fish_seen_subcommand_from $commands" \
  -s "e" \
  -l "command" \
  -d "Execute command (must be last arg)"

# Subcommand `msg`
complete -c alacritty \
  -n "__fish_seen_subcommand_from msg" \
  -s "s" \
  -l "socket" \
  -d "Socket path override"
complete -c alacritty \
  -n "__fish_seen_subcommand_from msg" \
  -a "create-window help"
