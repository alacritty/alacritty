ALACRITTY(5)

# NAME

Alacritty - TOML configuration file format

# SYNTAX

Alacritty's configuration file uses the TOML format. The format's specification
can be found at _https://toml.io/en/v1.0.0_.

# GENERAL

This section documents the root level of the configuration file.

*import* [<string>,]

	Import additional configuration files

	Imports are loaded in order, skipping all missing files, with the importing
	file being loaded last. If a field is already present in a previous import,
	it will be replaced.

	All imports must either be absolute paths starting with _/_, or paths
	relative to the user's home directory starting with _~/_.

	Example:
		import = [++
	_"~/.config/alacritty/base16-dark.toml"_,++
	_"~/.config/alacritty/keybindings.toml"_,++
]

*shell* <string> | { program = <string>, args = [<string>,] }

	You can set _shell.program_ to the path of your favorite shell, e.g.
	_/bin/zsh_. Entries in _shell.args_ are passed as arguments to the shell.

	Default:
		Linux/BSD/macOS: _$SHELL_ or the user's login shell, if _$SHELL_ is unset++
Windows: _"powershell"_

	Example:
		*[shell]*++
program = _"/bin/zsh"_++
args = [_"-l"_]

*working_directory* <string> | "None"

	Directory the shell is started in. When this is unset, or _"None"_, the
	working directory of the parent process will be used.

	Default: _"None"_

*live_config_reload* <boolean>

	Live config reload (changes require restart)

	Default: _true_

*ipc_socket* <boolean> _(unix only)_

	Offer IPC using _alacritty msg_

	Default: _true_

# ENV

All key-value pairs in the *env* section will be added as environment variables
for any process spawned by Alacritty, including its shell. Some entries may
override variables set by alacritty itself.

Example:
	*[env]*++
WINIT_X11_SCALE_FACTOR = _"1.0"_

# WINDOW

This section documents the *[window]* table of the configuration file.

*dimensions* { columns = <integer>, lines = <integer> }

	Window dimensions (changes require restart)

	Number of lines/columns (not pixels) in the terminal. Both lines and columns
	must be non-zero for this to take effect. The number of columns must be at
	least _2_, while using a value of _0_ for columns and lines will fall back
	to the window manager's recommended size

	Default: _{ column = 0, lines = 0 }_

*position* "None" | { x = <integer>, y = <integer> }

	Window startup position

	Specified in number of pixels.

	If the position is _"None"_, the window manager will handle placement.

	Default: _"None"_

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

*blur* <boolean> _(works on macOS/KDE Wayland)_

	Request compositor to blur content behind transparent windows.

	Default: _false_

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

*decorations_theme_variant* "Dark" | "Light" | "None"

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
	characters (Unicode points _U+2500_ - _U+259F_) and powerline symbols
	(Unicode points _U+E0B0_ - _U+E0B3_).

	Default: _true_

# Colors

This section documents the *[colors]* table of the configuration file.

Colors are specified using their hexadecimal values with a _#_ prefix: _#RRGGBB_

*primary*

	*foreground* <string>

		Default: _"#d8d8d8"_

	*background* <string>

		Default: _"#181818"_

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

		Default: _{ foreground = "#181818", background: "#ac4242" }_

	*focused_match* { foreground = <string>, background = <string> }

		Default: _{ foreground = "#181818", background: "#f4bf75" }_

*hints*

	*start* { foreground = <string>, background = <string> }

		First character in the hint label

		Allowed values are hexadecimal colors like _#ff00ff_, or
		_CellForeground_/_CellBackground_, which references the affected cell.

		Default: _{ foreground = "#181818", background = "#f4bf75" }_

	*end* { foreground = <string>, background = <string> }

		All characters after the first one in the hint label

		Allowed values are hexadecimal colors like _#ff00ff_, or
		_CellForeground_/_CellBackground_, which references the affected cell.

		Default: _{ foreground = "#181818", background = "#ac4242" }_

*line_indicator* { foreground = <string>, background = <string> }

	Color used for the indicator displaying the position in history during
	search and vi mode.

	Setting this to _"None"_ will use the opposing primary color.

	Default: _{ foreground = "None", background = "None" }_

