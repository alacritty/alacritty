ALACRITTY(1)

# NAME

Alacritty - A fast, cross-platform, OpenGL terminal emulator

# SYNOPSIS

Alacritty is a modern terminal emulator that comes with sensible defaults, but
allows for extensive configuration. By integrating with other applications,
rather than reimplementing their functionality, it manages to provide a flexible
set of features with high performance.

# FLAGS

*-h, --help*
	Prints help information

*--hold*
	Remain open after child process exits

*--print-events*
	Print all events to stdout

*-q*
	Reduces the level of verbosity (the min level is *-qq*)

*--ref-test*
	Generates ref test

*-v*
	Increases the level of verbosity (the max level is *-vvv*)

*-V, --version*
	Prints version information

# OPTIONS

*--class* <general> | <general>,<instance>
	Defines the window class hint on Linux [default: Alacritty,Alacritty]

	When only the general class is passed, instance will be set to the same value.

	On Wayland the general class sets the _app\_id_, while the instance class is ignored.

*-e, --command* <command>...
	Command and args to execute (must be last argument)

*--config-file* <config-file>
	Specify alternative configuration file

	Alacritty looks for the configuration file at the following paths:
		. $XDG_CONFIG_HOME/alacritty/alacritty.toml
		. $XDG_CONFIG_HOME/alacritty.toml
		. $HOME/.config/alacritty/alacritty.toml
		. $HOME/.alacritty.toml

	On Windows, the configuration file is located at %APPDATA%\\alacritty\\alacritty.toml

*--embed* <parent>
	X11 window ID to embed Alacritty within (decimal or hexadecimal with _0x_ prefix)

*-o, --option* <option>...
	Override configuration file options [example: cursor.style=Beam]

*--socket* <socket>
	Path for IPC socket creation

*-T, --title* <title>
	Defines the window title [default: Alacritty]

*--working-directory* <working-directory>
	Start the shell in the specified working directory

# SUBCOMMANDS

*msg*
	Send IPC socket messages (see *alacritty-msg*(1))

# SEE ALSO

*alacritty-msg*(1), *alacritty*(5), *alacritty-bindings*(5)

See the alacritty github repository at https://github.com/alacritty/alacritty
for the full documentation.

# BUGS

Found a bug? Please report it at https://github.com/alacritty/alacritty/issues.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
