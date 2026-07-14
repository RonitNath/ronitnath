{
  description = "Pinned build environment for ronitnath";

  inputs = {
    # flake.lock pins these inputs. Keep the lock committed so every build
    # host resolves the exact same toolchain.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  # This flake deliberately packages nothing. Release builds run incrementally
  # on the build host through deploy/deploy.sh inside this shell, so a source
  # change recompiles only the crates it touches; the flake's sole job is
  # pinning the toolchain that performs those builds (owner ruling 2026-07-14:
  # maximum incrementality, no sandboxed rebuilds).
  outputs = { nixpkgs, rust-overlay, ... }:
    let
      supportedSystems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
        in {
          default = pkgs.mkShell {
            packages = [
              # rust-overlay's current stable, pinned by flake.lock (nixpkgs
              # 25.11 ships rustc 1.91.1, older than this crate graph needs).
              pkgs.rust-bin.stable.latest.minimal
              # esbuild is the entire JS toolchain here: ts/build.sh bundles
              # the no-package-manager frontend with it.
              pkgs.esbuild
            ];
          };
        });
    };
}
