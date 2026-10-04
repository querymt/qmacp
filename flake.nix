{
  description = "JSON CLI for a running QueryMT ACP WebSocket server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = inputs @ {self, ...}:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      systems = inputs.nixpkgs.lib.systems.flakeExposed;

      perSystem = {system, ...}: let
        overlays = [inputs.rust-overlay.overlays.default];
        pkgs = import inputs.nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

        qmacp = pkgs.rustPlatform.buildRustPackage {
          pname = "qmacp";
          version = cargoToml.package.version;
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          env = {
            QMACP_GIT_SHA = self.shortRev or self.dirtyShortRev or "unknown";
          };
          auditable = false;
          doCheck = false;
        };
      in {
        packages = {
          qmacp = qmacp;
          default = qmacp;
        };

        apps = {
          qmacp = {
            type = "app";
            program = "${self.packages.${system}.qmacp}/bin/qmacp";
          };
          default = {
            type = "app";
            program = "${self.packages.${system}.qmacp}/bin/qmacp";
          };
        };

        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.pkg-config
            pkgs.openssl
          ];

          shellHook = ''
            export PS1="(dev:qmacp) $PS1"
          '';
        };
      };
    };
}
