# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_alacritty_global_optspecs
	string join \n print-events ref-test embed= config-file= socket= q v daemon working-directory= hold e/command= T/title= class= o/option= h/help V/version
end

function __fish_alacritty_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_alacritty_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_alacritty_using_subcommand
	set -l cmd (__fish_alacritty_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c alacritty -n "__fish_alacritty_needs_command" -l embed -d 'X11 window ID to embed Alacritty within (decimal or hexadecimal with "0x" prefix)' -r
complete -c alacritty -n "__fish_alacritty_needs_command" -l config-file -d 'Specify alternative configuration file [default: $XDG_CONFIG_HOME/alacritty/alacritty.toml]' -r -F
complete -c alacritty -n "__fish_alacritty_needs_command" -l socket -d 'Path for IPC socket creation' -r -F
complete -c alacritty -n "__fish_alacritty_needs_command" -l working-directory -d 'Start the shell in the specified working directory' -r -F
complete -c alacritty -n "__fish_alacritty_needs_command" -s e -l command -d 'Command and args to execute (must be last argument)' -r
complete -c alacritty -n "__fish_alacritty_needs_command" -s T -l title -d 'Defines the window title [default: Alacritty]' -r
complete -c alacritty -n "__fish_alacritty_needs_command" -l class -d 'Defines window class/app_id on X11/Wayland [default: Alacritty]' -r
complete -c alacritty -n "__fish_alacritty_needs_command" -s o -l option -d 'Override configuration file options [example: \'cursor.style="Beam"\']' -r
complete -c alacritty -n "__fish_alacritty_needs_command" -l print-events -d 'Print all events to STDOUT'
complete -c alacritty -n "__fish_alacritty_needs_command" -l ref-test -d 'Generates ref test'
complete -c alacritty -n "__fish_alacritty_needs_command" -s q -d 'Reduces the level of verbosity (the min level is -qq)'
complete -c alacritty -n "__fish_alacritty_needs_command" -s v -d 'Increases the level of verbosity (the max level is -vvv)'
complete -c alacritty -n "__fish_alacritty_needs_command" -l daemon -d 'Do not spawn an initial window'
complete -c alacritty -n "__fish_alacritty_needs_command" -l hold -d 'Remain open after child process exit'
complete -c alacritty -n "__fish_alacritty_needs_command" -s h -l help -d 'Print help'
complete -c alacritty -n "__fish_alacritty_needs_command" -s V -l version -d 'Print version'
complete -c alacritty -n "__fish_alacritty_needs_command" -f -a "msg" -d 'Send a message to the Alacritty socket'
complete -c alacritty -n "__fish_alacritty_needs_command" -f -a "migrate" -d 'Migrate the configuration file'
complete -c alacritty -n "__fish_alacritty_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and not __fish_seen_subcommand_from create-window config get-config help" -s s -l socket -d 'IPC socket connection path override' -r -F
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and not __fish_seen_subcommand_from create-window config get-config help" -s h -l help -d 'Print help'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and not __fish_seen_subcommand_from create-window config get-config help" -f -a "create-window" -d 'Create a new window in the same Alacritty process'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and not __fish_seen_subcommand_from create-window config get-config help" -f -a "config" -d 'Update the Alacritty configuration'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and not __fish_seen_subcommand_from create-window config get-config help" -f -a "get-config" -d 'Read runtime Alacritty configuration'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and not __fish_seen_subcommand_from create-window config get-config help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from create-window" -l working-directory -d 'Start the shell in the specified working directory' -r -F
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from create-window" -s e -l command -d 'Command and args to execute (must be last argument)' -r
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from create-window" -s T -l title -d 'Defines the window title [default: Alacritty]' -r
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from create-window" -l class -d 'Defines window class/app_id on X11/Wayland [default: Alacritty]' -r
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from create-window" -s o -l option -d 'Override configuration file options [example: \'cursor.style="Beam"\']' -r
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from create-window" -l hold -d 'Remain open after child process exit'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from create-window" -s h -l help -d 'Print help'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from config" -s w -l window-id -d 'Window ID for the new config' -r
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from config" -s r -l reset -d 'Clear all runtime configuration changes'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from config" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from get-config" -s w -l window-id -d 'Window ID for the config request' -r
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from get-config" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "create-window" -d 'Create a new window in the same Alacritty process'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "config" -d 'Update the Alacritty configuration'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "get-config" -d 'Read runtime Alacritty configuration'
complete -c alacritty -n "__fish_alacritty_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c alacritty -n "__fish_alacritty_using_subcommand migrate" -s c -l config-file -d 'Path to the configuration file' -r -F
complete -c alacritty -n "__fish_alacritty_using_subcommand migrate" -s d -l dry-run -d 'Only output TOML config to STDOUT'
complete -c alacritty -n "__fish_alacritty_using_subcommand migrate" -s i -l skip-imports -d 'Do not recurse over imports'
complete -c alacritty -n "__fish_alacritty_using_subcommand migrate" -l skip-renames -d 'Do not move renamed fields to their new location'
complete -c alacritty -n "__fish_alacritty_using_subcommand migrate" -s s -l silent -d 'Do not output to STDOUT'
complete -c alacritty -n "__fish_alacritty_using_subcommand migrate" -s h -l help -d 'Print help'
complete -c alacritty -n "__fish_alacritty_using_subcommand help; and not __fish_seen_subcommand_from msg migrate help" -f -a "msg" -d 'Send a message to the Alacritty socket'
complete -c alacritty -n "__fish_alacritty_using_subcommand help; and not __fish_seen_subcommand_from msg migrate help" -f -a "migrate" -d 'Migrate the configuration file'
complete -c alacritty -n "__fish_alacritty_using_subcommand help; and not __fish_seen_subcommand_from msg migrate help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c alacritty -n "__fish_alacritty_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "create-window" -d 'Create a new window in the same Alacritty process'
complete -c alacritty -n "__fish_alacritty_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "config" -d 'Update the Alacritty configuration'
complete -c alacritty -n "__fish_alacritty_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "get-config" -d 'Read runtime Alacritty configuration'
