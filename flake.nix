{
  description = "Pinned build environment for ronitnath";

  inputs = {
    # The generated flake.lock pins this nixpkgs input. Commit that lock so
    # every build host resolves the exact same toolchain.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  # This flake deliberately packages nothing. Release images build
  # incrementally on the build host against persistent caches, so a source
  # change recompiles only the crates it touches; the flake's sole job is
  # pinning the toolchain development uses.
  outputs = { nixpkgs, rust-overlay, ... }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
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
              (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)

              # Islands assets build in this same shell. corepack's pnpm shim
              # resolves the exact pnpm version pinned by package.json.
              pkgs.nodejs_24
              pkgs.corepack_24
            ];
          };
        });
    };
}
