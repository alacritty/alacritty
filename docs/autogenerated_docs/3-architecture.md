# Architecture

<details>
<summary>Relevant source files</summary>

The following files were used as context for generating this wiki page:

- [CHANGELOG.md](https://github.com/alacritty/alacritty/blob/a0c4dfe9/CHANGELOG.md)
- [Cargo.lock](https://github.com/alacritty/alacritty/blob/a0c4dfe9/Cargo.lock)
- [Cargo.toml](https://github.com/alacritty/alacritty/blob/a0c4dfe9/Cargo.toml)
- [alacritty/Cargo.toml](https://github.com/alacritty/alacritty/blob/a0c4dfe9/alacritty/Cargo.toml)
- [alacritty_terminal/Cargo.toml](https://github.com/alacritty/alacritty/blob/a0c4dfe9/alacritty_terminal/Cargo.toml)

</details>



This document provides an overview of Alacritty's system architecture, explaining how the different components work together to create a fast, cross-platform terminal emulator. For information about specific features, see [Features](#4).

## High-Level Architecture Overview

Alacritty follows a modular design with clear separation of concerns, organized around several key subsystems that work together to provide terminal emulation functionality.

### System Components Diagram

```mermaid
graph TD
    A["User"] --> |"Input"| B["Window & Event System"]
    B --> |"Events"| C["Terminal Emulator Core"]
    C --> |"Content"| D["Rendering System"]
    D --> |"Output"| A
    
    C <--> |"I/O"| E["PTY System"]
    E <--> |"I/O"| F["Shell Process"]
    
    G["Configuration System"] --> B
    G --> C
    G --> D
    
    H["CLI/IPC"] --> G
    H --> B
    
    
    subgraph "User Interface Layer"
        B
        D
        I
    end
    
    subgraph "Core Terminal Logic"
        C
        E
    end
    
    subgraph "External Process"
        F
    end
    
    subgraph "Configuration & Control"
        G
        H
    end
```

Sources:
- alacritty/Cargo.toml
- alacritty_terminal/Cargo.toml

## Project Structure

Alacritty is built using a multi-crate architecture, with clear separation between core terminal functionality and the user interface.

### Crate Organization

```mermaid
graph TD
    A["alacritty"] --> |"depends on"| AT["alacritty_terminal"]
    A --> |"depends on"| AC["alacritty_config"]
    A --> |"depends on"| ACD["alacritty_config_derive"]
    
    AT --> |"uses"| VTE["vte (escape sequence parser)"]
    AT --> |"uses"| PTY["platform-specific PTY APIs"]
    
    A --> |"uses"| WINIT["winit (window/events)"]
    A --> |"uses"| GLUTIN["glutin (OpenGL context)"]
    A --> |"uses"| CF["crossfont (font rendering)"]
    
    subgraph "Main Application"
        A
    end
    
    subgraph "Core Components"
        AT["alacritty_terminal<br>(terminal emulation, PTY)"]
        AC["alacritty_config<br>(configuration handling)"]
        ACD["alacritty_config_derive<br>(config macros)"]
    end
    
    subgraph "External Dependencies"
        VTE
        PTY
        WINIT
        GLUTIN
        CF
    end
```

Sources:
- Cargo.toml
- alacritty/Cargo.toml
- alacritty_terminal/Cargo.toml
- alacritty_config/Cargo.toml

The key components in the architecture are:

1. **alacritty**: The main application crate containing UI, rendering, and event handling
2. **alacritty_terminal**: Core terminal emulation functionality, PTY handling, and terminal state
3. **alacritty_config**: Configuration parsing and validation
4. **alacritty_config_derive**: Procedural macros for configuration

## Core Subsystems

### Event Processing System

The event system is the heart of Alacritty's interactive capabilities, handling user input and system events.

```mermaid
flowchart TD
    A["User Input"] --> B["Winit EventLoop"]
    B --> C["EventProcessor"]
    B --> D["Window Events"]
    B --> E["Custom Events"]
    
    C --> F{"Event Type?"}
    F -->|"Input (Key/Mouse)"| G["InputHandler"]
    F -->|"Resize"| H["ResizeHandler"]
    F -->|"Redraw"| I["Renderer Draw"]
    F -->|"Config Change"| J["Config Reload"]
    
    G --> L["ActionContext"]
    L --> M{Match Binding}
    M -->|"Terminal Action"| N["Terminal Update"]
    M -->|"Vi Mode Action"| O["Vi Mode Handler"]
    M -->|"Search Action"| P["Search Handler"]
    M -->|"Copy/Paste"| Q["Clipboard"]
    
    N --> R["PTY Write"]
```

Sources:
- alacritty/src/event.rs
- alacritty/src/input.rs

The event processing flow:
1. User input (keyboard/mouse) is captured by the `winit` event loop
2. Events are processed by the `EventProcessor`
3. Input events are matched against key/mouse bindings in `ActionContext`
4. Matched actions are dispatched to appropriate handlers
5. Terminal updates are sent to the PTY

### Terminal Core and Grid

The terminal core handles state management and communication with the shell.

```mermaid
graph TD
    A["PTY Read/Write"] <--> B["Shell Process"]
    A --> |"Input to"| C["Terminal"]
    C --> |"Updates"| D["Grid"]
    D --> |"Contains"| E["Cells"]
    E --> |"Has"| F["Character"]
    E --> |"Has"| G["Attributes"]
    G --> |"Include"| H["Colors"]
    G --> |"Include"| I["Flags"]
    G --> |"Include"| J["Hyperlinks"]
    
    D --> |"Manages"| K["Cursor"]
    D --> |"Manages"| L["Selection"]
    D --> |"Manages"| M["Scrollback"]
    
    C --> |"Handles"| O["Escape Sequences"]
    O --> |"Include"| P["CSI"]
    O --> |"Include"| Q["OSC"]
    O --> |"Include"| R["DCS"]
```

Sources:
- alacritty_terminal/src/term/mod.rs
- alacritty_terminal/src/grid/mod.rs
- alacritty_terminal/src/ansi.rs

The terminal core:
1. Maintains a grid of cells representing the terminal display
2. Each cell contains a character and attributes (color, style, etc.)
3. Processes escape sequences from the PTY using the `vte` parser
4. Updates the grid based on parsed commands
5. Manages cursor, selection, and scrollback buffer

### Rendering Pipeline

The rendering system converts terminal state into visible output.

```mermaid
graph LR
    A["Terminal State"] --> B["Display"]
    B --> |"Tracked by"| C["DamageTracker"]
    C --> D["Renderer"]
    
    D --> E["TextRenderer"]
    D --> F["RectangleRenderer"]
    
    E --> |"Uses"| G{"OpenGL Version"}
    G -->|"GLSL3"| H["Glsl3Renderer"]
    G -->|"GLES2"| I["Gles2Renderer"]
    
    B --> |"Contains"| J["Cells"]
    B --> |"Contains"| K["Cursor"]
    B --> |"Contains"| L["Selection"]
    B --> |"Contains"| M["Search Highlights"]
```

Sources:
- alacritty/src/display/mod.rs
- alacritty/src/renderer/mod.rs

The rendering pipeline:
1. Terminal state is managed by the `Display` component
2. Changes are tracked by the `DamageTracker` to optimize rendering
3. The `Renderer` uses OpenGL through `glutin` to draw content
4. Text is rendered using the `TextRenderer`
5. Backgrounds, cursor, and selections are drawn using the `RectangleRenderer`

### PTY System

The Pseudoterminal (PTY) system connects Alacritty to the shell process.

```mermaid
graph TD
    A["PtyManager"] --> |"Creates"| B["Pty"]
    A --> |"Spawns"| C["ShellProcess"]
    
    B --> D["ReadPipe"]
    B --> E["WritePipe"]
    
    D --> |"Data"| F["Terminal"]
    F --> |"Input"| E
    
    subgraph "Platform-Specific Implementations"
        G["Unix (pty, fork)"]
        H["Windows (ConPTY)"]
    end
```

Sources:
- alacritty_terminal/src/tty/mod.rs
- alacritty_terminal/src/tty/unix.rs
- alacritty_terminal/src/tty/windows/mod.rs

The PTY system:
1. Creates a pseudoterminal appropriate for the platform
2. Spawns a shell process connected to the PTY
3. Provides read/write pipes for communicating with the shell
4. Has platform-specific implementations for Unix and Windows

### Configuration System

The configuration system provides settings for all parts of Alacritty.

```mermaid
graph LR
    A["Configuration Files"] --> |"Loads"| B["ConfigManager"]
    C["CLI Arguments"] --> |"Overrides"| B
    D["IPC Messages"] --> |"Updates at runtime"| B
    
    B --> |"Provides"| E["Window Settings"]
    B --> |"Provides"| F["Terminal Settings"]
    B --> |"Provides"| G["Rendering Settings"]
    B --> |"Provides"| H["Key/Mouse Bindings"]
    
    subgraph "Config Sources"
        A
        C
        D
    end
    
    subgraph "Configured Components"
        E
        F
        G
        H
    end
```

Sources:
- alacritty/src/config/mod.rs
- alacritty_config/src/config.rs

The configuration system:
1. Loads and parses TOML configuration files
2. Applies command-line argument overrides
3. Supports live reloading of configuration
4. Provides settings to all subsystems
5. Handles IPC for runtime configuration changes

## Program Execution Flow

This sequence diagram illustrates the overall execution flow in Alacritty:

```mermaid
sequenceDiagram
    participant User
    participant EventLoop
    participant Terminal
    participant PTY
    participant Shell
    participant Renderer
    
    User->>EventLoop: Start application
    EventLoop->>Terminal: Initialize terminal
    Terminal->>PTY: Create PTY
    PTY->>Shell: Spawn shell process
    
    loop Main Event Loop
        User->>EventLoop: Input (keyboard/mouse)
        EventLoop->>Terminal: Process input
        Terminal->>PTY: Write to PTY
        PTY->>Shell: Forward input
        Shell->>PTY: Generate output
        PTY->>Terminal: Read from PTY
        Terminal->>Terminal: Update terminal state
        EventLoop->>Renderer: Render frame
        Renderer->>User: Display output
    end
```

Sources:
- alacritty/src/main.rs
- alacritty/src/event_loop.rs

The program execution flow:
1. Application starts and initializes terminal, window, and renderer
2. A PTY is created and shell process spawned
3. The main event loop processes user input and system events
4. Input is forwarded to the shell process via the PTY
5. Shell output is read from the PTY and updates the terminal state
6. The renderer draws the terminal content to the screen
7. This cycle continues until the application is closed

## Cross-Cutting Features

Alacritty includes several key features that span multiple subsystems:

```mermaid
graph TD
    A["Vi Mode"] --> B["Terminal State"]
    A --> C["Input Processing"]
    A --> D["Rendering"]
    
    E["Search Functionality"] --> B
    E --> C
    E --> D
    
    F["Clipboard Integration"] --> C
    F --> H["Platform Clipboard APIs"]
    
    I --> C
    I --> D
    I --> J["Platform URL Handlers"]
```

Sources:
- alacritty/src/input/mod.rs
- alacritty_terminal/src/vi_mode.rs
- alacritty/src/display/hint.rs

These cross-cutting features are implemented across multiple subsystems:
1. **Vi Mode**: Allows Vim-like navigation and selection within the terminal
2. **Search**: Text search functionality with highlighting
3. **Clipboard Integration**: Copy/paste with system clipboard
4. **URL Hints**: Detection and interaction with URLs in terminal content

## Platform-Specific Implementations

Alacritty provides consistent functionality across platforms through abstraction:

```mermaid
graph TD
    A["Alacritty Core"] --> B["Platform Abstractions"]
    
    B --> C["Window System"]
    B --> D["PTY Implementation"]
    B --> E["Clipboard"]
    B --> F["Font Rendering"]
    
    C --> C1["X11 (Linux/BSD)"]
    C --> C2["Wayland (Linux)"]
    C --> C3["Cocoa (macOS)"]
    C --> C4["Win32 (Windows)"]
    
    D --> D1["Unix PTY"]
    D --> D2["Windows ConPTY"]
    
    E --> E1["X11 Clipboard"]
    E --> E2["Wayland Clipboard"]
    E --> E3["macOS Pasteboard"]
    E --> E4["Windows Clipboard"]
    
    F --> F1["FreeType (Linux/BSD)"]
    F --> F2["Core Text (macOS)"]
    F --> F3["DirectWrite (Windows)"]
```

Sources:
- alacritty/src/window.rs
- alacritty_terminal/src/tty/mod.rs
- alacritty/src/clipboard.rs

Platform-specific implementations ensure Alacritty works consistently across:
1. **Window Systems**: X11, Wayland, Cocoa (macOS), and Win32 (Windows)
2. **PTY**: Unix (pty/fork) and Windows (ConPTY)
3. **Clipboard**: Platform-specific clipboard APIs
4. **Font Rendering**: FreeType, Core Text, and DirectWrite

## Related Pages

For more detailed information about the architecture components, refer to:
- [Project Structure](#3.1)
- [Terminal Core](#3.2)
- [Event System](#3.3)
- [Rendering Pipeline](#3.4)
- [PTY Interaction](#3.5)

Sources:
- Cargo.toml
- alacritty/Cargo.toml