# Features

This document gives an overview over Alacritty's features beyond its terminal
emulation capabilities. To get a list with supported control sequences take a
look at [Alacritty's escape sequence support](./escape_support.md).

## Vi Mode

The vi mode allows moving around Alacritty's viewport and scrollback using the
keyboard. It also serves as a jump-off point for other features like search and
opening URLs with the keyboard. By default you can launch it using
<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Space</kbd>.

### Motion

The cursor motions are setup by default to mimic vi, however they are fully
configurable. If you don't like vi's bindings, take a look at the [configuration
file] to change the various movements.

### Selection

One useful feature of vi mode is the ability to make selections and copy text to
the clipboard. By default you can start a selection using <kbd>v</kbd> and copy
it using <kbd>y</kbd>. All selection modes that are available with the mouse can
be accessed from vi mode, including the semantic (<kbd>Alt</kbd> <kbd>v</kbd>),
line (<kbd>Shift</kbd> <kbd>v</kbd>) and block selection (<kbd>Ctrl</kbd>
<kbd>v</kbd>). You can also toggle between them while the selection is still
active.

### Opening URLs

While in vi mode you can open URLs using the <kbd>Enter</kbd> key. If some text
is recognized as a URL, it will be underlined once you move the vi cursor above
it. The program used to open these URLs can be changed in the [configuration
file].

## Search

Search allows you to find anything in Alacritty's scrollback buffer. You can
search forward using <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>f</kbd> and
backward using <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>b</kbd>.

### Vi Search

In vi mode the search is bound to <kbd>/</kbd> for forward and <kbd>?</kbd> for
backward search. This allows you to move around quickly and help with selecting
content. The `SearchStart` and `SearchEnd` keybinding actions can be bound if
you're looking for a way to jump to the start or the end of a match.

### Normal Search

During normal search you don't have the opportunity to move around freely, but
you can still jump between matches using <kbd>Enter</kbd> and <kbd>Shift</kbd>
<kbd>Enter</kbd>. After leaving search with <kbd>Escape</kbd> your active match
stays selected, allowing you to easily copy it.

## Selection expansion

After making a selection, you can use the right mouse button to expand it.
Double-clicking will expand the selection semantically, while triple-clicking
will perform line selection. If you hold <kbd>Ctrl</kbd> while expanding the
selection, it will switch to the block selection mode.

## Opening URLs with the mouse

You can open URLs with your mouse by clicking on them. The modifiers required to
be held and program which should open the URL can be setup in the configuration
file. If an application captures your mouse clicks, which is indicated by a
change in mouse cursor shape, you're required to hold <kbd>Shift</kbd> to bypass
that.

[configuration file]: ../alacritty.yml
