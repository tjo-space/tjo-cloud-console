{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    crane.url = "github:ipetkov/crane";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      crane,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          buildInputs = with pkgs; [
            rustToolchain
            openssl
            pkg-config
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        bin = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
          }
        );

        image = pkgs.dockerTools.buildImage {
          name = "console";
          tag = "latest";
          copyToRoot = [
            bin
            pkgs.cacert
          ];
          config = {
            Cmd = [ "${bin}/bin/console" ];
          };
        };
      in
      {
        packages = {
          inherit bin image;
          default = bin;
        };
        devShells.default = craneLib.devShell {
          inputsFrom = [ bin ];
          packages = with pkgs; [
            kind
            just
            kubectl
            kubectx
            pgcli
            rust-analyzer
            skopeo
            kubeseal
          ];
        };
      }
    );
}
