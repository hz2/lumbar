{
  description = "A hardware-agnostic cache and memory-hierarchy simulator";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./src
              ./tests
              ./examples
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
        };
      });

      devShells = forAllSystems (
        pkgs:
        let
          inherit (pkgs) cargo rustc rustfmt clippy rust-analyzer;
        in
        {
          default = pkgs.mkShell {
            packages = [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
            ];
          };
        }
      );

      # `nix flake check` re-exercises the package build, which runs the full
      # test suite (including doctests) via buildRustPackage's default check
      # phase -- see the CI workflow for the fmt/clippy checks this doesn't
      # cover.
      checks = forAllSystems (pkgs: {
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      });
    };
}
