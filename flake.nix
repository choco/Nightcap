{
  description = "Nightcap development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShellNoCC {
          packages = [
            (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
            pkgs.gh
            pkgs.librsvg
            pkgs.jq
            pkgs.nodejs_24
            pkgs.pnpm
            pkgs.python3Packages.dmgbuild
          ];
          # The Rust toolchain propagates Nix's clang wrapper and SDK. On macOS, cargo
          # builds with Xcode's tools instead, matching CI and the Swift shell.
          shellHook = ''
            export PATH="$(git rev-parse --show-toplevel)/scripts:$PATH"
          ''
          + pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isDarwin ''
            export CC=/usr/bin/cc CXX=/usr/bin/c++ AR=/usr/bin/ar
            export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/usr/bin/cc
            export MACOSX_DEPLOYMENT_TARGET=15.0
          '';
        };
      });
    };
}
