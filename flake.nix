{
  inputs.fenix = {
    url = "github:nix-community/fenix";
    inputs.nixpkgs.follows = "nixpkgs";
  };
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { fenix, nixpkgs, ... }:
    let
      systems = [
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.mkShell {
            packages = [
              (fenix.packages.${system}.complete.withComponents [
                "cargo"
                "clippy"
                "rust-src"
                "rustc"
                "rustfmt"
              ])
              pkgs.mononoki
              fenix.packages.${system}.rust-analyzer
            ];
            BLIT_TEST_FONT = "${pkgs.mononoki}/share/fonts/opentype/mononoki-Regular.otf";
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
              libxkbcommon
              wayland
              libx11
              libxcursor
              libxi
              libxrandr
            ]);
          };
        });
    };
}
