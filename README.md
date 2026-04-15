<div align="center">
  <h1>Floresta</h1>

  <img src="https://avatars.githubusercontent.com/u/249173822" width="220" />

  <p>
    <strong>A lightweight and embeddable Bitcoin client, built for sovereignty!</strong>
  </p>

  <p>
    <a href="https://github.com/getfloresta/Floresta/releases/latest">
      <img alt="version" src="https://img.shields.io/github/v/release/getfloresta/floresta"/>
    </a>
    <a href="https://docs.getfloresta.sh">
      <img alt="API Docs" src="https://img.shields.io/badge/docs-floresta-brightgreen"/>
    </a>
    <a href="https://blog.rust-lang.org/2024/09/05/Rust-1.81.0/">
      <img alt="MSRV 1.81.0+" src="https://img.shields.io/badge/rustc-1.81.0%2B-orange.svg"/>
    </a>
    <a href="https://github.com/getfloresta/Floresta/blob/master/LICENSE.md">
      <img src="https://img.shields.io/badge/License-MIT%2FApache--2.0-red.svg"/>
    </a>
    <a href="https://github.com/getfloresta/Floresta/actions/workflows/rust.yml">
      <img src="https://github.com/getfloresta/Floresta/actions/workflows/rust.yml/badge.svg"/>
    </a>
    <a href="https://github.com/getfloresta/Floresta/actions/workflows/functional.yml">
      <img src="https://github.com/getfloresta/Floresta/actions/workflows/functional.yml/badge.svg"/>
    </a>
  </p>

  <h4>
    <a href="https://getfloresta.org">Homepage</a>
    <span> | </span>
    <a href="https://docs.getfloresta.sh">Documentation</a>
  </h4>
</div>

Floresta is a lightweight and embeddable Bitcoin client designed for users and applications that
want strong validation guarantees without the operational overhead of traditional full nodes.

It can be run as a standalone fully validating node or embedded as a library, allowing developers
to reuse the same client components across different applications and deployments.

## Name

Floresta is the Portuguese word for forest. It is a reference to the Utreexo accumulator,
which is a forest of Merkle trees. It's pronounced _/floˈɾɛstɐ/_.

## Architecture

Floresta is written in Rust and implements modern Bitcoin validation techniques such as
[Utreexo](https://eprint.iacr.org/2019/611),
[PoW Fraud Proofs](https://blog.dlsouza.lol/2023/09/28/pow-fraud-proof.html), and pruning,
to significantly reduce resource requirements while preserving trust and security.

Floresta is composed of two main components: `libfloresta` and `florestad`.

[`libfloresta`](https://github.com/getfloresta/Floresta/tree/master/crates) is a collection of
reusable components that can be integrated into Bitcoin applications.
[`florestad`](https://github.com/getfloresta/Floresta/tree/master/bin/florestad) builds on top of
[`libfloresta`](https://github.com/getfloresta/Floresta/tree/master/crates) to provide a full node
daemon, including a watch-only wallet and an Electrum server.

If you only want to run a node, you can use
[`florestad`](https://github.com/getfloresta/Floresta/tree/master/bin/florestad) by building it from
source, following the instructions for [Unix](doc/build-unix.md) or [MacOS](doc/build-macos.md).

## Consensus Implementation

One of the most challenging parts of working with Bitcoin is keeping up with the consensus rules.
Given its nature as a consensus protocol, it's very important to make sure that the implementation
is correct and on par with Bitcoin Core. Instead of reimplementing a Bitcoin Script interpreter,
we use [`rust-bitcoinkernel`](https://github.com/TheCharlatan/rust-bitcoinkernel/), which is a
wrapper around [`libbitcoinkernel`](https://github.com/bitcoin/bitcoin/issues/24303),
a C++ library that exposes Bitcoin Core's validation engine. It allows validating blocks,
transaction outputs and reading block data with the same API as Bitcoin Core.

## Developing

Detailed documentation for [`libfloresta`](https://github.com/getfloresta/Floresta/tree/master/crates)
is available [here](https://docs.getfloresta.sh/floresta/). Additionally, the
[floresta-docs](https://getfloresta.github.io/floresta-docs/) `mdBook` provides an
in-depth look at the libraries' architecture and internals.

Further information can be found in the [documentation folder](/doc).

Contributions are welcome. Feel free to open an issue or a pull request. Check out our
[Contribution Guidelines](CONTRIBUTING.md) for more information on best practices.

If you want to contribute but don't know where to start, take a look at the
[Good First Issues](https://github.com/getfloresta/Floresta/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22).

## Roadmap

Floresta's technical direction is guided by six strategic themes:

- Reliability & Security
- Sync & Transaction Relay
- Bitcoin Ecosystem Support
- Modular Architecture
- Testing & Validation
- Community Adoption & User Experience

For the full 2026 roadmap and ongoing priorities, see [doc/roadmap-2026.md](doc/roadmap-2026.md).
Active workstreams are tracked on the [Project Board](https://github.com/orgs/getfloresta/projects/2).

## Community

If you want to discuss this project, you can join the [Discord Server](https://discord.gg/5Wj8fjjS93).
To report security vulnerabilities, please see the [Security Policy](SECURITY.md).

## License

Licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)
- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)

at your option.

## Acknowledgments

* [Utreexo](https://eprint.iacr.org/2019/611)
  * [BIP-0181: Accumulator Specification](https://github.com/kcalvinalvin/bips/blob/2025-08-10-utreexo-bips/bip-0181.md)
  * [BIP-0182: Transaction and Block Validation](https://github.com/kcalvinalvin/bips/blob/2025-08-10-utreexo-bips/bip-0182.md)
  * [BIP-0183: Network Messages Specification](https://github.com/kcalvinalvin/bips/blob/2025-08-10-utreexo-bips/bip-0183.md)
* [Bitcoin Core](https://github.com/bitcoin/bitcoin)
* [Rust Bitcoin](https://github.com/rust-bitcoin/rust-bitcoin)
* [Rust Miniscript](https://github.com/rust-bitcoin/rust-miniscript)
* [Rust Bitcoin Kernel](https://github.com/TheCharlatan/rust-bitcoinkernel)
