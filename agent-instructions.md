# Alacritty Project Setup Instructions

This document provides comprehensive instructions for setting up the Alacritty terminal emulator project from scratch. Alacritty is a fast, cross-platform, OpenGL terminal emulator written in Rust.

## Three-Phase Execution Model

The installation and setup process must be performed in three distinct phases. **Do NOT proceed to the next phase until the user explicitly approves.**

---

## Phase 1: Research

### Objective

Collect all necessary information about the installation and setup requirements for Alacritty.

### Information Gathering Tasks

1. **Project Structure Analysis**
   - Examine the workspace structure (Cargo.toml at root)
   - Identify all workspace members:
     - `alacritty` (main application)
     - `alacritty_terminal` (terminal library)
     - `alacritty_config` (configuration)
     - `alacritty_config_derive` (derive macros)
   - Review documentation files:
     - `README.md` - Project overview
     - `INSTALL.md` - Detailed installation instructions
     - `CONTRIBUTING.md` - Development guidelines
     - `CHANGELOG.md` - Version history

2. **System Requirements Detection**
   - Determine the operating system (Linux, macOS, BSD, or Windows)
   - Check Rust compiler requirements:
     - Minimum Rust version: **1.85.0** (specified in `Cargo.toml` workspace package)
     - Verify if rustup is installed
   - Check OpenGL requirements:
     - Minimum: OpenGL ES 2.0
     - Windows: ConPTY support (Windows 10 version 1809 or higher)

3. **Dependencies Analysis**

   **By Platform:**

   **Linux/Unix:**
   - Platform-specific package managers (apt, dnf, pacman, zypper, etc.)
   - Required packages:
     - `cmake`
     - `pkg-config`
     - `freetype-dev` / `freetype-devel` / `libfreetype6-dev`
     - `fontconfig-dev` / `fontconfig-devel` / `libfontconfig1-dev`
     - `libxcb-dev` / `libxcb-devel` / `libxcb-xfixes0-dev`
     - `libxkbcommon-dev` / `libxkbcommon-devel`
     - `python3`
   - Optional for Wayland with Nvidia GPU: `libegl1-mesa-dev` (Ubuntu)

   **macOS:**
   - Xcode command line tools (check via `xcode-select --version`)
   - `scdoc` (for man pages)

   **Windows:**
   - MSVC toolchain (`x86_64-pc-windows-msvc`)
   - Clang 3.9 or greater
   - Visual Studio Build Tools

4. **Build Process Understanding**
   - Default build command: `cargo build --release`
   - Build features:
     - Default: `wayland` and `x11` (Linux/BSD)
     - Wayland-only: `--no-default-features --features=wayland`
     - X11-only: `--no-default-features --features=x11`
   - macOS-specific:
     - Use `make app` to create `.app` bundle
     - Universal binary: `make app-universal` (requires both x86_64 and aarch64 targets)
   - Output location: `target/release/alacritty` (binary) or `target/release/osx/Alacritty.app` (macOS app)

5. **Post-Build Installation Components**
   
   **Optional (but recommended) components:**
   - **Terminfo**: Install terminal info for proper operation
     - Source: `extra/alacritty.info`
     - Command: `sudo tic -xe alacritty,alacritty-direct extra/alacritty.info`
   
   - **Desktop Entry** (Linux/BSD only):
     - Source: `extra/linux/Alacritty.desktop`
     - Logo: `extra/logo/alacritty-term.svg`
     - Destination: `/usr/share/applications/`
   
   - **Manual Pages** (requires `scdoc` and `gzip`):
     - Sources in `extra/man/`:
       - `alacritty.1.scd` → man1
       - `alacritty-msg.1.scd` → man1
       - `alacritty.5.scd` → man5
       - `alacritty-bindings.5.scd` → man5
   
   - **Shell Completions**:
     - Zsh: `extra/completions/_alacritty`
     - Bash: `extra/completions/alacritty.bash`
     - Fish: `extra/completions/alacacritty.fish`

