#!/usr/bin/env bash

# Alacritty install script
# Created By: Chris, LinuxUser255.  https://github.com/LinuxUser255/
# License: GNU GPLv3
#
# Install shell script for the Alacritty Terminal emulator on Debian & Debian-based distros
#
# To-Do
#----------------------------------------------------------------------------------------
# Add the config file: alacritty.toml
# It' best to do this from your home directory
#
# Where to put the config file
# -> The alacritty.toml goes in the ~/.config/alacritty directory
# You will have to make the alacritty directory yourself, and remeber, it goes in the .config directory.
# mkdir .config/alacritty
#
# This is a generic alacritty.toml
# curl -LO https://raw.githubusercontent.com/LinuxUser255/BashAndLinux/main/Alacritty/configs/alacritty.toml
#
# This one is my custom config:
# curl -LO https://raw.githubusercontent.com/LinuxUser255/BashAndLinux/main/Alacritty/alacritty.toml
#
#
# After you have made the .config/alacritty directory, then curl which ever config you want listed above, and move it to  ~/.config/alacritty
# It' best to do this from your home directory
# mv alacritty.toml -t  ~/.config/alacritty
#
#---------------------------------------------------------------------------------------
#
# About the install script:
#
# This is a 2 part install Process
#
# Pre-build, and Post-build
#
# Part 1: Prebuild
# First check for sudo privileges, and if so, then proceede.
# Check for and, install Dependencies
# Install the Rust compiler
# Source the cargo environment
# Clone the Alacritty source code
# Build Alacritty from source
#
# Part 2: Post-build
# Post Build Alacritty Configurations
# Checking Terminfo
# Creating a Desktop Entry
# Enable Shell completions for Zsh, Bash, and Fish
#
#-------------Part 1: Pre-build-------------------------------------------------------#
#
# First check for sudo privileges, and if so, then proceede
is_sudo() {
    if [ ${UID} -ne 0 ]; then
        printf "\e[1;31m Sudo privileges required. \e[0m\n"
        exit 1
    fi
}

# Install dependencies
check_and_install_packages() {

   #dependencies to check for
   packages=(
       curl
       git
       cmake
       scdoc
       ripgrep
       pkg-config
       libfreetype6-dev
       libfontconfig1-dev
       libxcb-xfixes0-dev
       libxkbcommon-dev
       python3
   )

    # Check if packages are already installed
    printf "\e[1;31m Checking dependencies \e[0m\n"
    missing_packages=()
    for package in "${packages[@]}"; do
        if ! dpkg -l | grep -q "^ii\s*$package\s"; then
            missing_packages+=("$package")
        fi
    done

    if [ ${#missing_packages[@]} -eq 0 ]; then
        printf "\e[1;31m Dependencies already installed \e[0m\n"
    else
        printf "\e[1;31m Installing missing packages... \e[0m\n"
        sudo apt update
        sudo apt install -y "${missing_packages[@]}"
        sudo apt upgrade
        printf "\e[1;31m Installation complete. \e[0m\n"
    fi
}

# Install rustup and its compiler
install_rustup_and_compiler() {
    printf "\e[1;31m Installing rustup and its compiler... \e[0m\n"

    # Install rustup and the Rust compiler
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

    # Source the cargo enviroment
    source $HOME/.cargo/env

    printf "\e[1;31m Rustup and its compiler installed successfully.\e[0m\n"
}

# Cloning & building from source
clone_and_build() {
    printf "\e[1;31mInstalling Alacritty\e[0m\n"

    git clone https://github.com/alacritty/alacritty.git
    # Need to ensure that necssary cmds are executed in the alacritty dir
    cd alacritty
    cargo build --release
    # configure your current shell &  copy the alacritty binary to path
    source "$HOME/.cargo/env"
    sudo cp -r ~/target/release/alacritty /usr/local/bin
    printf "\e[1;31mAlacritty binary is installed\e[0m\n"
}


# Verify and install terminfo for Alacritty
verify_and_install_terminfo() {
    printf "\e[1;31mVerifying terminfo installation for Alacritty...\e[0m\n"

    # Check if terminfo for Alacritty is installed
    if infocmp alacritty &> /dev/null; then
        printf "\e[1;31mTerminfo for Alacritty is installed.\e[0m\n"
    else
        printf "\e[1;31mTerminfo for Alacritty is not installed. Installing globally...\e[0m\n"
        # Install terminfo globally
        sudo tic -xe alacritty,alacritty-direct extra/alacritty.info
        printf "\e[1;31mTerminfo installed successfully.\e[0m\n"

    fi
}

#-------------Part 2: Post-build-------------------------------------------------------#

# Create a Desktop entry for Alacritty
create_desktop_entry() {
    printf "\e[1;31mTCreating desktop entry..\e[0m\n"

    sudo cp target/release/alacritty /usr/local/bin
    sudo cp extra/logo/alacritty-term.svg /usr/share/pixmaps/Alacritty.svg
    sudo desktop-file-install extra/linux/Alacritty.desktop
    sudo update-desktop-database

    printf "\e[1;31mTDesktop entry created..\e[0m\n"
}

# Check which shell is in use, then install the appropriate auto complete
check_shell() {
    printf "\e[1;31m Checking user shell to for Alacritty's auto complete \e[0m\n"

    SHELL_TYPE=$(basename "$SHELL")

    case $SHELL_TYPE in
        "zsh")
            printf "\e[1;31m Current shell is Zsh. \e[0m\n"
            # Create directory for Zsh completions if not already present
            mkdir -p ${ZDOTDIR:-~}/.zsh_functions
            echo 'fpath+=${ZDOTDIR:-~}/.zsh_functions' >> ${ZDOTDIR:-~}/.zshrc
            cp extra/completions/_alacritty ${ZDOTDIR:-~}/.zsh_functions/_alacritty
            ;;
        "bash")
            printf "\e[1;31m Current shell is Bash. \e[0m\n"
            # Source the completion file in ~/.bashrc
            echo "source $(pwd)/extra/completions/alacritty.bash" >> ~/.bashrc
            ;;
        "fish")
            printf "\e[1;31m Current shell is Fish. \e[0m\n"
            # Get Fish completion directory
            fish_complete_path=(`echo $fish_complete_path`)
            # Create directory for Fish completions if not already present
            mkdir -p $fish_complete_path[1]
            # Copy completion file to Fish directory
            cp extra/completions/alacritty.fish $fish_complete_path[1]/alacritty.fish
            ;;
        *)
            printf "\e[1;31m Current shell not supported. \e[0m\n"
            ;;
    esac
}

main() {
    is_sudo

    check_and_install_packages

    install_rustup_and_compiler

    clone_and_build

    verify_and_install_terminfo

    create_desktop_entry

    check_shell
}

main
