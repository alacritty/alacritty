# Alacritty-visor
> ⚠ **Disclaimer** Im not familiar to Rust and advanced concepts used in this program. Dont take it as an example for good practices on Rust. If you think that i could made something to make this more readable, developer friendly or performant, You can always create a PR or an Issue and let me know what could i've done better ⚠

![Demo](demo.gif)

Fork of [Alacritty](https://github.com/alacritty/alacritty) modified to:
- Register a global key shortcut to call terminal and hide it
- AutoHide (opt-in) window on focus lost
- Run terminal with none decorators (with decorators but without buttons in Windows due to a bug in `wininit`)
- Always spawn on center of screen with a predefined size

Currently tested and working on a Windows 10 machine

## Credits
- [Original Readme of Alacritty](./ORIGINAL_README.md)
- <a href="https://www.flaticon.com/free-icons/terminal" title="terminal icons">Terminal icons created by Royyan Wijaya - Flaticon</a>
