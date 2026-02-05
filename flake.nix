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

          app_source="$HOME/.nix-profile/Applications/Alacritty.app"
          app_target="$HOME/Applications/Alacritty.app"

          if [ ! -e "$app_source" ]; then
            echo "Alacritty.app not found in Nix profile." >&2
            echo "Expected: $app_source" >&2
            exit 1
          fi

          mkdir -p "$HOME/Applications"
          ln -sfn "$app_source" "$app_target"

          if command -v mdimport >/dev/null 2>&1; then
            mdimport "$app_target" >/dev/null 2>&1 || true
          fi

          echo "Linked $app_target"
          echo "If Spotlight does not pick it up, run:"
          echo "  mdimport \"$app_target\""
          echo "  open -a \"Alacritty\""
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
            else
              alacritty;
          alacritty = alacritty;
        }
        // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
          app = alacrittyApp;
          link-app = alacrittyLinkApp;
        };

        apps = {
          default = {
            type = "app";
            program = "${alacritty}/bin/alacritty";
          };
        }
        // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
          link-app = {
            type = "app";
            program = "${alacrittyLinkApp}/bin/alacritty-link-app";
          };
        };
      }
    );
}
