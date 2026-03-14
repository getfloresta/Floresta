# Tests

This document is a guide for the different testing options available in Floresta. We have an extensive suite of Rust tests, as well as a functional tests Python framework, found in the [tests directory](../tests). For fuzzing tests refer to [this document](fuzzing.md).

## Requirements

The tests in `floresta-cli` depend on the compiled `florestad` binary. Make sure to build the entire project first by running:

```bash
cargo build
```

The functional tests also need some dependencies, we use python for writing them and `uv` to manage its dependencies.

### Dependencies requirements to run functional tests

The functional tests will build Bitcoin Core, Utreexo and Floresta in order to make integration testing. To do so it will use some dependencies.

The following guide is a compilation taken from [Bitcoin](https://github.com/bitcoin/bitcoin/tree/master/doc) and [Utreexo](https://github.com/utreexo/utreexod/). It considers the user running the tests already has the required dependencies for building [Floresta](https://github.com/getfloresta/Floresta/tree/master/doc).

#### Ubuntu & Debian

```bash
sudo apt-get install build-essential cmake pkgconf python3 libevent-dev libboost-dev golang
```

#### Fedora

```bash
sudo dnf install gcc-c++ cmake make python3 libevent-devel boost-devel golang
```

#### MacOS

```bash
brew install cmake boost pkgconf libevent coreutils go
```

#### Installing UV

UV is an extremely fast Python package and project manager, written in Rust.

```bash
# On macOS and Linux.
curl -LsSf https://astral.sh/uv/install.sh | sh
```

## Testing Options

There's a set of unit and integration Rust tests that you can run with:

```bash
cargo test
```

For the full test suite, including long-running tests, use:

```bash
cargo test --release
```

Next sections will cover the Python functional tests.

### Setting Functional Tests Binaries

We provide three way for running functional tests:

- from `just` tool that abstracts what is necessary to run the tests before doing a commit;
- from the helper script — [run_functional.sh](https://github.com/vinteumorg/Floresta/blob/master/tests/run_functional.sh) — to automatically build and run the tests;
- from python utility directly: the most laborious, but you can run a specific test suite.

#### From `just` tool

It abstracts all things that will be explained in the next sections, and for that
reason, we recommend to use it before doing a commit when changes only the functional tests.

```bash
just test-functional
```

Furthermore, you can only specific tests, rather than all at once.

```bash
# runs all tests in 'floresta-cli' suite
just test-functional "--test-suite floresta-cli"

# same as above
just test-functional "-t floresta-cli"

# run the stop and ping tests in the floresta-cli suite
just test-functional "--test-suite floresta-cli --test-name stop --test-name ping"

# same as above
just test-functional "-t floresta-cli -k stop -k ping"

# run many tests that start with the word `getblock` (getblockhash, getblockheader, etc...)
just test-functional "-t floresta-cli -k getblock"
```

#### From the test runner directly

The test runner handles everything: checking dependencies, building binaries, and running the suite.

Basic usage:

```bash
uv run tests/test_runner.py
```

##### Utreexod

By default, the runner will build `utreexod` at the `v0.4.0` tag.
If you want to build a specific release, set the `UTREEXOD_REVISION` environment variable.
It must be a [valid tag](https://github.com/utreexo/utreexod/tags). For example:

```bash
UTREEXOD_REVISION=v0.3.0 uv run tests/test_runner.py
```

##### Bitcoin-core

By default, the runner will download a prebuilt `bitcoind` (v30.2). If you want to use a different version, configure it with the `BITCOIN_REVISION` environment variable. Also, if you need to change the number of CPU cores for source builds, use
`BUILD_BITCOIND_NPROCS`. For example:

```bash
BITCOIN_REVISION=28.0 BUILD_BITCOIND_NPROCS=2 uv run tests/test_runner.py
```

Additionally, you can use some flags:

```bash
uv run tests/test_runner.py --force-rebuild --preserve-data-dir
```

The `--force-rebuild` flag will force the runner to rebuild all binaries even if they are already present.
The `--release` flag will build florestad in release mode (default is debug).
The `--preserve-data-dir` flag will keep the data and logs directories after running the tests
(this is useful if you want to keep the data for debugging purposes).

Furthermore, you can run a set of specific tests, rather than all at once.

```bash
# runs all tests in 'floresta-cli' suite
uv run tests/test_runner.py --test-suite floresta-cli

# same as above
uv run tests/test_runner.py -t floresta-cli

# run the stop and ping tests in the floresta-cli suite
uv run tests/test_runner.py --test-suite floresta-cli --test-name stop --test-name ping

# same as above
uv run tests/test_runner.py -t floresta-cli -k stop -k ping

# run many tests that start with the word `getblock` (getblockhash, getblockheader, etc...)
uv run tests/test_runner.py -t floresta-cli -k getblock
```

#### How the setup works

When you run `uv run tests/test_runner.py`, the runner automatically prepares
everything before executing the test suite. The setup resolves a working directory
(`FLORESTA_TEMP_DIR`), then obtains three binaries: **florestad**, **utreexod**,
and **bitcoind**. Each binary is resolved using a three-tier fallback strategy:

1. **Environment variable** — if `BITCOIND_EXE` or `UTREEXOD_EXE` is set, the
   runner copies that executable directly into the binaries directory.
2. **Prebuilt download** — the runner downloads a prebuilt binary from the
   project's official release page and verifies its SHA256 checksum.
3. **Source build** — as a last resort, the runner clones the repository and
   builds from source.

For **florestad**, the runner always builds from the local source tree via
`cargo build`. If a binary already exists in the target directory, it is skipped
unless `--force-rebuild` is passed.

All binaries are placed under `$FLORESTA_TEMP_DIR/binaries/`.

#### Manual setup

If you prefer to manage binaries yourself (e.g. from a Nix shell or CI cache),
you can skip the automatic setup entirely:

1. Pick a working directory and export it:

```bash
export FLORESTA_TEMP_DIR=/tmp/floresta-func-tests
mkdir -p "$FLORESTA_TEMP_DIR/binaries"
```

2. Place the three required binaries there:

```bash
# florestad — build from the local tree
cargo build --bin florestad
ln -sf "$(pwd)/target/debug/florestad" "$FLORESTA_TEMP_DIR/binaries/florestad"

# utreexod — use your own build or a prebuilt binary
cp /path/to/utreexod "$FLORESTA_TEMP_DIR/binaries/utreexod"

# bitcoind — use your own build or a prebuilt binary
cp /path/to/bitcoind "$FLORESTA_TEMP_DIR/binaries/bitcoind"
```

3. Run the tests. The runner will detect existing binaries and skip building:

```bash
uv run tests/test_runner.py
```

You can also point directly to external executables without copying:

```bash
BITCOIND_EXE=/usr/local/bin/bitcoind UTREEXOD_EXE=/usr/local/bin/utreexod \
  uv run tests/test_runner.py
```

#### Python development tools

- Recommended: install [uv: a rust-based python package and project manager](https://docs.astral.sh/uv/).

- Format code:

```bash
uv run black ./tests

# check only (no changes)
uv run black --check --verbose ./tests
```

- Lint code:

```bash
uv run pylint ./tests
```

#### Running individual test scripts

You can run a single test script directly. The framework will auto-resolve
`FLORESTA_TEMP_DIR` if it is not set, but the binaries must already exist
under `$FLORESTA_TEMP_DIR/binaries/`:

```bash
uv run tests/floresta-cli/ping.py
```

#### Clean up

Before each run the runner removes stale `data/` and `logs/` directories left
over from previous executions (including failed ones), so you always start from
a clean state.

On success the runner removes them again unless `--preserve-data-dir` is passed.
If you need to inspect artefacts from a failed run, re-run with
`--preserve-data-dir` to keep them around.

### Running/Developing Functional Tests with Nix

If you have nix, you can run the tests following the instructions [here](nix.md).
