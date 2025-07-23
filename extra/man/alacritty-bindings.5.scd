ALACRITTY-BINDINGS(5)

# NAME

alacritty-bindings - Default configuration file bindings.

# SYNOPSIS

This page documents all key and mouse bindings for the default Alacritty
configuration. See *alacritty*(5) for full configuration format documentation.

# MOUSE BINDINGS

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

# KEY BINDINGS

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
|  _"L"_
:  _"Control"_
:[
:  _"ClearLogNotice"_
|  _"Paste"_
:[
:  _"Vi|Search"_
:  _"Paste"_
|  _"L"_
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
|  _"I"_
:[
:  _"Vi|~Search"_
:  _"ToggleViMode"_
|  _"I"_
:[
:  _"Vi|~Search"_
:  _"ScrollToBottom"_
|  _"C"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ToggleViMode"_
|  _"Y"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollLineUp"_
|  _"E"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollLineDown"_
|  _"G"_
:[
:  _"Vi|~Search"_
:  _"ScrollToTop"_
|  _"G"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"ScrollToBottom"_
|  _"B"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollPageUp"_
|  _"F"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollPageDown"_
|  _"U"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollHalfPageUp"_
|  _"D"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ScrollHalfPageDown"_
|  _"Y"_
:[
:  _"Vi|~Search"_
:  _"Copy"_
|  _"Y"_
:[
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"Copy"_
:[
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"Y"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"ToggleNormalSelection"_
|  _"Y"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Last"_
|  _"Y"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Copy"_
|  _"Y"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"V"_
:[
:  _"Vi|~Search"_
:  _"ToggleNormalSelection"_
|  _"V"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"ToggleLineSelection"_
|  _"V"_
:  _"Control"_
:  _"Vi|~Search"_
:  _"ToggleBlockSelection"_
|  _"V"_
:  _"Alt"_
:  _"Vi|~Search"_
:  _"ToggleSemanticSelection"_
|  _"Enter"_
:[
:  _"Vi|~Search"_
:  _"Open"_
|  _"Z"_
:[
:  _"Vi|~Search"_
:  _"CenterAroundViCursor"_
|  _"F"_
:[
:  _"Vi|~Search"_
:  _"InlineSearchForward"_
|  _"F"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"InlineSearchBackward"_
|  _"T"_
:[
:  _"Vi|~Search"_
:  _"InlineSearchForwardShort"_
|  _"T"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"InlineSearchBackwardShort"_
|  _";"_
:[
:  _"Vi|~Search"_
:  _"InlineSearchNext"_
|  _","_
:[
:  _"Vi|~Search"_
:  _"InlineSearchPrevious"_
|  _"\*"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"SemanticSearchForward"_
|  _"#"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"SemanticSearchBackward"_
|  _"K"_
:[
:  _"Vi|~Search"_
:  _"Up"_
|  _"J"_
:[
:  _"Vi|~Search"_
:  _"Down"_
|  _"H"_
:[
:  _"Vi|~Search"_
:  _"Left"_
|  _"L"_
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
|  _"$"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Last"_
|  _"Home"_
:[
:  _"Vi|~Search"_
:  _"First"_
|  _"End"_
:[
:  _"Vi|~Search"_
:  _"Last"_
|  _"^"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"FirstOccupied"_
|  _"H"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"High"_
|  _"M"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Middle"_
|  _"L"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Low"_
|  _"B"_
:[
:  _"Vi|~Search"_
:  _"SemanticLeft"_
|  _"W"_
:[
:  _"Vi|~Search"_
:  _"SemanticRight"_
|  _"E"_
:[
:  _"Vi|~Search"_
:  _"SemanticRightEnd"_
|  _"B"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"WordLeft"_
|  _"W"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"WordRight"_
|  _"E"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"WordRightEnd"_
|  _"%"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"Bracket"_
|  _"{"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"ParagraphUp"_
|  _"}"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"ParagraphDown"_
|  _"/"_
:[
:  _"Vi|~Search"_
:  _"SearchForward"_
|  _"?"_
:  _"Shift"_
:  _"Vi|~Search"_
:  _"SearchBackward"_
|  _"N"_
:[
:  _"Vi|~Search"_
:  _"SearchNext"_
|  _"N"_
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
|  _"C"_
:  _"Control"_
:  _"Search"_
:  _"SearchCancel"_
|  _"U"_
:  _"Control"_
:  _"Search"_
:  _"SearchClear"_
|  _"W"_
:  _"Control"_
:  _"Search"_
:  _"SearchDeleteWord"_
|  _"P"_
:  _"Control"_
:  _"Search"_
:  _"SearchHistoryPrevious"_
|  _"N"_
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
|  _"V"_
:  _"Control|Shift"_
:  _"~Vi"_
:  _"Paste"_
|  _"V"_
:  _"Control|Shift"_
:  _"Vi|Search"_
:  _"Paste"_
|  _"C"_
:  _"Control|Shift"_
:[
:  _"Copy"_
|  _"F"_
:  _"Control|Shift"_
:  _"~Search"_
:  _"SearchForward"_
|  _"B"_
:  _"Control|Shift"_
:  _"~Search"_
:  _"SearchBackward"_
|  _"C"_
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
|  _"K"_
:  _"Command"_
:  _"~Vi|~Search"_
:  *chars*: _"\\u000c"_
|  _"K"_
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
|  _"V"_
:  _"Command"_
:  _"~Vi"_
:  _"Paste"_
|  _"V"_
:  _"Command"_
:  _"Vi|Search"_
:  _"Paste"_
|  _"C"_
:  _"Command"_
:[
:  _"Copy"_
|  _"C"_
:  _"Command"_
:  _"Vi|~Search"_
:  _"ClearSelection"_
|  _"H"_
:  _"Command"_
:[
:  _"Hide"_
|  _"H"_
:  _"Command|Alt"_
:[
:  _"HideOtherApplications"_
|  _"M"_
:  _"Command"_
:[
:  _"Minimize"_
|  _"Q"_
:  _"Command"_
:[
:  _"Quit"_
|  _"W"_
:  _"Command"_
:[
:  _"Quit"_
|  _"N"_
:  _"Command"_
:[
:  _"CreateNewWindow"_
|  _"T"_
:  _"Command"_
:[
:  _"CreateNewTab"_
|  _"F"_
:  _"Command|Control"_
:[
:  _"ToggleFullscreen"_
|  _"F"_
:  _"Command"_
:  _"~Search"_
:  _"SearchForward"_
|  _"B"_
:  _"Command"_
:  _"~Search"_
:  _"SearchBackward"_
|  _"]"_
:  _"Command|Shift"_
:[
:  _"SelectNextTab"_
|  _"["_
:  _"Command|Shift"_
:[
:  _"SelectPreviousTab"_
|  _"Tab"_
:  _"Command"_
:[
:  _"SelectNextTab"_
|  _"Tab"_
:  _"Command|Shift"_
:[
:  _"SelectPreviousTab"_
|  _"1"_
:  _"Command"_
:[
:  _"SelectTab1"_
|  _"2"_
:  _"Command"_
:[
:  _"SelectTab2"_
|  _"3"_
:  _"Command"_
:[
:  _"SelectTab3"_
|  _"4"_
:  _"Command"_
:[
:  _"SelectTab4"_
|  _"5"_
:  _"Command"_
:[
:  _"SelectTab5"_
|  _"6"_
:  _"Command"_
:[
:  _"SelectTab6"_
|  _"7"_
:  _"Command"_
:[
:  _"SelectTab7"_
|  _"8"_
:  _"Command"_
:[
:  _"SelectTab8"_
|  _"9"_
:  _"Command"_
:[
:  _"SelectLastTab"_

# SEE ALSO

*alacritty*(1), *alacritty-msg*(1), *alacritty*(5)

# BUGS

Found a bug? Please report it at _https://github.com/alacritty/alacritty/issues_.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
