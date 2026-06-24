{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-parts,
      treefmt-nix,
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ treefmt-nix.flakeModule ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        { pkgs, ... }:
        {
          treefmt = {
            projectRootFile = "flake.nix";
            programs.nixfmt.enable = true;
            programs.statix.enable = true;
          };

          devShells.default =
            let
              packages = with pkgs; [
                just
                rustup
                git
                boost
                cmake
                typos
                python312
                uv
                go
                cargo-hack
                pkg-config
                openssl
                llvmPackages.clang
              ];
            in
            pkgs.mkShell {
              inherit packages;
              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
              CMAKE_PREFIX_PATH = "${pkgs.boost.dev}";
            };
        };
    };
}
