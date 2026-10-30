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
      devShells = forAllSystems (
        pkgs:
        let
          # The linters alone, for CI's lint job, which takes cargo from the runner.
          lint = pkgs.mkShellNoCC {
            packages = [
              pkgs.actionlint
              pkgs.cargo-deny
              pkgs.shellcheck
            ];
          };
          # The vulnerability checks, for CI's audit job, which takes cargo from the runner too.
          audit = pkgs.mkShellNoCC {
            packages = [
              pkgs.cargo-deny
              pkgs.gh
              pkgs.jq
              pkgs.osv-scanner
              pkgs.pnpm
              pkgs.zizmor
            ];
          };
        in
        {
          inherit lint audit;
          default = pkgs.mkShellNoCC {
            inputsFrom = [
              lint
              audit
            ];
            packages = [
              (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
              pkgs.librsvg
              pkgs.nodejs_24
            ]
            # dmgbuild builds the DMG with hdiutil, so nixpkgs offers it on macOS alone.
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.python3Packages.dmgbuild ];
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
        }
      );
    };
}
