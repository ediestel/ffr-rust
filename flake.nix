{
  description = "ffr.nvim — classified, bounded, chunk-capable file reading for Neovim & AI agents";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        cargoToml = builtins.fromTOML (builtins.readFile ./crates/ffr-nvim/Cargo.toml);

        commonArgs = {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;

          nativeBuildInputs = [ pkgs.pkg-config pkgs.llvmPackages.libclang.lib ];
          buildInputs = with pkgs; [ openssl ];
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };

        ffr-native = craneLib.buildPackage (
          commonArgs
          // {
            cargoArtifacts = craneLib.buildDepsOnly commonArgs;
            doCheck = false;
          }
        );

        copy-dynamic-library = ''
          set -eo pipefail
          mkdir -p target/release
          if [ "$(uname)" = "Darwin" ]; then
            cp -vf ${ffr-native}/lib/libffr_nvim.dylib target/release/libffr_nvim.dylib
          else
            cp -vf ${ffr-native}/lib/libffr_nvim.so target/release/libffr_nvim.so
          fi
          echo "Library copied to target/release/"
        '';
      in
      {
        checks = { inherit ffr-native; };

        packages = {
          default = ffr-native;
          ffr-nvim = pkgs.vimUtils.buildVimPlugin {
            pname = "ffr.nvim";
            version = "main";
            src = pkgs.lib.cleanSource ./.;
            postPatch = copy-dynamic-library;
            doCheck = false;
          };
        };

        apps.default = flake-utils.lib.mkApp { drv = ffr-native; };
        apps.release = flake-utils.lib.mkApp {
          drv = pkgs.writeShellScriptBin "release" copy-dynamic-library;
        };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = [ ];
        };
      }
    );
}
