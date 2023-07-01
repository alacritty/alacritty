ALACRITTY-MSG(1)

# NAME

alacritty-msg - Send messages to Alacritty

# SYNOPSIS

This command communicates with running Alacritty instances through a socket,
making it possible to control Alacritty without directly accessing it.

# OPTIONS

*-s, --socket* <socket>
	Path for IPC socket communication

# MESSAGES

*create-window*
	Create a new window in the same Alacritty process

	*OPTIONS*
		*--hold*
			Remain open after child process exits

		*--working-directory* <working-directory>
			Start the shell in the specified working directory

		*command* <command>...
			Command and args to execute (must be last argument)

*config*
	Update the Alacritty configuration

	*ARGS*
		*<CONFIG_OPTIONS>...*
			Configuration file options [example: cursor.style=Beam]

	*OPTIONS*
		*-w, --window-id* <WINDOW_ID>
			Window ID for the new config.

			Use _-1_ to apply this change to all windows.

			\[default: *$ALACRITTY_WINDOW_ID*]

# SEE ALSO

*alacritty*(1), *alacritty*(5), *alacritty-bindings*(5)

See the alacritty github repository at https://github.com/alacritty/alacritty
for the full documentation.

# BUGS

Found a bug? Please report it at https://github.com/alacritty/alacritty/issues.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
