ALACRITTY-BINDINGS(5)

# NAME

Alacritty Bindings - Default configuration file bindings

# SYNOPSIS

This page documents all key and mouse bindings for the default Alacritty
configuration. See *alacritty*(5) for full configuration format documentation.

# Mouse Bindings

[[ *mouse*
:[ *mods*
:[ *action*
|  _"Right"_
:[
:  _"ExpandSelection"_
|  _"Right"_
:  _"Control"_
:  _"ExpandSelection"_
|  _"Middle"_
:  _"~Vi"_
:  _"PasteSelection"_

# Key Bindings

[[ *key*
:[ *mods*
:[ *mode*
:[ *action* / *chars*
|  _"Paste"_
:[
:[
:  _"Paste"_
|  _"Copy"_
:[
:[
:  _"Copy"_
|  _"l"_
:  _"Control"_
:[
:  _"ClearLogNotice"_
|  _"l"_
:  _"Control"_
:  _"~Vi|~Search"_
:  *chars*: _"\\u000c"_
|  _"PageUp"_
:  _"Shift"_
:  _"~Alt"_
:  _"ScrollPageUp"_
|  _"PageDown"_
:  _"Shift"_
:  _"~Alt"_
:  _"ScrollPageDown"_
|  _"Home"_
:  _"Shift"_
:  _"~Alt"_
:  _"ScrollToTop"_
|  _"End"_
:  _"Shift"_
:  _"~Alt"_
:  _"ScrollToBottom"_

## Vi Mode

[[ *key*
:[ *mods*
:[ *mode*
:[ *action*
|  _"Space"_
:  _"Shift|Control"_
:  _"~Search"_
:  _"ToggleViMode"_
|  _"Space"_
:  _"Shift|Control"_
:  _"Vi|~Search"_
:  _"ScrollToBottom"_
|  _"Escape"_
:[
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"i"_
:[
:  _"Vi|~Search"_
:  _"ToggleViMode"_
|  _"i"_
:[
:  _"Vi|~Search"_
:  _"ScrollToBottom"_
|  _"c"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ToggleViMode"_
|  _"y"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollLineUp"_
|  _"e"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollLineDown"_
|  _"g"_
:[
:  _"Vi|~Search"_
:  _"ScrollToTop"_
|  _"g"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"ScrollToBottom"_
|  _"b"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollPageUp"_
|  _"f"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollPageDown"_
|  _"u"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollHalfPageUp"_
|  _"d"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollHalfPageDown"_
|  _"y"_
:[
:  _"Vi|~Search"_
:  _"Copy"_
|  _"y"_
:[
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"Copy"_
:[
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"v"_
:[
:  _"Vi|~Search"_
:  _"ToggleNormalSelection"_
|  _"v"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"ToggleLineSelection"_
|  _"v"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ToggleBlockSelection"_
|  _"v"_
:  _"Alt"_
:  _"Vi|~Search"_
:  _"ToggleSemanticSelection"_
|  _"Enter"_
:[
:  _"Vi|~Search"_
:  _"Open"_
|  _"z"_
:[
:  _"Vi|~Search"_
:  _"CenterAroundViCursor"_
|  _"k"_
:[
:  _"Vi|~Search"_
:  _"Up"_
|  _"j"_
:[
:  _"Vi|~Search"_
:  _"Down"_
|  _"h"_
:[
:  _"Vi|~Search"_
:  _"Left"_
|  _"l"_
:[
:  _"Vi|~Search"_
:  _"Right"_
|  _"ArrowUp"_
:[
:  _"Vi|~Search"_
:  _"Up"_
|  _"ArrowDown"_
:[
:  _"Vi|~Search"_
:  _"Down"_
|  _"ArrowLeft"_
:[
:  _"Vi|~Search"_
:  _"Left"_
|  _"ArrowRight"_
:[
:  _"Vi|~Search"_
:  _"Right"_
|  _"0"_
:[
:  _"Vi|~Search"_
:  _"First"_
|  _"4"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Last"_
|  _"6"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"FirstOccupied"_
|  _"h"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"High"_
|  _"m"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Middle"_
|  _"l"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Low"_
|  _"b"_
:[
:  _"Vi|~Search"_
:  _"SemanticLeft"_
|  _"w"_
:[
:  _"Vi|~Search"_
:  _"SemanticRight"_
|  _"e"_
:[
:  _"Vi|~Search"_
:  _"SemanticRightEnd"_
|  _"b"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"WordLeft"_
|  _"w"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"WordRight"_
|  _"e"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"WordRightEnd"_
|  _"5"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Bracket"_
|  _"/"_
:[
:  _"Vi|~Search"_
:  _"SearchForward"_
|  _"/"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"SearchBackward"_
|  _"n"_
:[
:  _"Vi|~Search"_
:  _"SearchNext"_
|  _"n"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"SearchPrevious"_


## Search Mode

[[ *key*
:[ *mods*
:[ *mode*
:[ *action*
|  _"Enter"_
:[
:  _"Search|Vi"_
:  _"SearchConfirm"_
|  _"Escape"_
:[
:  _"Search"_
:  _"SearchCancel"_
|  _"c"_
:  _"Control"_
:  _"Search"_
:  _"SearchCancel"_
|  _"u"_
:  _"Control"_
:  _"Search"_
:  _"SearchClear"_
|  _"w"_
:  _"Control"_
:  _"Search"_
:  _"SearchDeleteWord"_
|  _"p"_
:  _"Control"_
:  _"Search"_
:  _"SearchHistoryPrevious"_
|  _"n"_
:  _"Control"_
:  _"Search"_
:  _"SearchHistoryNext"_
|  _"ArrowUp"_
:[
:  _"Search"_
:  _"SearchHistoryPrevious"_
|  _"ArrowDown"_
:[
:  _"Search"_
:  _"SearchHistoryNext"_
|  _"Enter"_
:[
:  _"Search|~Vi"_
:  _"SearchFocusNext"_

## Windows, Linux, and BSD only

[[ *key*
:[ *mods*
:[ *mode*
:[ *action*
|  _"v"_
:  _"Control|Shift"_
:  _"~Vi"_
:  _"Paste"_
|  _"c"_
:  _"Control|Shift"_
:[
:  _"Copy"_
|  _"f"_
:  _"Control|Shift"_
:  _"~Search"_
:  _"SearchForward"_
|  _"b"_
:  _"Control|Shift"_
:  _"~Search"_
:  _"SearchBackward"_
|  _"c"_
:  _"Control|Shift"_
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"Insert"_
:  _"Shift"_
:[
:  _"PasteSelection"_
|  _"0"_
:  _"Control"_
:[
:  _"ResetFontSize"_
|  _"="_
:  _"Control"_
:[
:  _"IncreaseFontSize"_
|  _"+"_
:  _"Control"_
:[
:  _"IncreaseFontSize"_
|  _"NumpadAdd"_
:  _"Control"_
:[
:  _"IncreaseFontSize"_
|  _"-"_
:  _"Control"_
:[
:  _"DecreaseFontSize"_
|  _"NumpadSubtract"_
:  _"Control"_
:[
:  _"DecreaseFontSize"_

## Windows only

[[ *key*
:[ *mods*
:[ *mode*
:[ *action*
|  _"Enter"_
:  _"Alt"_
:[
:  _"ToggleFullscreen"_

## macOS only

[[ *key*
:[ *mods*
:[ *mode*
:[ *action* / *chars*
|  _"k"_
:  _"Command"_
:  _"~Vi|~Search"_
:  *chars*: _"\\u000c"_
|  _"k"_
:  _"Command"_
:  _"~Vi|~Search"_
:  _"ClearHistory"_
|  _"0"_
:  _"Command"_
:[
:  _"ResetFontSize"_
|  _"="_
:  _"Command"_
:[
:  _"IncreaseFontSize"_
|  _"+"_
:  _"Command"_
:[
:  _"IncreaseFontSize"_
|  _"NumpadAdd"_
:  _"Command"_
:[
:  _"IncreaseFontSize"_
|  _"-"_
:  _"Command"_
:[
:  _"DecreaseFontSize"_
|  _"NumpadSubtract"_
:  _"Command"_
:[
:  _"DecreaseFontSize"_
|  _"v"_
:  _"Command"_
:[
:  _"Paste"_
|  _"c"_
:  _"Command"_
:[
:  _"Copy"_
|  _"c"_
:  _"Command"_
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"h"_
:  _"Command"_
:[
:  _"Hide"_
|  _"h"_
:  _"Command|Alt"_
:[
:  _"HideOtherApplications"_
|  _"m"_
:  _"Command"_
:[
:  _"Minimize"_
|  _"q"_
:  _"Command"_
:[
:  _"Quit"_
|  _"w"_
:  _"Command"_
:[
:  _"Quit"_
|  _"n"_
:  _"Command"_
:[
:  _"CreateNewWindow"_
|  _"f"_
:  _"Command|Control"_
:[
:  _"ToggleFullscreen"_
|  _"f"_
:  _"Command"_
:  _"~Search"_
:  _"SearchForward"_
|  _"b"_
:  _"Command"_
:  _"~Search"_
:  _"SearchBackward"_

# SEE ALSO

*alacritty*(1), *alacritty-msg*(1), *alacritty*(5)

See the alacritty github repository at https://github.com/alacritty/alacritty
for the full documentation.

# BUGS

Found a bug? Please report it at https://github.com/alacritty/alacritty/issues.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