*footer_bar* { foreground = <string>, background = <string> }

	Color used for the footer bar on the bottom, used by search regex input,
	hyperlink URI preview, etc.

	Default: _{ foreground = "#181818", background = "#d8d8d8" }_

*selection* { text = <string>, background = <string> }

	Colors used for drawing selections.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	Default: _{ text = "CellBackground", background = "CellForeground" }_

*normal*

	*black* <string>
		Default: _"#181818"_
	*red* <string>
		Default: _"#ac4242"_
	*green* <string>
		Default: _"#90a959"_
	*yellow* <string>
		Default: _"#f4bf75"_
	*blue* <string>
		Default: _"#6a9fb5"_
	*magenta* <string>
		Default: _"#aa759f"_
	*cyan* <string>
		Default: _"#75b5aa"_
	*white* <string>
		Default: _"#d8d8d8"_

*bright*

	*black* <string>
		Default: _"#6b6b6b"_
	*red* <string>
		Default: _"#c55555"_
	*green* <string>
		Default: _"#aac474"_
	*yellow* <string>
		Default: _"#feca88"_
	*blue* <string>
		Default: _"#82b8c8"_
	*magenta* <string>
		Default: _"#c28cb8"_
	*cyan* <string>
		Default: _"#93d3c3"_
	*white* <string>
		Default: _"#f8f8f8"_

*dim*

	If the dim colors are not set, they will be calculated automatically based
	on the _normal_ colors.

	*black* <string>
		Default: _"#0f0f0f"_
	*red* <string>
		Default: _"#712b2b"_
	*green* <string>
		Default: _"#5f6f3a"_
	*yellow* <string>
		Default: _"#a17e4d"_
	*blue* <string>
		Default: _"#456877"_
	*magenta* <string>
		Default: _"#704d68"_
	*cyan* <string>
		Default: _"#4d7770"_
	*white* <string>
		Default: _"#8e8e8e"_

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

	Default: _false_

# Bell

This section documents the *[bell]* table of the configuration file.

*animation* "Ease" | "EaseOut" | "EaseOutSine" | "EaseOutQuad" | "EaseOutCubic"
\| "EaseOutQuart" | "EaseOutQuint" | "EaseOutExpo" | "EaseOutCirc" | "Linear"

	Visual bell animation effect for flashing the screen when the visual bell is rung.

	Default: _"Linear"_

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

	If the vi mode cursor style is _"None"_ or not specified, it will fall back to
	the active value of the normal cursor.

	See _cursor.style_ for available options.

	Default: _"None"_

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

# Terminal

This section documents the *[terminal]* table of the configuration file.

*osc52* "Disabled" | "OnlyCopy" | "OnlyPaste" | "CopyPaste"

	Controls the ability to write to the system clipboard with the _OSC 52_
	escape sequence. While this escape sequence is useful to copy contents
	from the remote server, allowing any application to read from the clipboard
	can be easily abused while not providing significant benefits over
	explicitly pasting text.

	Default: _"OnlyCopy"_

# Mouse

This section documents the *[mouse]* table of the configuration file.

*hide_when_typing* <boolean>

	When this is _true_, the cursor is temporarily hidden when typing.

	Default: _false_

*bindings*: [{ <mouse>, <mods>, <mode>, <action> | chars = <string> },]

	See _keyboard.bindings_ for full documentation on _mods_, _mode_, _action_,
	and _chars_.

	When an application running within Alacritty captures the mouse, the `Shift`
	modifier can be used to suppress mouse reporting. If no action is found for
	the event, actions for the event without the `Shift` modifier are triggered
	instead.

	*mouse* "Middle" | "Left" | "Right" | "Back" | "Forward" | <number>

		Mouse button which needs to be pressed to trigger this binding.

	*action* <keyboard.bindings.action> | "ExpandSelection"

		*ExpandSelection*
			Expand the selection to the current mouse cursor location.

	Example:
		*[mouse]*++
bindings = [++
	{ mouse = _"Right"_, mods = _"Control"_, action = _"Paste"_ },++
]

