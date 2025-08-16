{
  description = "Alacritty flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    fenix.url = "github:nix-community/fenix";

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    flake-utils,
    fenix,
    ...
  } @ inputs:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      crane = inputs.crane.mkLib pkgs;

      libs = with pkgs; [ freetype.dev freetype fontconfig fontconfig.dev wayland wayland.dev libxkbcommon libxkbcommon.dev libGL ];
      toolchain = with fenix.packages.${system};
        combine [
          minimal.rustc
          minimal.cargo
          complete.rustfmt
          complete.clippy
        ];

      craneLib = crane.overrideToolchain toolchain;
    in {
      formatter = pkgs.alejandra;
      devShells.default = craneLib.devShell {
        packages = with pkgs; [toolchain  pkg-config] ++ libs;

        nativeBuildInputs = with pkgs; [pkg-config];
        LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath libs}";

      };
    });
}
