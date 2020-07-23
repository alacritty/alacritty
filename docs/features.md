# Features

This document gives an overview over Alacritty's features beyond its terminal
emulation capabilities. To get a list with supported control sequences take a
look at [Alacritty's escape sequence support](./escape_support.md).

## Vi Mode

The vi mode allows moving around Alacritty's viewport and scrollback using the
keyboard. It also serves as a jump-off point for other features like search and
opening URLs with the keyboard. By default you can launch it using
<kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>Space</kbd>.

### Motion

The cursor motions are setup by default to mimic vi, however they are fully
configurable. If you don't like vi's bindings, take a look at the [configuration
file] to change the various movements.

### Selection

One useful feature of vi mode is the ability to make selections and copy text to
the clipboard. By default you can start a selection using <kbd>v</kbd> and copy
it using <kbd>y<kbd>. All selection modes that are available with the mouse can
be accessed from vi mode, including the semantic (<kbd>Alt</kbd><kbd>v</kbd>),
line (<kbd>Shift</kbd><kbd>v</kbd>) and block selection
(<kbd>Ctrl</kbd><kbd>v</kbd>). You can also toggle between them while the
selection is still active.

### Opening URLs

While in vi mode you can open URLs using the <kbd>Enter</kbd> key. If some text
is recognized as a URL, it will be underlined once you move the vi cursor above
it. The program used to open these URLs can be changed in the [configuration
file].

## Search

Search allows you to find anything in Alacritty's scrollback buffer, it can be
launched either directly (<kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>f</kbd>), or from
vi mode (<kbd>/</kbd>).

### Vi Search

When using search while the vi mode is active, it can be used to quickly move
around the scrollback buffer. The bindings <kbd>Ctrl</kbd><kbd>n</kbd> and
<kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>n</kbd> allow you to navigate within
matches, which make them useful tools for selecting your matches.

### Normal Search

During normal search you don't have the opportunity to move around freely, but
you can still jump between matches using <kbd>Enter</kbd> and
<kbd>Shift</kbd><kbd>Enter</kbd>. After leaving search with <kbd>Escape</kbd>
your active match stays selected, allowing you to easily copy it.

## Selection expansion

After making a selection, you can use the right mouse button to expand it.
Double-clicking will expand the selection semantically, while triple-clicking
will perform line selection. If you hold <kbd>Ctrl</kbd> while expanding the
selection, it will switch to the block selection mode.

[configuration file]: ../alacritty.yml
