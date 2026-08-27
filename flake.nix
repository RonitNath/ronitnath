{
  # The reproducible build contract for CI (procedures/ci.md): the gates compile
  # with the exact toolchain pinned here, never with whatever rustc happens to be
  # installed on the runner. The release image pins its own toolchain in the
  # Containerfile; keep the two in step when either moves.
  description = "rn-site — ronitnath.com (Axum + askama + trunk CSR bundles)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # The workspace is edition 2024 and pinned by docs/rebuild/plan.md.
        toolchain = pkgs.rust-bin.stable."1.97.1".default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            # trunk builds every CSR bundle (crates/<bundle>/index.html) and
            # fetches its own pinned wasm-bindgen/wasm-opt, exactly as the
            # Containerfile does, so the devshell and the image agree.
            pkgs.trunk
            pkgs.pkg-config
          ];
        };
      });
}
