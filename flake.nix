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

  # This flake deliberately packages nothing. It pins the host-native
  # verification toolchain; production artifacts are built from the separately
  # pinned OCI stages in Dockerfile.
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
              # The islands pipeline is Vite + Solid through the package-lock
              # controlled pnpm workspace; no host-global Node tooling.
              pkgs.nodejs
              pkgs.pnpm
            ];
          };
        });
    };
}