6. **Configuration Setup**
   - Configuration file locations (in order of priority):
     - `$XDG_CONFIG_HOME/alacritty/alacritty.toml`
     - `$XDG_CONFIG_HOME/alacritty.toml`
     - `$HOME/.config/alacritty/alacritty.toml`
     - `$HOME/.alacritty.toml`
     - `/etc/alacritty/alacritty.toml`
   - Windows: `%APPDATA%\alacritty\alacritty.toml`
   - Note: Alacritty does NOT create the config file automatically

7. **Development Tools (for contribution)**
   - Testing: `cargo test`
   - Formatting: `cargo +nightly fmt` (as per CI)
   - Linting: `cargo clippy --all-targets`
   - Style guidelines:
     - `rustfmt.toml` configuration file present
     - Code must conform to rustfmt guidelines
     - Comments should be fully punctuated with trailing periods

8. **CI/CD Configuration**
   - Review `.github/workflows/ci.yml` for automated testing
   - Review `.github/workflows/release.yml` for release process
   - Review `.builds/` for platform-specific CI builds

### Ambiguity Resolution

Identify any unclear, ambiguous, or missing information:

1. **Installation Path Preferences**
   - Should the binary be installed to `/usr/local/bin/` or another location?
   - Should man pages go to `/usr/local/share/man/` or `/usr/share/man/`?

2. **Feature Selection**
   - Build with default features (both wayland and x11)?
   - Force specific backend (wayland-only or x11-only)?

3. **Post-Build Components**
   - Which optional components should be installed?
   - Should terminfo, desktop entry, man pages, and shell completions be installed?

4. **Configuration**
   - Should a sample configuration file be created?
   - Should default configuration be copied from a template?

5. **macOS-Specific**
   - Should a universal binary be built (both x86_64 and ARM)?
   - Should the .app bundle be copied to `/Applications/`?

6. **Development vs. Production**
   - Is this setup for development (contribution) or production use?
   - If development, should development tools be installed?

### Research Documentation

Create a `research.md` file containing:

1. **Summary of Findings**
   - Detected operating system
   - Current Rust toolchain status
   - Available package manager
   - System capabilities

2. **Requirements List**
   - Required dependencies (with versions where applicable)
   - Optional dependencies
   - System prerequisites

3. **Ambiguities and Questions**
   - List all questions requiring user clarification
   - Identify decisions that need user input

4. **Recommendations**
   - Suggested installation approach
   - Recommended optional components
   - Platform-specific considerations

### User Verification

Before proceeding to Phase 2:

1. Present a concise summary of the most important findings:
   - OS and platform detection
   - Rust toolchain status
   - Key dependencies required
   - Build method appropriate for the platform

2. Ask the user to review the detailed `research.md` file

3. Request clarification on ambiguous points:
   - Installation path preferences
   - Optional components to install
   - Development vs. production setup

4. **Wait for explicit user approval before proceeding to Phase 2**

---

## Phase 2: Plan

### Objective

Create a comprehensive, detailed plan for installation and setup based on the approved research.

### Task Breakdown

Based on approved research, create a detailed, numbered task list:

1. **Prerequisites Installation**
   - Install Rust toolchain (if not present)
   - Verify rust-version compatibility (1.85.0+)
   - Install system dependencies for detected OS
   - Install additional build tools (cmake, pkg-config, etc.)

2. **Source Code Acquisition**
   - Clone repository from GitHub: `https://github.com/alacritty/alacritty.git`
   - Verify repository integrity
   - Switch to appropriate branch if needed

3. **Build Configuration**
   - Set Rust toolchain version: `rustup override set stable`
   - Update Rust: `rustup update stable`
   - Configure build features (if non-default)
   - Set environment variables (e.g., `MACOSX_DEPLOYMENT_TARGET` for macOS)

4. **Build Execution**
   - Execute build command appropriate for platform:
     - Linux/Windows/BSD: `cargo build --release`
     - macOS: `make app` or `make app-universal`
   - Monitor build process for errors
   - Verify binary generation

5. **Binary Installation**
   - Copy binary to appropriate location:
     - Linux/BSD: `/usr/local/bin/alacritty` or user-local bin
     - macOS: Copy `Alacritty.app` to `/Applications/`
     - Windows: Add to PATH or copy to desired location
   - Set executable permissions (Unix): `chmod +x`
   - Verify binary is accessible

