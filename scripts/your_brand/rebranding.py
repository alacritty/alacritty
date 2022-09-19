#!/usr/bin/env python3
# -*- coding: utf-8 -*-
#

import os
import sys
import re
import shutil
import argparse
import subprocess

your_brand_folder = ""

filenames_uppercase_alacritty = [
    "../../alacritty/src/ipc.rs",
    "../../alacritty/src/logging.rs",
]

filenames_capitalized_alacritty = [
    "../../alacritty/extra/linux/Alacritty.desktop",
    "../../alacritty/extra/osx/Alacritty.app/Contents/Info.plist",
    "../../alacritty/src/ipc.rs",
    "../../alacritty/src/logging.rs",
    "../../alacritty/src/panic.rs",
    "../../alacritty/src/config/window.rs",
    "../../alacritty/windows/wix/alacritty.wxs",
]

filenames_lowercase_alacritty = [
    "../../alacritty/extra/linux/Alacritty.desktop",
    "../../alacritty/extra/osx/Alacritty.app/Contents/Info.plist",
    "../../alacritty/windows/wix/alacritty.wxs",
]

# Replace all occurrences of a string in a line
# alacritty/src/main.rs
main_rs_welcome_msg = "Welcome to Alacritty"
# alacritty/src/cli.rs
cli_rs = 'clap_complete::generate(*shell, &mut clap, "alacritty", &mut generated);'
# alacritty/scr/ipc.rs
ipc_rs = 'xdg::BaseDirectories::with_prefix("alacritty")' #??
# alacritty/src/logging.rs
logging_rs = 'pub const LOG_TARGET_IPC_CONFIG: &str = "alacritty_log_ipc_config";', #??
logging_rs_2 = '"alacritty"'
# alacritty/src/config/mod.rs
mod_rs = 'xdg::BaseDirectories::with_prefix("alacritty")'
# alacritty/windows/alacritty.rc
# alacritty_rc = 'alacritty.ico'

#?? alacritty.info

# alacritty/Cargo.toml
cargo_toml_name = 'name = "alacritty"' 
cargo_toml_homepage = 'homepage = "https://github.com/alacritty/alacritty"'


# Replace files
# README.md
root_readme = "../../README.md"
appdata_xml = "../../alacritty/extra/linux/org.alacritty.Alacritty.appdata.xml"
logo_simple_svg = "../../alacritty/extra/logo/alacritty-simple.svg"
logo_term_svg = "../../alacritty/extra/logo/alacritty-term.svg"
logo_term_scanlines_svg = "../../alacritty/extra/logo/alacritty-term+scanlines.svg"
logo_compat_simple_svg = "../../alacritty/extra/logo/compat/alacritty-simple.svg",
logo_compat_term_svg = "../../alacritty/extra/logo/compat/alacritty-term.svg",
logo_compat_term_scanlines_svg = "../../alacritty/extra/logo/compat/alacritty-term+scanlines.svg"
alacritty_ico = "../../alacritty/windows/alacritty.ico"

alacritty_folder = "alacritty_brand"

def replace_in_files(files, old, new):
    # replace all occurances of alacritty with brand name in list of files
    for file in files:
        with open(file, 'r') as f:
            s = f.read()
        s = s.replace(old, new)
        with open(file, 'w') as f:
            f.write(s)

# revert changes from main() function
def revert_changes():
    brand_name = ""
    if os.path.exists("brand_name.txt"):
        with open("brand_name.txt", "r") as f:
            brand_name = f.read()
    else:
        print("File brand_name.txt not found")
        sys.exit(1)

    # LICENSE
    if os.path.exists("LICENSE-APACHE_ALACRITTY"):
        shutil.move('../../alacritty/LICENSE-APACHE-ALACRITTY', '../../alacritty/LICENSE-APACHE')

    # WELCOME_MESSAGE
    welcome_message = ""
    with open("../../WELCOME_MESSAGE", "w") as f:
        welcome = f.readlines()
        welcome_message = welcome.join("\n")
    replace_in_files("../../alacritty/src/main.rs", welcome_message, main_rs_welcome_msg)

    # alacritty/src/cli.rs
    replace_in_files("../../alacritty/src/cli.rs", brand_name, "alacritty")

    # alacritty/src/ipc.rs
    replace_in_files("../../alacritty/src/ipc.rs", brand_name, "alacritty")

    # alacritty/src/logging.rs
    replace_in_files("../../alacritty/src/logging.rs", brand_name, "alacritty")

    # alacritty/src/config/mod.rs
    replace_in_files("../../alacritty/src/config/mod.rs", brand_name, "alacritty")

    # alacritty/Cargo.toml
    replace_in_files("../../alacritty/Cargo.toml", brand_name, "alacritty")

