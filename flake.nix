{
  description = "A pretty git log viewer for showing commits ahead of main";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "git-log-pretty";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            rustc
            cargo
          ];

          buildInputs = with pkgs; [
            git
            openssl
            libgit2
          ];

          meta = with pkgs.lib; {
            description = "A pretty git log viewer for showing commits ahead of main";
            homepage = "https://github.com/andrewgazelka/git-log-pretty"; # Update with your GitHub username
            license = licenses.mit; # Update this based on your LICENSE file
            maintainers = [ ];
            platforms = platforms.unix;
          };
        };

        # Development shell
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rust-analyzer
            rustfmt
            clippy
            pkg-config
            git
            openssl
            libgit2
          ];

          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
        };
      });
} 