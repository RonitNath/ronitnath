{
  description = "Nix package for ronitnath";

  inputs = {
    # flake.lock pins these inputs once `nix flake lock` is run on the Linux
    # build host. Keep the lock committed so profile generations are
    # reconstructable.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, ... }:
    let
      supportedSystems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in {
      packages = forAllSystems (system:
        let
          # This repository has no rust-toolchain.toml, so use nixpkgs'
          # matched rustPlatform directly. The pinned rust-overlay input is
          # ready for fromRustupToolchainFile if the repository adds one.
          pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "ronitnath";
            version = "0.1.0";
            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "--bin" "site" "--bin" "admin" ];

            # sqlx query macros compile against the committed offline cache;
            # no production database is needed or available in the sandbox.
            SQLX_OFFLINE = "true";

            # build.sh accepts an explicit esbuild executable. esbuild is a
            # build-only Go binary, so Node and a package manager are absent
            # from the runtime closure.
            preBuild = ''
              ESBUILD=${pkgs.esbuild}/bin/esbuild sh ts/build.sh
            '';

            postInstall = ''
              # Fail during packaging rather than after activation if Cargo's
              # install hook ever stops installing one of the selected bins.
              test -x "$out/bin/site"
              test -x "$out/bin/admin"

              mkdir -p "$out/share/ronitnath"
              cp -R static "$out/share/ronitnath/static"
              test -f "$out/share/ronitnath/static/dist/site.js"
              test -f "$out/share/ronitnath/static/dist/guestbook.js"
              test -f "$out/share/ronitnath/static/dist/event_rsvp.js"
              test -f "$out/share/ronitnath/static/dist/events_admin.js"
            '';

            meta.mainProgram = "site";
          };
        });
    };
}
