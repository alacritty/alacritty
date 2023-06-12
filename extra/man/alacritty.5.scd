ALACRITTY(5)

# NAME

Alacritty - TOML configuration file format

# SYNTAX

Alacritty's configuration file uses the TOML format. The format's specification
can be found at https://toml.io/en/v1.0.0.

# GENERAL

This section documents the root level of the configuration file.

*import* [<string>,]

	Import additional configuration files

	Imports are loaded in order, skipping all missing files, with the importing
	file being loaded last. If a field is already present in a previous import,
	it will be replaced.

	All imports must either be absolute paths starting with _/_, or paths
	relative to the user's home directory starting with _~/_.

*working_directory* <string> | "None"

	Directory the shell is started in. When this is unset, or _None_, the
	working directory of the parent process will be used.

	Default: _None_

*live_config_reload* <boolean>

	Live config reload (changes require restart)

*ipc_socket* <boolean> _(unix only)_

	Offer IPC using _alacritty msg_

# ENV

All key-value pairs in the *env* section will be added as environment variables
for any process spawned by Alacritty, including its shell. Some entries may
override variables set by alacritty itself.

```
[env]
TERM = "alacritty"
```

# WINDOW

This section documents the *[window]* table of the configuration file.

*dimensions* { columns = <integer>, lines = <integer> }

	Window dimensions (changes require restart)

	Number of lines/columns (not pixels) in the terminal. Both lines and columns
	must be non-zero for this to take effect. The number of columns must be at
	least _2_, while using a value of _0_ for columns and lines will fall back
	to the window manager's recommended size

	Default: _{ column = 0, lines = 0 }_

*padding* { x = <integer>, y = <integer> }

	Blank space added around the window in pixels. This padding is scaled
	by DPI and the specified value is always added at both opposing sides.

	Default: _{ x = 0, y = 0 }_

*dynamic_padding* <boolean>

	Spread additional padding evenly around the terminal content.

	Default: _false_

*decorations* "Full" | "None" | "Transparent" | "Buttonless"

	Window decorations

	*Full*
		Borders and title bar
	*None*
		Neither borders nor title bar
	*Transparent* _(macOS only)_
		Title bar, transparent background and title bar buttons
	*Buttonless* _(macOS only)_
		Title bar, transparent background and no title bar buttons

	Default: _"Full"_

*opacity* <float>

	Background opacity as a floating point number from _0.0_ to _1.0_. The value
	\_0.0_ is completely transparent and _1.0_ is opaque.

	Default: _1.0_

*startup_mode* "Windowed" | "Maximized" | "Fullscreen" | "SimpleFullscreen"

	Startup mode (changes require restart)

	Default: _"Windowed"_

*title* <string>

	Window title

	Default: _"Alacritty"_

*dynamic_title* <boolean>

	Allow terminal applications to change Alacritty's window title.

	Default: _true_

*class* { instance = <string>, general = <string> } _(Linux/BSD only)_

	Window class

	On Wayland, *general* is used as _app\_id_ and *instance* is ignored.

	Default: _{ instance = "Alacritty", general = "Alacritty" }_

*decoration_theme_variant* "Dark" | "Light" | "None"

	Override the variant of the System theme/GTK theme/Wayland client side
	decorations. Set this to _"None"_ to use the system's default theme variant.

	Default: _"None"_

*resize_increments* <boolean>

	Prefer resizing window by discrete steps equal to cell dimensions.

	Default: _false_

*option_as_alt* "OnlyLeft" | "OnlyRight" | "Both" | "None" _(macos only)_

	Make _Option_ key behave as _Alt_

	Default: _"None"_

# Scrolling

This section documents the *[scrolling]* table of the configuration file.

*history* <integer>

	Maximum number of lines in the scrollback buffer.++
Specifying _0_ will disable scrolling.++
Limited to _100000_.

	Default: _10000_

*multiplier* <integer>

	Number of line scrolled for every input scroll increment.

	Default: _3_

# Font

This section documents the *[font]* table of the configuration file.

*normal* { family = <string>, style = <string> }

	Default:
		Linux/BSD: _{ family = "monospace", style = "Regular" }_++