6. **Optional: Terminfo Installation**
   - Check if terminfo already installed: `infocmp alacritty`
   - Install if not present: `sudo tic -xe alacritty,alacritty-direct extra/alacritty.info`
   - Verify installation

7. **Optional: Desktop Entry (Linux/BSD only)**
   - Install icon: `sudo cp extra/logo/alacritty-term.svg /usr/share/pixmaps/Alacritty.svg`
   - Install desktop file: `sudo desktop-file-install extra/linux/Alacritty.desktop`
   - Update desktop database: `sudo update-desktop-database`

8. **Optional: Manual Pages Installation**
   - Check for required tools: `scdoc` and `gzip`
   - Create man directories: `sudo mkdir -p /usr/local/share/man/man1 /usr/local/share/man/man5`
   - Generate and install man pages:
     ```bash
     scdoc < extra/man/alacritty.1.scd | gzip -c | sudo tee /usr/local/share/man/man1/alacritty.1.gz > /dev/null
     scdoc < extra/man/alacritty-msg.1.scd | gzip -c | sudo tee /usr/local/share/man/man1/alacritty-msg.1.gz > /dev/null
     scdoc < extra/man/alacritty.5.scd | gzip -c | sudo tee /usr/local/share/man/man5/alacritty.5.gz > /dev/null
     scdoc < extra/man/alacritty-bindings.5.scd | gzip -c | sudo tee /usr/local/share/man/man5/alacritty-bindings.5.gz > /dev/null
     ```

9. **Optional: Shell Completions Installation**

   **Zsh:**
   ```bash
   mkdir -p ${ZDOTDIR:-~}/.zsh_functions
   echo 'fpath+=${ZDOTDIR:-~}/.zsh_functions' >> ${ZDOTDIR:-~}/.zshrc
   cp extra/completions/_alacritty ${ZDOTDIR:-~}/.zsh_functions/_alacritty
   ```

   **Bash:**
   ```bash
   echo "source $(pwd)/extra/completions/alacritty.bash" >> ~/.bashrc
   # OR copy to bash_completion:
   mkdir -p ~/.bash_completion
   cp extra/completions/alacritty.bash ~/.bash_completion/alacritty
   echo "source ~/.bash_completion/alacritty" >> ~/.bashrc
   ```

   **Fish:**
   ```bash
   mkdir -p $fish_complete_path[1]
   cp extra/completions/alacritty.fish $fish_complete_path[1]/alacritty.fish
   ```

10. **Configuration Setup**
    - Determine config file location
    - Create configuration directory if needed
    - Copy sample configuration (if desired)
    - Set appropriate permissions

11. **Development Tools (if requested)**
    - Install rustfmt: `rustup component add rustfmt`
    - Install clippy: `rustup component add clippy`
    - Verify tools: `cargo fmt --check` and `cargo clippy --all-targets`

12. **Verification**
    - Run alacritty: `alacritty --version`
    - Verify binary works correctly
    - Check for any runtime errors
    - Verify optional components (terminfo, desktop entry, etc.)

### Success Criteria

For each task, define:

1. **Success Criterion**: What constitutes successful completion
2. **Verification Method**: Specific command or test to verify success
3. **Rollback Strategy**: How to undo the task if needed

Example format:

```
Task X: Install Rust toolchain
- Success Criterion: Rust 1.85.0+ is installed and accessible
- Verification Method: `rustc --version` and `cargo --version`
- Rollback Strategy: `rustup self uninstall`
```

### Risk Assessment

Identify:

1. **High-Risk Operations** (require explicit user permission):
   - System package installations with `sudo`
   - Copying files to system directories (`/usr/local/bin/`, `/usr/share/`, etc.)
   - Modifying shell configuration files (`~/.bashrc`, `~/.zshrc`, etc.)
   - Installing terminfo system-wide

2. **Medium-Risk Operations**:
   - Building from source (time-consuming, may fail)
   - Creating configuration files
   - Installing desktop entries

3. **Low-Risk Operations**:
   - Installing user-local binaries
   - Installing user-local shell completions
   - Verifying existing installations

4. **Operations That Cannot Be Easily Reversed**:
   - System package installations (may have dependencies)
   - Modifications to system directories
   - Terminfo installation (may require manual removal)

