# Building on macOS
The following steps should be executed in a Terminal application. Tip: press `Command (⌘) + Space` and search for `terminal`.

## Prerequisites
### 1. Xcode Command Line Tools

To install, run the following command from your terminal:

``` bash
xcode-select --install
```

Upon running the command, you should see a popup appear.
Click on `Install` to continue the installation process.

### 2. Homebrew Package Manager

Homebrew is a package manager for macOS that allows one to install packages from the command line easily. You can use the package manager of your preference.

To install the Homebrew package manager, see: https://brew.sh

Note: If you run into issues while installing Homebrew or pulling packages, refer to [Homebrew's troubleshooting page](https://docs.brew.sh/Troubleshooting).

### 3. Install Required Dependencies

On the Terminal, using Homebrew, run the following:
```bash
brew update
brew install gcc pkg-config
```
You'll need Rust and Cargo, refer to [this](https://www.rust-lang.org/) for more details. Minimum support version is rustc 1.74 and newer.

## Building

Once you have Cargo, clone the repository with:

```bash
git clone https://github.com/getfloresta/Floresta.git
```

go to the Floresta directory

```bash
cd Floresta/
```

and build with cargo build

```bash
cargo build --release

# Alternatively, you can add florestad and floresta-cli to the path with
cargo install --path ./bin/florestad --locked
cargo install --path ./bin/floresta-cli --locked
```

If you are using Nix, you can get floresta packages into your system following the instructions [here](nix.md).