Windows:   _{ family = "Consolas",  style = "Regular" }_++
macOS:     _{ family = "Menlo",     style = "Regular" }_

*bold* { family = <string>, style = <string> }

	If the family is not specified, it will fall back to the value specified for
	the normal font.

	Default: _{ style = "Bold" }_

*italic* { family = <string>, style = <string> }

	If the family is not specified, it will fall back to the value specified for
	the normal font.

	Default: _{ style = "Italic" }_

*bold_italic* { family = <string>, style = <string> }

	If the family is not specified, it will fall back to the value specified for
	the normal font.

	Default: _{ style = "Bold Italic" }_

*size* <float>

	Font size in points.

	Default: _11.0_

*offset* { x = <integer>, y = <integer> }

	Offset is the extra space around each character. _y_ can be thought of as
	modifying the line spacing, and _x_ as modifying the letter spacing.

	Default: _{ x = 0, y = 0 }_

*glyph_offset* { x = <integer>, y = <integer> }

	Glyph offset determines the locations of the glyphs within their cells with
	the default being at the bottom. Increasing _x_ moves the glyph to the
	right, increasing _y_ moves the glyph upward.

*builtin_box_drawing* <boolean>

	When _true_, Alacritty will use a custom built-in font for box drawing
	characters (Unicode points _U+2500_ - _U+259f_).

	Default: _true_

# Colors

This section documents the *[colors]* table of the configuration file.

Colors are specified using their hexadecimal values with a _#_ prefix: _#RRGGBB_

*primary*

	*foreground* <string>

		Default: _"#1d1f21"_

	*background* <string>

		Default: _"#c5c8c6"_

	*dim_foreground* <string>

		If this is not set, the color is automatically calculated based on the
		foreground color.

		Default: _"#828482"_

	*bright_foreground* <string>

		This color is only used when _draw\_bold\_text\_with\_bright\_colors_ is
		_true_.

		If this is not set, the normal foreground will be used.

		Default: _"None"_

*cursor* { text = <string>, cursor = <string> }

	Colors which should be used to draw the terminal cursor.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	Default: _{ text = "CellBackground", cursor: "CellForeground" }_

*vi_mode_cursor* { text = <string>, cursor = <string> }

	Colors for the cursor when the vi mode is active.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	Default: _{ text = "CellBackground", cursor: "CellForeground" }_

*search*

	Colors used for the search bar and match highlighting.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	*matches* { foreground = <string>, background = <string> }

		Default: _{ foreground = "#000000", background: "#ffffff" }_

	*focused_match* { foreground = <string>, background = <string> }

		Default: _{ foreground = "#ffffff", background: "#000000" }_

*hints*

	*start* { foreground = <string>, background = <string> }

		First character in the hint label

		Allowed values are hexadecimal colors like _#ff00ff_, or
		_CellForeground_/_CellBackground_, which references the affected cell.

		Default: _{ foreground = "#1d1f21", background = "#e9ff5e" }_

	*end* { foreground = <string>, background = <string> }

		All characters after the first one in the hint label

		Allowed values are hexadecimal colors like _#ff00ff_, or
		_CellForeground_/_CellBackground_, which references the affected cell.

		Default: _{ foreground = "#e9ff5e", background = "#1d1f21" }_

*line_indicator* { foreground = <string>, background = <string> }

	Color used for the indicator displaying the position in history during
	search and vi mode.

	Setting this to _"None"_ will use the opposing primary color.

	Default: _{ foreground = "None", background = "None" }_

*footer_bar* { foreground = <string>, background = <string> }

	Color used for the footer bar on the bottom, used by search regex input,
	hyperlink URI preview, etc.

	Default: _{ foreground = "#c5c8c6", background = "#1d1f21" }_

*selection* { text = <string>, background = <string> }

	Colors used for drawing selections.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	Default: _{ text = "CellBackground", background = "CellForeground" }_

