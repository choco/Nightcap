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
          # The linters and the secret scanner alone, for CI's lint job, which takes cargo from the
          # runner, and the pre-commit hook.
          lint = pkgs.mkShellNoCC {
            packages = [
              pkgs.actionlint
              pkgs.betterleaks
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
          # dmgbuild, for CI's release job, which builds the DMG on the macOS runner. It builds
          # the image with hdiutil, so nixpkgs offers it on macOS alone.
          dmg = pkgs.mkShellNoCC {
            packages = [ pkgs.python3Packages.dmgbuild ];
          };
          isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
        in
        {
          inherit lint audit;
          default = pkgs.mkShellNoCC {
            inputsFrom = [
              lint
              audit
            ]
            ++ pkgs.lib.optionals isDarwin [ dmg ];
            packages = [
              (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
              pkgs.librsvg
              pkgs.nodejs_24
            ];
            # The Rust toolchain propagates Nix's clang wrapper and SDK. On macOS, cargo
            # builds with Xcode's tools instead, matching CI and the Swift shell.
            shellHook = ''
              export PATH="$(git rev-parse --show-toplevel)/scripts:$PATH"
            ''
            + pkgs.lib.optionalString isDarwin ''
              export CC=/usr/bin/cc CXX=/usr/bin/c++ AR=/usr/bin/ar
              export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/usr/bin/cc
              export MACOSX_DEPLOYMENT_TARGET=15.0
            '';
          };
        }
        // pkgs.lib.optionalAttrs isDarwin { inherit dmg; }
      );
    };
}