def main():
    """"
    
    """
    parser = argparse.ArgumentParser(description='''Place files from your brand folder here with the
    same names as they appear in alacritty, put logo and log/compat files separately.\n
    You can use --brand for custom folder. And finnaly specify --new brand_name.\n
    Optionally create LICENSE file and WELCOME_MESSAGE\n
    Revert changes with --revert\n
    It's important to run this script from 'scripts/your_brand' directory\n
    Example: python3 rebranding.py --brand my_brand --new my_brand''')
    # parser.add_argument('--help', help=)
    parser.add_argument('--new', help='New name')
    parser.add_argument('--revert', action=argparse.BooleanOptionalAction, help='Revert changes')
    parser.add_argument('--brand', help='Absolute path to your brand folder [default .]')
    parser.print_help()
    args = parser.parse_args()

    if not args.brand:
        args.brand = ""

    if not os.path.exists(args.brand):
        print("Folder not found")
        sys.exit(1)

    if args.revert:
        revert_changes()

    # Check if the new name is valid
    if not re.match(r'^[a-zA-Z0-9]+$', args.new):
        print('The new name is invalid')
        sys.exit(1)

    # Check if the new name is valid
    if str(args.new).lower == "alacritty":
        print('The new name is the same as the old name')
        sys.exit(1)

    os.system(f"echo '{args.new}' > ../../brand_name.txt")

    welcome_message = ""
    with open("../../WELCOME_MESSAGE", "w") as f:
        welcome = f.readlines()
        welcome_message = welcome.join("\n")

    # Check if file exists
    if not os.path.exists("README.md") and not os.path.exists(args.brand + "/" + "README.md"):
        print('README.md not found')
        sys.exit(1)
    else:
        if os.path.exists("README.md"):
            shutil.move(root_readme, alacritty_folder + "/README.md")
            shutil.copy("README.md", root_readme)
        else:
            shutil.move(root_readme, alacritty_folder + "/README.md")
            shutil.copy(args.brand + "/" + "README.md", root_readme)

    # Check if file exists
    if not os.path.exists("org.alacritty.Alacritty.appdata.xml") and not os.path.exists(args.brand + "/" + "org.alacritty.Alacritty.appdata.xml"):
        print('org.alacritty.Alacritty.appdata.xml not found')
        sys.exit(1)
    else:
        if os.path.exists("org.alacritty.Alacritty.appdata.xml"):
            shutil.move(appdata_xml, alacritty_folder + "/org.alacritty.Alacritty.appdata.xml")
            shutil.copy("org.alacritty.Alacritty.appdata.xml", appdata_xml)
        else:
            shutil.move(appdata_xml, alacritty_folder + "/org.alacritty.Alacritty.appdata.xml")
            shutil.copy(args.brand + "/" + "org.alacritty.Alacritty.appdata.xml", appdata_xml)

    # Check if file exists
    if not os.path.exists("logo/alacritty-simple.svg") and not os.path.exists(args.brand + "/" + "logo/alacritty-simple.svg"):
        print('logo/alacritty-simple.svg not found')
        sys.exit(1)
    else:
        if os.path.exists("logo/alacritty-simple.svg"):
            shutil.move(logo_simple_svg, alacritty_folder + "/logo/alacritty-simple.svg")
            shutil.copy("logo/alacritty-simple.svg", logo_simple_svg)
        else:
            shutil.move(logo_simple_svg, alacritty_folder + "/logo/alacritty-simple.svg")
            shutil.copy(args.brand + "/" + "logo/alacritty-simple.svg", logo_simple_svg)

    # Check if file exists
    if not os.path.exists("logo/alacritty-term.svg") and not os.path.exists(args.brand + "/" + "logo/alacritty-term.svg"):
        print('logo/alacritty-term.svg not found')
        sys.exit(1)
    else:
        if os.path.exists("logo/alacritty-term.svg"):
            shutil.move(logo_term_svg, alacritty_folder + "/logo/alacritty-term.svg")
            shutil.copy("logo/alacritty-term.svg", logo_term_svg)
        else:
            shutil.move(logo_term_svg, alacritty_folder + "/logo/alacritty-term.svg")
            shutil.copy(args.brand + "/" + "logo/alacritty-term.svg", logo_term_svg)

    # Check if file exists
    if not os.path.exists("logo/alacritty-term+scanlines.svg") and not os.path.exists(args.brand + "/" + "logo/alacritty-term+scanlines.svg"):
        print('logo/alacritty-term+scanlines.svg not found')
        sys.exit(1)
    else:
        if os.path.exists("logo/alacritty-term+scanlines.svg"):
            shutil.move(logo_term_scanlines_svg, alacritty_folder + "/logo/alacritty-term+scanlines.svg")
            shutil.copy("logo/alacritty-term+scanlines.svg", logo_term_scanlines_svg)
        else:
            shutil.move(logo_term_scanlines_svg, alacritty_folder + "/logo/alacritty-term+scanlines.svg")
            shutil.copy(args.brand + "/" + "logo/alacritty-term+scanlines.svg", logo_term_scanlines_svg)

    # Check if file exists
    if not os.path.exists("logo/compat/alacritty-simple.svg") and not os.path.exists(args.brand + "/" + "logo/compat/alacritty-simple.svg"):
        print('logo/compat/alacritty-simple.svg not found')
        sys.exit(1)
    else:
        if os.path.exists("logo/compat/alacritty-simple.svg"):
            shutil.move(logo_compat_simple_svg, alacritty_folder + "/logo/compat/alacritty-simple.svg")
            shutil.copy("logo/compat/alacritty-simple.svg", logo_compat_simple_svg)
        else:
            shutil.move(logo_compat_simple_svg, alacritty_folder + "/logo/compat/alacritty-simple.svg")
            shutil.copy(args.brand + "/" + "logo/compat/alacritty-simple.svg", logo_compat_simple_svg)

    # Check if file exists
    if not os.path.exists("logo/compat/alacritty-term.svg") and not os.path.exists(args.brand + "/" + "logo/compat/alacritty-term.svg"):
        print('logo/compat/alacritty-term.svg not found')
        sys.exit(1)
    else:
        if os.path.exists("logo/compat/alacritty-term.svg"):
            shutil.move(logo_compat_term_svg, alacritty_folder + "/logo/compat/alacritty-term.svg")
            shutil.copy("logo/compat/alacritty-term.svg", logo_compat_term_svg)
        else:
            shutil.move(logo_compat_term_svg, alacritty_folder + "/logo/compat/alacritty-term.svg")
            shutil.copy(args.brand + "/" + "logo/compat/alacritty-term.svg", logo_compat_term_svg)

    # Check if file exists
    if not os.path.exists("logo/compat/alacritty-term+scanlines.svg") and not os.path.exists(args.brand + "/" + "logo/compat/alacritty-term+scanlines.svg"):
        print('logo/compat/alacritty-term+scanlines.svg not found')
        sys.exit(1)
    else:
        if os.path.exists("logo/compat/alacritty-term+scanlines.svg"):
            shutil.move(logo_compat_term_scanlines_svg, alacritty_folder + "/logo/compat/alacritty-term+scanlines.svg")
            shutil.copy("logo/compat/alacritty-term+scanlines.svg", logo_compat_term_scanlines_svg)
        else:
            shutil.move(logo_compat_term_scanlines_svg, alacritty_folder + "/logo/compat/alacritty-term+scanlines.svg")
            shutil.copy(args.brand + "/" + "logo/compat/alacritty-term+scanlines.svg", logo_compat_term_scanlines_svg)

    # Check if file exists
    if not os.path.exists("alacritty.ico") and not os.path.exists(args.brand + "/" + "alacritty.ico"):
        print('alacritty.ico not found')
        sys.exit(1)
    else:
        if os.path.exists("alacritty.ico"):
            shutil.move(alacritty_ico, alacritty_folder + "/alacritty.ico")
            shutil.copy("alacritty.ico", alacritty_ico)
        else:
            shutil.move(alacritty_ico, alacritty_folder + "/alacritty.ico")
            shutil.copy(args.brand + "/" + "alacritty.ico", alacritty_ico)

    # LICENSE
    if os.path.exists("LICENSE"):
        shutil.move('../../alacritty/LICENSE-APACHE', '../../alacritty/LICENSE-APACHE-ALACRITTY')
        shutil.move('LICENSE', '../../alacritty/LICENSE')
    elif os.path.exists(args.brand + "/" + "LICENSE"):
        shutil.move('../../alacritty/LICENSE-APACHE', '../../alacritty/LICENSE-APACHE-ALACRITTY')
        shutil.move(args.brand + '/LICENSE', '../../alacritty/LICENSE')

    replace_in_files(filenames_uppercase_alacritty, 'ALACRITTY', args.new.upper())
    replace_in_files(filenames_lowercase_alacritty, 'alacritty', args.new.lower())
    replace_in_files(filenames_capitalized_alacritty, 'Alacritty', args.new.title())

    replace_in_files("../../alacritty/src/main.rs", main_rs_welcome_msg, welcome_message)
    
    new = cli_rs.replace("alacritty", args.new.lower())
    replace_in_files("../../alacritty/src/cli.rs", cli_rs, new)

    new = ipc_rs.replace("alacritty", args.new.lower())
    replace_in_files("../../alacritty/src/ipc.rs", ipc_rs, new)

    new = logging_rs.replace("alacritty", args.new.lower())
    replace_in_files("../../alacritty/src/logging.rs", logging_rs, new)

    new = logging_rs_2.replace("alacritty", args.new.lower())
    replace_in_files("../../alacritty/src/logging.rs", logging_rs_2, new)

    new = mod_rs.replace("alacritty", args.new.lower())
    replace_in_files("../../alacritty/src/mod.rs", mod_rs, new)

    new = cargo_toml_name.replace("alacritty", args.new.lower())
    replace_in_files("../../alacritty/Cargo.toml", cargo_toml_name, new)

# run main
if __name__ == "__main__":
    main()