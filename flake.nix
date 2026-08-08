{
  # The reproducible build contract for CI (procedures/ci.md): the gates compile
  # with the exact toolchain pinned here, never with whatever rustc happens to be
  # installed on the runner. The release image pins its own toolchain in the
  # Containerfile; keep the two in step when either moves.
  description = "rn-site — ronitnath.com (Leptos islands + Axum)";

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

        # The crate is edition 2024; leptos 0.9-beta wants a recent stable.
        toolchain = pkgs.rust-bin.stable."1.97.1".default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.cargo-leptos
            # wasm-opt, invoked by cargo-leptos on release builds.
            pkgs.binaryen
            # cargo-leptos shells out to the tailwind CLI for
            # style/tailwind.css; providing it here keeps the devshell from
            # reaching out to download one.
            pkgs.tailwindcss_4
            pkgs.pkg-config
          ];
        };
      });
}
