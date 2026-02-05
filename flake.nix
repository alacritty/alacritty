{
  description = "Alacritty terminal emulator";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        stdenv = pkgs.stdenv;
        rpathLibs = [
          pkgs.expat
          pkgs.fontconfig
          pkgs.freetype
        ]
        ++ lib.optionals stdenv.hostPlatform.isLinux [
          pkgs.libGL
          pkgs.libx11
          pkgs.libxcursor
          pkgs.libxi
          pkgs.libxxf86vm
          pkgs.libxcb
          pkgs.libxkbcommon
          pkgs.wayland
        ];
        alacritty = pkgs.rustPlatform.buildRustPackage {
          pname = "alacritty";
          version = "0.17.0-dev";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          cargoBuildFlags = [
            "-p"
            "alacritty"
          ];

          nativeBuildInputs = with pkgs; [
            cmake
            pkg-config
            python3
          ];

          buildInputs = rpathLibs;

          postPatch = lib.optionalString stdenv.hostPlatform.isLinux ''
            substituteInPlace alacritty/src/config/ui_config.rs \
              --replace xdg-open ${pkgs.xdg-utils}/bin/xdg-open
          '';

          postInstall = lib.optionalString stdenv.hostPlatform.isLinux ''
            $STRIP -S $out/bin/alacritty
            patchelf --add-rpath "${lib.makeLibraryPath rpathLibs}" $out/bin/alacritty
          '';

          dontPatchELF = true;
          doCheck = false;

          meta = with lib; {
            description = "A fast, cross-platform, OpenGL terminal emulator";
            homepage = "https://alacritty.org";
            license = licenses.asl20;
            mainProgram = "alacritty";
            platforms = platforms.unix;
          };
        };
        alacrittyApp = pkgs.runCommand "alacritty-app" { } ''
          app="$out/Applications/Alacritty.app/Contents"
          mkdir -p "$app/MacOS" "$app/Resources"
          ln -s ${alacritty}/bin/alacritty "$app/MacOS/alacritty"
          cp ${./extra/osx/Alacritty.app/Contents/Info.plist} "$app/Info.plist"
          cp ${./extra/osx/Alacritty.app/Contents/Resources/alacritty.icns} "$app/Resources/alacritty.icns"
        '';
        alacrittyLinkApp = pkgs.writeShellScriptBin "alacritty-link-app" ''
          set -euo pipefail

          profile_dir="${NIX_PROFILE:-$HOME/.nix-profile}"
          app_source="$profile_dir/Applications/Alacritty.app"
          app_target="$HOME/Applications/Alacritty.app"

          if [ ! -e "$app_source" ]; then
            echo "Alacritty.app not found in Nix profile." >&2
            echo "Expected: $app_source" >&2
            exit 1
          fi

          mkdir -p "$HOME/Applications"

          if [ -L "$app_target" ]; then
            target_link="$(readlink "$app_target" || true)"
            if [ "$target_link" = "$app_source" ]; then
              echo "Already linked: $app_target"
              exit 0
            fi
          fi

          if [ -e "$app_target" ] || [ -L "$app_target" ]; then
            ts="$(date +%Y%m%d%H%M%S)"
            backup="$app_target.bak-$ts"
            mv "$app_target" "$backup"
            echo "Backed up existing app to $backup"
          fi

          ln -s "$app_source" "$app_target"

          if command -v mdimport >/dev/null 2>&1; then
            mdimport "$app_target" >/dev/null 2>&1 || true
          fi

          echo "Linked $app_target"
          echo "If Spotlight does not pick it up, run:"
          echo "  mdimport \"$app_target\""
          echo "  open -a \"Alacritty\""
        '';
        alacrittyDesktop = pkgs.runCommand "alacritty-desktop" { } ''
          mkdir -p "$out/share/applications" "$out/share/icons/hicolor/scalable/apps"
          cp ${./extra/linux/Alacritty.desktop} "$out/share/applications/Alacritty.desktop"
          cp ${./extra/logo/alacritty-term.svg} "$out/share/icons/hicolor/scalable/apps/Alacritty.svg"
          substituteInPlace "$out/share/applications/Alacritty.desktop" \
            --replace "Exec=alacritty" "Exec=${alacritty}/bin/alacritty" \
            --replace "TryExec=alacritty" "TryExec=${alacritty}/bin/alacritty"
        '';
        alacrittyLinkDesktop = pkgs.writeShellScriptBin "alacritty-link-desktop" ''
          set -euo pipefail

          profile_dir="${NIX_PROFILE:-$HOME/.nix-profile}"
          desktop_source="$profile_dir/share/applications/Alacritty.desktop"
          icon_source="$profile_dir/share/icons/hicolor/scalable/apps/Alacritty.svg"
          desktop_target="$HOME/.local/share/applications/Alacritty.desktop"
          icon_target="$HOME/.local/share/icons/hicolor/scalable/apps/Alacritty.svg"

          if [ ! -e "$desktop_source" ]; then
            echo "Alacritty.desktop not found in Nix profile." >&2
            echo "Expected: $desktop_source" >&2
            exit 1
          fi

          mkdir -p "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/scalable/apps"

          link_with_backup() {
            src="$1"
            dst="$2"
            label="$3"

            if [ -L "$dst" ]; then
              target_link="$(readlink "$dst" || true)"
              if [ "$target_link" = "$src" ]; then
                echo "Already linked: $dst"
                return 0
              fi
            fi

            if [ -e "$dst" ] || [ -L "$dst" ]; then
              ts="$(date +%Y%m%d%H%M%S)"
              backup="$dst.bak-$ts"
              mv "$dst" "$backup"
              echo "Backed up existing $label to $backup"
            fi

            ln -s "$src" "$dst"
            echo "Linked $dst"
          }

          link_with_backup "$desktop_source" "$desktop_target" "desktop entry"

          if [ -e "$icon_source" ]; then
            link_with_backup "$icon_source" "$icon_target" "icon"
          fi

          if command -v update-desktop-database >/dev/null 2>&1; then
            update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true
          fi
        '';
        alacrittyProfileInstall = pkgs.writeShellScriptBin "alacritty-profile-install" ''
          set -euo pipefail

          if [ "$#" -eq 0 ]; then
            set -- .
          fi

          profile_dir="${NIX_PROFILE:-$HOME/.nix-profile}"
          args=("$@")
          for ((i=0; i<${#args[@]}; i++)); do
            case "${args[$i]}" in
              --profile)
                if [ $((i+1)) -lt ${#args[@]} ]; then
                  profile_dir="${args[$((i+1))]}"
                fi
                ;;
              --profile=*)
                profile_dir="${args[$i]#--profile=}"
                ;;
            esac
          done

          ${pkgs.nix}/bin/nix profile add "$@"

          if [ -e "$profile_dir" ]; then
            profile_dir="$(cd "$profile_dir" && pwd)"
          fi

          export NIX_PROFILE="$profile_dir"
          profile_bin="$profile_dir/bin"

          app_source="$profile_dir/Applications/Alacritty.app"
          desktop_source="$profile_dir/share/applications/Alacritty.desktop"

          if [ -e "$app_source" ] && [ -x "$profile_bin/alacritty-link-app" ]; then
            "$profile_bin/alacritty-link-app"
          fi

          if [ -e "$desktop_source" ] && [ -x "$profile_bin/alacritty-link-desktop" ]; then
            "$profile_bin/alacritty-link-desktop"
          fi
        '';
      in
      {
        packages = {
          default =
            if stdenv.hostPlatform.isDarwin then
              pkgs.symlinkJoin {
                name = "alacritty";
                paths = [
                  alacritty
                  alacrittyApp
                  alacrittyLinkApp
                ];
                meta = alacritty.meta;
              }
            else if stdenv.hostPlatform.isLinux then
              pkgs.symlinkJoin {
                name = "alacritty";
                paths = [
                  alacritty
                  alacrittyDesktop
                  alacrittyLinkDesktop
                ];
                meta = alacritty.meta;
              }
            else
              alacritty;
          alacritty = alacritty;
          profile-install = alacrittyProfileInstall;
        }
        // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
          app = alacrittyApp;
          link-app = alacrittyLinkApp;
        }
        // lib.optionalAttrs stdenv.hostPlatform.isLinux {
          desktop = alacrittyDesktop;
          link-desktop = alacrittyLinkDesktop;
        };

        apps = {
          default = {
            type = "app";
            program = "${alacritty}/bin/alacritty";
          };
          profile-install = {
            type = "app";
            program = "${alacrittyProfileInstall}/bin/alacritty-profile-install";
          };
        }
        // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
          link-app = {
            type = "app";
            program = "${alacrittyLinkApp}/bin/alacritty-link-app";
          };
        }
        // lib.optionalAttrs stdenv.hostPlatform.isLinux {
          link-desktop = {
            type = "app";
            program = "${alacrittyLinkDesktop}/bin/alacritty-link-desktop";
          };
        };
      }
    );
}