*normal*

	*black* <string>
		Default: _"#1d1f21"_
	*red* <string>
		Default: _"#cc6666"_
	*green* <string>
		Default: _"#b5bd68"_
	*yellow* <string>
		Default: _"#f0c674"_
	*blue* <string>
		Default: _"#81a2be"_
	*magenta* <string>
		Default: _"#b294bb"_
	*cyan* <string>
		Default: _"#8abeb7"_
	*white* <string>
		Default: _"#c5c8c6"_

*bright*

	*black* <string>
		Default: _"#666666"_
	*red* <string>
		Default: _"#d54e53"_
	*green* <string>
		Default: _"#b9ca4a"_
	*yellow* <string>
		Default: _"#e7c547"_
	*blue* <string>
		Default: _"#7aa6da"_
	*magenta* <string>
		Default: _"#c397d8"_
	*cyan* <string>
		Default: _"#70c0b1"_
	*white* <string>
		Default: _"#eaeaea"_

*dim*

	If the dim colors are not set, they will be calculated automatically based
	on the _normal_ colors.

	*black* <string>
		Default: _"#131415"_
	*red* <string>
		Default: _"#864343"_
	*green* <string>
		Default: _"#777c44"_
	*yellow* <string>
		Default: _"#9e824c"_
	*blue* <string>
		Default: _"#556a7d"_
	*magenta* <string>
		Default: _"#75617b"_
	*cyan* <string>
		Default: _"#5b7d78"_
	*white* <string>
		Default: _"#828482"_

*indexed_colors* [{ index = <integer>, color = <string> },]

	The indexed colors include all colors from 16 to 256.
	When these are not set, they're filled with sensible defaults.

	Default: _[]_

*transparent_background_colors* <boolean>

	Whether or not _window.opacity_ applies to all cell backgrounds, or only to
	the default background. When set to _true_ all cells will be transparent
	regardless of their background color.

	Default: _false_

*draw_bold_text_with_bright_colors* <boolean>

	When _true_, bold text is drawn using the bright color variants.

# Bell

This section documents the *[bell]* table of the configuration file.

*animation* "Ease" | "EaseOut" | "EaseOutSine" | "EaseOutQuad" | "EaseOutCubic"
\| "EaseOutQuart" | "EaseOutQuint" | "EaseOutExpo" | "EaseOutCirc" | "Linear"

	Visual bell animation effect for flashing the screen when the visual bell is rung.

	Default: _"EaseOutExpo"_

*duration* <integer>

	Duration of the visual bell flash in milliseconds. A `duration` of `0` will
	disable the visual bell animation.

	Default: _0_

*color* <string>

	Visual bell animation color.

	Default: _"#ffffff"_

*command* <string> | { program = <string>, args = [<string>,] }

	This program is executed whenever the bell is rung.

	When set to _"None"_, no command will be executed.

	Default: _"None"_

# Selection

This section documents the *[selection]* table of the configuration file.

*semantic_escape_chars* <string>

	This string contains all characters that are used as separators for
	"semantic words" in Alacritty.

	Default: _",│`|:\"' ()[]{}<>\t"_

*save_to_clipboard* <boolean>

	When set to _true_, selected text will be copied to the primary clipboard.

	Default: _false_

# Cursor

This section documents the *[cursor]* table of the configuration file.

*style*

	*shape* "Block" | "Underline" | "Beam"

		Default: _"Block"_

	*blinking* "Never" | "Off" | "On" | "Always"

		*Never*
			Prevent the cursor from ever blinking
		*Off*
			Disable blinking by default
		*On*
			Enable blinking by default
		*Always*
			Force the cursor to always blink

		Default: _"Off"_

*vi_mode_style*

	If the vi mode cursor style is _None_ or not specified, it will fall back to
	the active value of the normal cursor.

	See _cursor.style_ for available options.

	Default: _None_

*blink_interval* <integer>

	Cursor blinking interval in milliseconds.

	Default: _750_

*blink_timeout* <integer>

	Time after which cursor stops blinking, in seconds.

	Specifying _0_ will disable timeout for blinking.

	Default: _5_

*unfocused_hollow* <boolean>

	When this is _true_, the cursor will be rendered as a hollow box when the
	window is not focused.

	Default: _true_

