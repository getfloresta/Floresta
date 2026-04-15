{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    nixpkgs-unstable.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    floresta-flake.url = "github:getfloresta/floresta-nix/stable_building";
    pre-commit-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs-unstable";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      nixpkgs-unstable,
      pre-commit-hooks,
      floresta-flake,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];

        pkgs = import nixpkgs { inherit system overlays; };

        # We use src a lot across this flake
        src = ./.;

        inherit (floresta-flake.lib.${system}) florestaBuild;
      in
      with pkgs;
      {
        packages = {
          florestad = florestaBuild.build {
            inherit src;
            packageName = "florestad";
          };
          floresta-cli = florestaBuild.build {
            inherit src;
            packageName = "floresta-cli";
          };
          libfloresta = florestaBuild.build {
            inherit src;
            packageName = "libfloresta";
          };
          floresta-debug = florestaBuild.build {
            inherit src;
            packageName = "floresta-debug";
          };
          default = florestaBuild.build {
            inherit src;
            packageName = "all";
          };
        };
        devShells.default =
          let
            # This is the dev tools used while developing in Floresta.
            packages = with pkgs; [
              just
              rustup
              git
              boost
              cmake
              typos
              taplo
              python312
              uv
              gcc
              go
            ];

            preCommitHooks = pre-commit-hooks.lib.${system}.run {
              src.root = src;
              default_stages = [
                "pre-commit"
                "pre-push"
              ];
              hooks = {
                check-merge-conflicts = {
                  enable = true;
                  stages = [ "pre-commit" ];
                };
                nixfmt = {
                  enable = true;
                  stages = [ "pre-commit" ];
                };
                commitizen.enable = true;
                statix = {
                  enable = true;
                  stages = [ "pre-commit" ];
                };
                flake-checker = {
                  enable = true;
                  stages = [ "pre-commit" ];
                };
                tomlfmt-hook = {
                  enable = true;
                  name = "tomlfmt-hook";
                  entry = "${pkgs.just}/bin/just toml-fmt --check";
                  language = "system";
                  stages = [ "pre-commit" ];
                  types = [ "toml" ];
                  pass_filenames = false;
                  verbose = true;
                };
                justfmt-hook = {
                  enable = true;
                  name = "justfmt-hook";
                  entry = "${pkgs.just}/bin/just just-fmt --check";
                  language = "system";
                  stages = [ "pre-commit" ];
                  types = [ "file" ];
                  files = "justfile";
                  pass_filenames = false;
                  verbose = true;
                };
                rustfmt-hook = {
                  enable = true;
                  name = "rustfmt-hook";
                  entry = "${pkgs.just}/bin/just fmt --check";
                  language = "system";
                  stages = [ "pre-commit" ];
                  types = [ "rust" ];
                  pass_filenames = false;
                  verbose = true;
                };
                rusttest-unit-hook = {
                  enable = true;
                  name = "rusttest-unit-hook";
                  entry = "${pkgs.just}/bin/just test-unit";
                  language = "system";
                  stages = [ "pre-commit" ];
                  types = [ "rust" ];
                  pass_filenames = false;
                  verbose = true;
                };
                typos-hook = {
                  enable = true;
                  name = "typos-hook";
                  entry = "${pkgs.just}/bin/just spell-check";
                  language = "system";
                  stages = [ "pre-commit" ];
                  types = [ "rust" ];
                  pass_filenames = false;
                  verbose = true;
                };
                rusttest-full-hook = {
                  enable = true;
                  name = "rusttest-full-hook";
                  entry = "${pkgs.just}/bin/just test";
                  language = "system";
                  stages = [ "pre-push" ];
                  types = [ "rust" ];
                  pass_filenames = false;
                  verbose = true;
                };
              };
            };

            # Floresta flavored commitizen config file.
            czFlorestaConfigFile = pkgs.writeText ".cz.toml" ''
              [tool.commitizen]
              name = "cz_customize"

              [tool.commitizen.customize]
              types = [
                { type = "feat",    description = "A new feature" },
                { type = "fix",     description = "A bug fix" },
                { type = "docs",    description = "Documentation changes" },
                { type = "style",   description = "Code style changes (formatting, missing semicolons, etc.)" },
                { type = "refactor",description = "Code changes that neither fix a bug nor add a feature" },
                { type = "test",    description = "Adding missing tests or correcting existing tests" },
                { type = "perf",    description = "A code change that improves performance" },
                { type = "ci",      description = "Changes to CI configuration files and scripts" },
                { type = "chore",   description = "Other changes that don't modify src or test files" },
                { type = "fuzz",    description = "Fuzzing-related changes" },
                { type = "bench",   description = "Benchmark-related changes" }
              ]

              schema_pattern = '^(feat|fix|docs|style|refactor|test|perf|ci|chore|fuzz|bench)(\([^)]+\))?: [^\n]+(\n\n[\s\S]+)?$'
            '';

            czHook = ''
              cp -f ${czFlorestaConfigFile} .cz.toml
              echo "Commitizen config written"
            '';

            shellHook = preCommitHooks.shellHook + czHook;
          in
          mkShell {
            inherit packages shellHook;
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            CMAKE_PREFIX_PATH = "${pkgs.boost.dev}";
          };
      }
    );
}
