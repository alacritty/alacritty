For bug reports, the following information can help speed up the process.

Please describe the bug that you have found and what you would expect to happen instead.

# System

|                   |                                   |
|-------------------|-----------------------------------|
| Operating System  | [Linux/BSD/macOS/Windows]         |
| Alacritty Version | [`alacritty --version` output]    |
| Display Server    | [X11/Wayland]   (only on Linux)   |
| Window Manager    | [i3/xfwm/...]   (only on Linux)   |
| Compositor        | [compton/...]   (only on Linux)   |
| PTY Backend       | [WinPTY/ConPTY] (only on Windows) |

# Logs

Based on the issue at hand, some logs might be relevant:

| Command                    | Issues                                              |
|----------------------------|-----------------------------------------------------|
| STDOUT, STDERR             | Crashes                                             |
| `alacritty -vv`            | DPI, font size, resize, terminal grid and cell size |
| `alacritty --print-events` | Problems with keyboard and keybindings              |