*thickness* <float>

	Thickness of the cursor relative to the cell width as floating point number
	from _0.0_ to _1.0_.

	Default: _0.15_

*shell* <string> | { program = <string>, args = [<string>,] }

	You can set _shell.program_ to the path of your favorite shell, e.g.
	_/bin/zsh_. Entries in _shell.args_ are passed as arguments to the shell.

	Default:
		Linux/BSD/macOS: _$SHELL_ or the user's login shell, if _$SHELL_ is unset++
Windows: _"powershell"_

# Mouse

This section documents the *[mouse]* table of the configuration file.

*double_click* { threshold = <integer> }

	Set maximum time between double-clicks in milliseconds.

	Default: _300_

*triple_click* { threshold = <integer> }

	Set maximum time between triple-clicks in milliseconds.

	Default: _300_

*hide_when_typing* <boolean>

	When this is _true_, the cursor is temporarily hidden when typing.

	Default: _false_

# Hints

This section documents the *[hints]* table of the configuration file.

Terminal hints can be used to find text or hyperlinks in the visible part of the
terminal and pipe it to other applications.

*alphabet* <string>

	Keys used for the hint labels.

	Default: _"jfkdls;ahgurieowpq"_

*enabled* [{ <regex>, <hyperlinks>, <post_processing>, <action>, <command> <binding>, <mouse> },]

Array with all available hints.

Each hint must have at least one of _regex_ or _hyperlinks_ and either an
_action_ or a _command_.

	*regex* <string>

		Regex each line will be compared against.

	*hyperlinks* <boolean>

		When this is _true_, all OSC 8 escape sequence hyperlinks will be
		included in the hints.

	*post_processing* <boolean>

		When this is _true_, heuristics will be used to shorten the match if
		there are characters likely not to be part of the hint (e.g. a trailing
		_._). This is most useful for URIs and applies only to _regex_ matches.

	*action* "Copy" | "Paste" | "Select" | "MoveViModeCursor"

		*Copy*
			Copy the hint's text to the clipboard.
		*Paste*
			Paste the hint's text to the terminal or search.
		*Select*
			Select the hint's text.
		*MoveViModeCursor*
			Move the vi mode cursor to the beginning of the hint.

	*command* <string> | { program = <string>, args = [<string>,] }

		Command which will be executed when the hint is clicked or selected with
		the _binding_.

		The hint's text is always attached as the last argument.

	*binding* { key = <string>, mods = <string>, mode = <string> }

		See _key_bindings_ for documentation on available values.

		This controls which key binding is used to start the keyboard hint
		selection process.

	*mouse* { mods = <string>, enabled = <boolean> }

		See _key_bindings_ for documentation on available _mods_.

		The _enabled_ field controls if the hint should be underlined when
		hovering over the hint text with all _mods_ pressed.

