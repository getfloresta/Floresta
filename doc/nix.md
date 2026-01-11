# Nix on floresta project

Nix is a declarative package manager that uses the equally named Nix programing language to build deterministic expressions. You can learn more about Nix in their [page](https://nixos.org/) but let's focus on what it does inside this project.

To accomplish any of the steps down below you only need nix installed.

- [Using Floresta in a Nix expression](#using-floresta-in-a-nix-expression)
- [Building with flakes from the project root](#with-flakes)
- [Dev: devShells](#dev-environments-with-devshells)

## Building floresta with nix

In the Floresta project, the Nix packaging logic is maintained in a separate flake: [floresta-nix](https://github.com/getfloresta/floresta-nix).

This repository only keeps the Nix development tooling (such as devShells) and re-exports the set of packages and the `florestaBuild` helper function from that external flake, so they can be used directly from the main Floresta repository.

### Using Floresta in a Nix expression

Just a example of how to consume our expressions. Check out on [floresta-nix](https://github.com/getfloresta/floresta-nix) for more expressive documentation.

```Nix
{
  description = "A very basic flake example for using the Floresta project with Nix";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    floresta.url = "github:getfloresta/floresta"; # Using the latest nix
    # expressions from the main repository of the project.
    floresta-flake.url = "github:getfloresta/floresta-nix/stable_building"; # Using the
    # decoupled nix flake, this one that contains all the nix heavy work to
    # avoid bloating the main repository.
  };

  outputs =
    { self, ... }@inputs:
    let
      # Specify your system
      system = "x86_64-linux";
      floresta = inputs.floresta.packages.${system}.default; # the "default"
      # package will retrieve these components:
      # [ libfloresta, florestad , floresta-cli ]
      florestad = inputs.floresta.packages.${system}.florestad;
      floresta-cli = inputs.floresta.packages.${system}.floresta-cli;

      # This is re exported from the floresta-flake repository that contains
      # the nix heavy work. Ideally, all project specific features offered by
      # the flake.nix
      #
      # on the floresta main repository is re exported from this other flake,
      # the code is maintained there due to some nix maintainabilitty issues.
      custom-floresta = inputs.floresta-flake.lib.${system}.florestaBuild.build {
        packageName = "florestad"; # The package to select:
        # ["florestad", "floresta-cli", "all", "libfloresta", "floresta-debug"]
        features = [ ]; # The features to append during build time.
        # The caller may want to pass his own version of the codebase.
        #
        # Note that this expression only supports florestas codebases after the
        # structure changes, that is, the binaries under bin/.
        src = (import inputs.nixpkgs { inherit system; }).fetchFromGitHub {
          rev = "stable_building"; # Right now we need to consume from stable building.
          owner = "getfloresta";
          repo = "floresta";
          sha256 = "sha256-N9QC0N0rCr+9pgp9wtcKT38/3jzNdOE8IaixOWwvg98=";
        };
        # Tells to execute floresta tests after building.
        doCheck = false;
        # You can even pass some build inputs.
        extraBuildInputs = [
          pkgs.mySpecificDep
        ]
      };
    in
    {
      # You can try the florestaBuild method by `nix build`.
      packages.${system}.default = custom-floresta;

      devShells.${system}.default = (import inputs.nixpkgs { inherit system; }).mkShell {

        nativeBuildInputs = [
          floresta
          florestad
          floresta-cli
        ];

        shellHook = ''
          echo "floresta @: ${floresta}"
          echo "florestad @: ${florestad}"
          echo "floresta-cli @: ${floresta-cli}"

          echo "Try running it!"
          echo ""
          echo "$ florestad"

          echo "$ floresta-cli getblockchaininfo"
        '';
      };
    };
}
```

### With flakes

From the root source of this project you have the following alternatives to build using flakes:

```Bash
# The default building derivation, build all the components this project
# provide: florestad, floresta-cli and libfloresta.
$ nix build

$ ls ./result/bin/
floresta-cli  florestad

$ ls ./result/lib/
libfloresta.a  libfloresta_chain.so  libfloresta.so
```

You can specify the component to build a derivation only for it.

```Bash
# The available options are: florestad, floresta-cli and libfloresta
$ nix build .#florestad

$ ls ./result/bin
florestad
```

It's recommended to use the default build command.

## Dev environments with devshells

This project offers devshells that offers git hooks and all dependencies needed.

```Bash
# The default devshell which include githooks and project dependencies.
$ nix develop

# `python-deps` carries all what is needed to run the python tests.
$ nix develop .#python-deps

```

What the `python-deps` devshell provides:

- `uv`
- `python`
- `black` fmt git hook
- `utreexod` included in `$PATH` and linked in `$FLORESTA_TEMP_DIR`
- `florestad` included in `$PATH` and linked in `$FLORESTA_TEMP_DIR`

You can find more information about our tests in [running tests](./running-tests.md)