### Plan Documentation

Create a `plan.md` file containing:

1. **Task Sequence**
   - Complete numbered list of all tasks
   - Dependencies between tasks (e.g., build before install)
   - Estimated completion time for each task

2. **Success Criteria Table**
   - For each task:
     - Task number and name
     - Success criterion
     - Verification command(s)
     - Rollback strategy

3. **Permission Gates**
   - List all tasks requiring explicit user permission
   - Clearly mark high-risk operations
   - Indicate where execution should pause for approval

4. **Manual Input Summary**
   - Consolidate all questions requiring user input
   - List configuration preferences needed
   - Document platform-specific decisions

5. **Execution Order and Dependencies**
   - Critical path analysis
   - Tasks that can be run in parallel
   - Tasks that must wait for completion of previous tasks

### User Verification

Before proceeding to Phase 3:

1. Present a summary of key milestones:
   - Prerequisites installation
   - Build completion
   - Binary installation
   - Optional components installation

2. Highlight critical tasks:
   - System-level changes requiring sudo
   - Shell configuration modifications
   - Permanent system installations

3. Ask the user to review the detailed `plan.md` file

4. Highlight any high-risk operations requiring approval

5. **Wait for explicit user approval before proceeding to Phase 3**

---

## Phase 3: Execute

### Objective

Execute all planned tasks with verification and error handling.

### Pre-Execution Checklist

Before starting execution:

1. Confirm all manual inputs are available
2. Verify user has approved the plan from Phase 2
3. Ensure permission gates are understood
4. Check that all prerequisites from Phase 1 are met

### Sequential Execution

For each task in the plan:

1. **Display Task Information**
   - Show task name and number
   - Display success criterion
   - Show verification method

2. **Request Permission (if high-risk)**
   - For high-risk operations, pause and ask for explicit user permission
   - Show what will be done and why
   - Wait for user confirmation before proceeding

3. **Execute the Task**
   - Run the specified commands
   - Capture output for troubleshooting
   - Monitor for errors or warnings

4. **Verify Success**
   - Run the verification method specified in the plan
   - Check return codes and output
   - Confirm success criterion is met

5. **Report Results**
   - Inform user of task completion
   - Show verification results
   - Note any warnings or non-critical issues

6. **Proceed to Next Task**
   - Only continue to next task after current task succeeds
   - Do not skip verification steps

### Error Handling

If a task fails verification:

1. **Stop Immediately**
   - Halt execution at the failed task
   - Do not attempt to proceed with remaining tasks

2. **Document the Failure**
   - Record what task failed
   - Capture error messages and output
   - Document what was attempted

3. **Present Error to User**
   - Show the error with context
   - Explain what went wrong
   - Provide relevant error logs or output

4. **Request User Decision**
   - Ask user to choose from:
     - **Retry**: Attempt the task again with modifications
     - **Skip**: Skip this task and continue (if safe)
     - **Abort**: Stop the entire installation process
   - Wait for explicit user decision

5. **Never Proceed Without Approval**
   - Do not automatically continue after a failure
   - Do not attempt workarounds without user consent

### Completion Report

After all tasks are completed (or user decides to stop):

1. **Summary of Completed Tasks**
   - List all tasks successfully completed
   - Show final status of each task

2. **List of Skipped or Failed Tasks**
   - Document any tasks that were skipped
   - List any tasks that failed
   - Note reasons for skips/failures

3. **Overall Installation Verification**
   - Run comprehensive verification:
     - Check binary exists and is executable
     - Verify version: `alacritty --version`
     - Test basic functionality: `alacritty -e echo "Test"`
   - Verify optional components if installed

4. **Next Steps and Usage Instructions**
   - How to run Alacritty
   - Configuration file location
   - Basic configuration steps
   - Link to full documentation: `man alacritty` or https://alacritty.org/config-alacritty.html
   - Where to find help (README.md, documentation, IRC channel)

5. **Troubleshooting Tips**
   - Common issues and solutions
   - Where to find logs
   - How to report bugs (GitHub issues)

### Example Execution Flow

