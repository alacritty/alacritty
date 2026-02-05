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
      in
      {
        packages = {
          default = alacritty;
        };

        apps = {
          default = {
            type = "app";
            program = "${alacritty}/bin/alacritty";
          };
        };
      }
    );
}
