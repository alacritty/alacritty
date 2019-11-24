# Meta
complete -c alacritty \
  -s "v" \
  -l "version" \
  -d "Prints version information"
complete -c alacritty \
  -s "h" \
  -l "help" \
  -d "Prints help information"

# Config
complete -c alacritty \
  -l "live-config-reload" \
  -d "Enable automatic config reloading"
complete -c alacritty \
  -l "no-live-config-reload" \
  -d "Disable automatic config reloading"
complete -c alacritty \
  -l "persistent-logging" \
  -d "Keep the log file after quitting Alacritty"
complete -c alacritty \
  -f \
  -l "config-file" \
  -d "Specify an alternative config file"
complete -c alacritty \
  -s "t" \
  -l "title" \
  -d "Defines the window title"
complete -c alacritty \
  -l "class" \
  -d "Defines the window class"
complete -c alacritty \
  -l "embed" \
  -d "Defines the X11 window ID (as a decimal integer) to embed Alacritty within"
complete -c alacritty \
  -x \
  -a '(__fish_complete_directories (commandline -ct))' \
  -l "working-directory" \
  -d "Start shell in specified directory"
complete -c alacritty \
  -l "hold" \
  -d "Remain open after child process exits"

# Output
complete \
  -c alacritty \
  -l "print-events" \
  -d "Print all events to stdout"
complete \
  -c alacritty \
  -s "q" \
  -d "Reduces the level of verbosity (min is -qq)"
complete \
  -c alacritty \
  -s "qq" \
  -d "Reduces the level of verbosity"
complete \
  -c alacritty \
  -s "v" \
  -d "Increases the level of verbosity"
complete \
  -c alacritty \
  -s "vv" \
  -d "Increases the level of verbosity"
complete \
  -c alacritty \
  -s "vvv" \
  -d "Increases the level of verbosity"

complete \
  -c alacritty \
  -l "ref-test" \
  -d "Generates ref test"

complete \
  -c alacritty \
  -s "d" \
  -l "dimensions" \
  -d "Window dimensions <columns> <lines>"

complete \
  -c alacritty \
  -l "position" \
  -d "Window position <x-pos> <y-pos>"

complete \
  -c alacritty \
  -s "e" \
  -l "command" \
  -d "Execute command (must be last arg)"
