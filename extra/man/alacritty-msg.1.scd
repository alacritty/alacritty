ALACRITTY-MSG(1)

# NAME

alacritty-msg - Send messages to Alacritty.

# SYNOPSIS

This command communicates with running Alacritty instances through a socket,
making it possible to control Alacritty without directly accessing it.

# OPTIONS

*-s, --socket* _<SOCKET>_

	Path for IPC socket communication.

# MESSAGES

*create-window*

	Create a new window in the same Alacritty process.

	*FLAGS*
		*--hold*

			Remain open after child process exits.

	*OPTIONS*
		*--working-directory* _<WORKING_DIRECTORY>_

			Start the shell in the specified working directory.

		*-T, --title* _<TITLE>_

			Defines the window title.

			Default: _Alacritty_

		*--class* _<GENERAL>_ | _<GENERAL>_,_<INSTANCE>_

			Defines window class/app_id on X11/Wayland.

			Default: _Alacritty,Alacritty_

		*-o, --option* _<OPTION>..._

			Override configuration file options.

			Example: _alacritty msg create-window -o 'cursor.style="Beam"'_

		*-e, --command* _<COMMAND>..._

			Command and args to execute (must be last argument).

*config*

	Update the Alacritty configuration.

	*ARGS*
		*<CONFIG_OPTIONS>...*

			Configuration file options.

			Example: _alacritty msg config 'cursor.style="Beam"'_

	*FLAGS*
		*-r, --reset*

			Clear all runtime configuration changes.

	*OPTIONS*
		*-w, --window-id* _<WINDOW_ID>_

			Window ID for the new config.

			Use _-1_ to apply this change to all windows.

			Default: _$ALACRITTY_WINDOW_ID_

*get-config*

	Read runtime Alacritty configuration.

	*OPTIONS*
		*-w, --window-id* _<WINDOW_ID>_

			Window ID for the config request.

			Use _-1_ to get the global config.

			Default: _$ALACRITTY_WINDOW_ID_

# SEE ALSO

*alacritty*(1), *alacritty*(5), *alacritty-bindings*(5)

# BUGS

Found a bug? Please report it at _https://github.com/alacritty/alacritty/issues_.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