Default: _[{
	regex = "(mailto:|gemini:|gopher:|https:|http:|news:|file:|git:|ssh:|ftp:)[^\\u0000-\\u001F\\u007F-\\u009F<>\\"\\\\s{-}\\\\^⟨⟩`]+",++
hyperlinks = true,++
post_processing = true,++
mouse = { enabled = true },++
binding = { key = "U", mods = "Control | Shift" },
}]_

# Bindings

This section documents the *mouse_bindings* and *key_bindings* at the root level
of the configuration file.

*mouse_bindings* [{ <mouse>, <mods>, <mode>, <action> | chars = <string> },]

	See _key_bindings_ for full documentation on _mods_, _mode_, _action_, and
	_chars_.

	To trigger mouse bindings when an application running within Alacritty
	captures the mouse, the `Shift` modifier is automatically added as a
	requirement.

	*mouse* "Middle" | "Left" | "Right" | <number>

		Mouse button which needs to be pressed to trigger this binding.

	*action* <key_bindings.action> | "ExpandSelection"

		*ExpandSelection*
			Expand the selection to the current mouse cursor location.

*key_bindings* [{ <key>, <mods>, <mode>, <action> | chars = <string> },]

	To unset a default binding, you can use the action _"ReceiveChar"_ to remove
	it or _"None"_ to inhibit any action.

	Multiple keybindings can be triggered by a single key press and will be
	executed in the order they are defined in.

	*key* <string>

		Identifier of the binding's key, for example: _"A"_, _"F1"_, or
		_"Key0"_.

		A full list with available key codes can be found here:++
https://docs.rs/winit/\*/winit/event/enum.VirtualKeyCode.html#variants

		The _key_ field also supports using scancodes, which are specified as a
		decimal number.

	*mods* "Command" | "Control" | "Option" | "Super" | "Shift" | "Alt"

		Multiple modifiers can be combined using _|_, like this: _"Control |
		Shift"_.

	*mode* "AppCursor" | "AppKeypad" | "Search" | "Alt" | "Vi"

		This defines a terminal mode which must be active for this binding to
		have an effect.

		Prepending _~_ to a mode will require the mode to *not* be active for
		the binding to take effect.

		Multiple modes can be combined using _|_, like this: _"~Vi|Search"_.

	*chars*

		Writes the specified string to the terminal.

	*action*

		*Up*
			One line up.
		*Down*
			One line down.
		*Left*
			One character left.
		*Right*
			One character right.
		*First*
			First column, or beginning of the line when already at the first column.
		*Last*
			Last column, or beginning of the line when already at the last column.
		*FirstOccupied*
			First non-empty cell in this terminal row, or first non-empty cell
			of the line when already at the first cell of the row.
		*High*
			Top of the screen.
		*Middle*
			Center of the screen.
		*Low*
			Bottom of the screen.
		*SemanticLeft*
			the previous semantically separated word.
		*SemanticRight*
			he next semantically separated word.
		*SemanticLeftEnd*
			revious semantically separated word.
		*SemanticRightEnd*
			End of the next semantically separated word.
		*WordLeft*
			Start of the previous whitespace separated word.
		*WordRight*
			Start of the next whitespace separated word.
		*WordLeftEnd*
			End of the previous whitespace separated word.
		*WordRightEnd*
			End of the next whitespace separated word.
		*Bracket*
			Character matching the bracket at the cursor's location.
		*SearchNext*
			Beginning of the next match.
		*SearchPrevious*
			Beginning of the previous match.
		*SearchStart*
			Start of the match to the left of the vi mode cursor.
		*SearchEnd*
			End of the match to the right of the vi mode cursor.

		_Search mode exclusive:_

		*SearchFocusNext*
			Move the focus to the next search match.
		*SearchFocusPrevious*
			Move the focus to the previous search match.
		*SearchConfirm*
		*SearchCancel*
		*SearchClear*
			Reset the search regex.
		*SearchDeleteWord*
			Delete the last word in the search regex.
		*SearchHistoryPrevious*
			Go to the previous regex in the search history.
		*SearchHistoryNext*
			Go to the next regex in the search history.

		_macOS exclusive:_

		*ToggleSimpleFullscreen*
			Enter fullscreen without occupying another space.

		_Linux/BSD exclusive:_

		*CopySelection*
			Copy from the selection buffer.
		*PasteSelection*
			Paste from the selection buffer.

# Debug

This section documents the *[debug]* table of the configuration file.

Debug options are meant to help troubleshoot issues with Alacritty. These can
change or be removed entirely without warning, so their stability shouldn't be
relied upon.

*render_timer* <boolean>

	Display the time it takes to draw each frame.

	Default: _false_

*persistent_logging* <boolean>

	Keep the log file after quitting Alacritty.

	Default: _false_

*log_level* "Off" | "Error" | "Warn" | "Info" | "Debug" | "Trace"

	Default: _"Warn"_

*renderer* "glsl3" | "gles2" | "gles2_pure" | "None"

	Force use of a specific renderer, _"None"_ will use the highest available
	one.

	Default: _"None"_

*print_events* <boolean>

	Log all received window events.

	Default: _false_

*highlight_damage* <boolean>

	Highlight window damage information.

	Default: _false_

# SEE ALSO

See the alacritty github repository at https://github.com/alacritty/alacritty
for the full documentation.

# BUGS

Found a bug? Please report it at https://github.com/alacritty/alacritty/issues.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
