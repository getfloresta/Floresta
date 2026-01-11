# Building on Unix

## Prerequisites
```bash
sudo apt update
apt-get update && apt-get install -y \
    build-essential \
    cmake \
    curl \
    clang \
    libclang-dev \
    git \
    libssl-dev \
    pkg-config \
    libboost-all-dev
```

You'll need Rust and Cargo, refer to [this](https://www.rust-lang.org/) for more details. Minimum support version is rustc 1.81 and newer.

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

If you are using Nix, you can get `floresta` packages into your system following the instructions [here](nix.md).