```
[Task 1/10] Install Rust toolchain
  Success Criterion: Rust 1.85.0+ installed and accessible
  Verification: rustc --version && cargo --version
  Rollback: rustup self uninstall
  
  Executing: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  ...
  
  Verifying: rustc --version
  Output: rustc 1.85.0 (target triple)
  ✓ Success!

[Task 2/10] Install system dependencies
  Success Criterion: All required packages installed
  Verification: Check package manager status
  Rollback: Remove installed packages
  ⚠ HIGH RISK: This requires sudo access
  Permission required: Install cmake, freetype-dev, fontconfig-dev, libxcb-dev, libxkbcommon-dev, python3?
  [User]: yes
  
  Executing: sudo apt install cmake ...
  ...
  
  Verifying: dpkg -l | grep -E 'cmake|freetype|fontconfig'
  ✓ Success!

... (continue with remaining tasks)

[Summary]
  Completed: 10/10 tasks
  Failed: 0
  Skipped: 0
  
  ✓ Alacritty installation completed successfully!
  
  Next steps:
  - Run: alacritty
  - Configure: ~/.config/alacritty/alacritty.toml
  - Documentation: man alacritty
```

---

## Additional Guidelines

### Automated Detection

Whenever possible, automatically determine values:

- **OS Detection**: Use `uname` or Rust's `std::env::consts::OS`
- **Architecture**: Use `uname -m` or `rustc --print target-list`
- **Package Manager**: Detect based on OS:
  - Debian/Ubuntu: `apt`
  - Arch Linux: `pacman`
  - Fedora/RHEL: `dnf` or `yum`
  - openSUSE: `zypper`
  - macOS: Homebrew (`brew`) or MacPorts
  - Windows: `winget`, `chocolatey`, or `scoop`
- **Shell Detection**: Check `$SHELL` environment variable
- **Config Paths**: Automatically detect XDG_CONFIG_HOME, HOME, etc.

### Permission Gates

Require explicit user permission before:

1. Any command using `sudo`
2. Modifying system directories (`/usr/local/bin`, `/usr/share`, etc.)
3. Modifying shell configuration files (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/`)
4. Installing system-wide packages
5. Creating system-wide terminfo entries

### Manual Input Consolidation

If multiple manual inputs are required, group them into a clear "Manual Input Summary" section in the plan document. After creating the plan, create a comprehensive `human_tasks.md` file that helps the user complete all remaining tasks that require manual intervention.

### Execution Flow

Format the workflow so a future agent can execute tasks autonomously with minimal human intervention beyond:

- Initial permissions
- Required manual inputs (configurations, secrets, preferences)
- Approval to proceed between phases

### Code Readiness

Provide specific shell commands or scripts that the future agent can execute directly. All commands should be:

- Complete and ready to run
- Include proper error handling
- Include verification steps
- Be platform-appropriate

### Platform-Specific Considerations

**Linux:**
- Multiple distributions with different package managers
- Desktop environment detection for completions
- System package installation requires sudo

**macOS:**
- Homebrew is most common package manager
- System Integrity Protection (SIP) may affect some operations
- .app bundles are standard installation method
- Universal binary support for both Intel and Apple Silicon

**Windows:**
- Requires Visual Studio Build Tools or MSVC
- Package managers: winget, chocolatey, scoop
- Path management is important
- Installer creation using WiX (optional)

**BSD (FreeBSD, OpenBSD):**
- Use `pkg` or `pkg_add` for packages
- May require different dependency names
- User limits may need adjustment (OpenBSD)

---

## Important Notes

- Alacritty is in beta; expect some bugs and missing features
- Configuration must be created manually (not auto-generated)
- OpenGL ES 2.0 or higher is required
- Wayland with Nvidia GPUs may need EGL drivers
- Minimum Supported Rust Version (MSRV): 1.85.0
- The project uses a Cargo workspace with multiple crates
- All workspace members share the same Rust version requirement

## Resources

- Official Documentation: https://alacritty.org
- Configuration Reference: https://alacritty.org/config-alacritty.html
- GitHub Repository: https://github.com/alacritty/alacritty
- IRC Channel: #alacritty on libera.chat
- Issue Tracker: https://github.com/alacritty/alacritty/issues