# Hints

This section documents the *[hints]* table of the configuration file.

Terminal hints can be used to find text or hyperlinks in the visible part of the
terminal and pipe it to other applications.

*alphabet* <string>

	Keys used for the hint labels.

	Default: _"jfkdls;ahgurieowpq"_

*enabled* [{ <regex>, <hyperlinks>, <post_processing>, <persist>, <action>, <command> <binding>, <mouse> },]

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

	*persist* <boolean>

		When this is _true_, hints remain persistent after selection.

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

		See _keyboard.bindings_ for documentation on available values.

		This controls which key binding is used to start the keyboard hint
		selection process.

	*mouse* { mods = <string>, enabled = <boolean> }

		See _keyboard.bindings_ for documentation on available _mods_.

		The _enabled_ field controls if the hint should be underlined when
		hovering over the hint text with all _mods_ pressed.

Default: _[{
	regex = "(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ssh:|ftp://)[^\\u0000-\\u001F\\u007F-\\u009F<>\\"\\\\s{-}\\\\^⟨⟩`]+",++
hyperlinks = true,++
post_processing = true,++
persist = false,++
mouse = { enabled = true },++
binding = { key = "U", mods = "Control | Shift" },
}]_

# Keyboard

This section documents the *[keyboard]* table of the configuration file.

*bindings*: [{ <key>, <mods>, <mode>, <action> | chars = <string> },]

	To unset a default binding, you can use the action _"ReceiveChar"_ to remove
	it or _"None"_ to inhibit any action.

	Multiple keybindings can be triggered by a single key press and will be
	executed in the order they are defined in.

	*key* <string>

		The regular keys like _"A"_, _"0"_, and _"Я"_ can be mapped directly
		without any special syntax. Full list of named keys like _"F1"_ and the
		syntax for dead keys can be found here:

		https://docs.rs/winit/latest/winit/keyboard/enum.NamedKey.html++
https://docs.rs/winit/latest/winit/keyboard/enum.Key.html#variant.Dead

		Numpad keys are prefixed by _Numpad_: "NumpadEnter" | "NumpadAdd" |
		"NumpadComma" | "NumpadDivide" | "NumpadEquals" | "NumpadSubtract" |
		"NumpadMultiply" | "Numpad[0-9]".

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

		*ReceiveChar*
			Allow receiving char input.
		*None*
			No action.
		*Paste*
			Paste contents of system clipboard.
		*Copy*
			Store current selection into clipboard.
		*IncreaseFontSize*
			Increase font size.
		*DecreaseFontSize*
			Decrease font size.
		*ResetFontSize*
			Reset font size to the config value.
		*ScrollPageUp*
			Scroll exactly one page up.
		*ScrollPageDown*
			Scroll exactly one page down.
		*ScrollHalfPageUp*
			Scroll half a page up.
		*ScrollHalfPageDown*
			Scroll half a page down.
		*ScrollLineUp*
			Scroll one line up.
		*ScrollLineDown*
			Scroll one line down.
		*ScrollToTop*
			Scroll all the way to the top.
		*ScrollToBottom*
			Scroll all the way to the bottom.
		*ClearHistory*
			Clear the display buffer(s) to remove history.
		*Hide*
			Hide the Alacritty window.
		*Minimize*
			Minimize the Alacritty window.
		*Quit*
			Quit Alacritty.
		*ClearLogNotice*
			Clear warning and error notices.
		*SpawnNewInstance*
			Spawn a new instance of Alacritty.
		*CreateNewWindow*
			Create a new Alacritty window.
		*ToggleFullscreen*
			Toggle fullscreen.
		*ToggleMaximized*
			Toggle maximized.
		*ClearSelection*
			Clear active selection.
		*ToggleViMode*
			Toggle vi mode.
		*SearchForward*
			Start a forward buffer search.
		*SearchBackward*
			Start a backward buffer search.

		_Vi mode actions:_

		*Up*
			Move up.
		*Down*
			Move down.
		*Left*
			Move left.
		*Right*
			Move right.
		*First*
			First column, or beginning of the line when already at the first column.
		*Last*
			Last column, or beginning of the line when already at the last column.
		*FirstOccupied*
			First non-empty cell in this terminal row, or first non-empty cell
			of the line when already at the first cell of the row.
		*High*
			Move to top of screen.
		*Middle*
			Move to center of screen.
		*Low*
			Move to bottom of screen.
		*SemanticLeft*
			Move to start of semantically separated word.
		*SemanticRight*
			Move to start of next semantically separated word.
		*SemanticLeftEnd*
			Move to end of previous semantically separated word.
		*SemanticRightEnd*
			Move to end of semantically separated word.
		*WordLeft*
			Move to start of whitespace separated word.
		*WordRight*
			Move to start of next whitespace separated word.
		*WordLeftEnd*
			Move to end of previous whitespace separated word.
		*WordRightEnd*
			Move to end of whitespace separated word.
		*Bracket*
			Move to opposing bracket.
		*ToggleNormalSelection*
			Toggle normal vi selection.
		*ToggleLineSelection*
			Toggle line vi selection.
		*ToggleBlockSelection*
			Toggle block vi selection.
		*ToggleSemanticSelection*
			Toggle semantic vi selection.
		*SearchNext*
			Jump to the beginning of the next match.
		*SearchPrevious*
			Jump to the beginning of the previous match.
		*SearchStart*
			Jump to the next start of a match to the left of the origin.
		*SearchEnd*
			Jump to the next end of a match to the right of the origin.
		*Open*
			Launch the URL below the vi mode cursor.
		*CenterAroundViCursor*
			Centers the screen around the vi mode cursor.
		*InlineSearchForward*
			Search forward within the current line.
		*InlineSearchBcakward*
			Search backward within the current line.
		*InlineSearchForwardShort*
			Search forward within the current line, stopping just short of the character.
		*InlineSearchBackwardShort*
			Search backward within the current line, stopping just short of the character.
		*InlineSearchNext*
			Jump to the next inline search match.
		*InlineSearchPrevious*
			Jump to the previous inline search match.

		_Search actions:_

		*SearchFocusNext*
			Move the focus to the next search match.
		*SearchFocusPrevious*
			Move the focus to the previous search match.
		*SearchConfirm*
			Confirm the active search.
		*SearchCancel*
			Cancel the active search.
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
		*HideOtherApplications*
			Hide all windows other than Alacritty.
		*CreateNewTab*
			Create new window in a tab.
		*SelectNextTab*
			Select next tab.
		*SelectPreviousTab*
			Select previous tab.
		*SelectTab1*
			Select the first tab.
		*SelectTab2*
			Select the second tab.
		*SelectTab3*
			Select the third tab.
		*SelectTab4*
			Select the fourth tab.
		*SelectTab5*
			Select the fifth tab.
		*SelectTab6*
			Select the sixth tab.
		*SelectTab7*
			Select the seventh tab.
		*SelectTab8*
			Select the eighth tab.
		*SelectTab9*
			Select the ninth tab.
		*SelectLastTab*
			Select the last tab.

		_Linux/BSD exclusive:_

		*CopySelection*
			Copy from the selection buffer.
		*PasteSelection*
			Paste from the selection buffer.

Default: See *alacritty-bindings*(5)

Example:
	*[keyboard]*++
bindings = [++
	{ key = _"N"_, mods = _"Control|Shift"_, action = _"CreateNewWindow"_ },++
]


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

	To add extra libraries to logging _ALACRITTY_EXTRA_LOG_TARGETS_ variable
	can be used.

	Example:
		_ALACRITTY_EXTRA_LOG_TARGETS="winit;vte" alacritty -vvv_

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

*prefer_egl* <boolean>

	Use EGL as display API if the current platform allows it. Note that
	transparency may not work with EGL on Linux/BSD.

	Default: _false_

# SEE ALSO

*alacritty*(1), *alacritty-msg*(1), *alacritty-bindings*(5)

See the alacritty github repository at https://github.com/alacritty/alacritty
for the full documentation.

# BUGS

Found a bug? Please report it at https://github.com/alacritty/alacritty/issues.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
