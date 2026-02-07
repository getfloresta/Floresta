> \[!WARNING\]
> We are currently migrating from the old Utreexo P2P protocol to the new version described in [BIP-0183](https://github.com/bitcoin/bips/pull/1923).
> Since there are no bridge nodes running the new protocol on mainnet yet, Floresta can currently sync only on 
> signet. Mainnet support will return once the new protocol design and its implementation in both Floresta and
> [utreexod](https://github.com/utreexo/utreexod) are complete.

<div align="center">
  <h1>Floresta</h1>

  <img src="https://avatars.githubusercontent.com/u/249173822" width="220" />

  <p>
    <strong>A lightweight and embeddable Bitcoin client, built for sovereignty!</strong>
  </p>

  <p>
    <a href="https://github.com/getfloresta/Floresta/blob/master/LICENSE">
      <img alt="MIT Licensed" src="https://img.shields.io/github/license/getfloresta/floresta" />
    </a>
    <a href="https://github.com/getfloresta/Floresta/releases/latest">
      <img alt="Latest release" src="https://img.shields.io/github/v/release/getfloresta/floresta" />
    </a>
    <a href="https://github.com/getfloresta/Floresta/actions/workflows/functional.yml">
      <img alt="Functional Tests" src="https://github.com/getfloresta/Floresta/actions/workflows/functional.yml/badge.svg" />
    </a>
    <a href="https://docs.getfloresta.sh"><img alt="API Docs" src="https://img.shields.io/badge/docs-floresta-green" /></a>
    <a href="https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/">
      <img alt="Rustc Version 1.81.0+" src="https://img.shields.io/badge/rustc-1.81.0%2B-lightgrey.svg" />
    </a>
    <a href="https://hub.docker.com/r/dlsz/floresta">
      <img alt="Docker pulls" src="https://img.shields.io/docker/pulls/dlsz/floresta" />
    </a>
    <a href="https://github.com/getfloresta/Floresta/actions/workflows/docker.yml">
      <img alt="Docker Build Status" src="https://github.com/getfloresta/Floresta/actions/workflows/docker.yml/badge.svg" />
    </a>
    <a href="https://discord.gg/5Wj8fjjS93">
      <img alt="Chat on Discord" src="https://img.shields.io/discord/1185232004506198056" />
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

## Community

If you want to discuss this project, you can join the [Discord Server](https://discord.gg/5Wj8fjjS93).
To report security vulnerabilities, please see the [Security Policy](SECURITY.md).

## License

This project is licensed under the [MIT License](https://github.com/getfloresta/Floresta/blob/master/LICENSE).

## Acknowledgments

* [Utreexo](https://eprint.iacr.org/2019/611)
    * [BIPs 181](https://github.com/bitcoin/bips/blob/master/bip-0181.mediawiki)
    * [BIPs 182](https://github.com/bitcoin/bips/blob/master/bip-0182.mediawiki)
    * [BIPs 183](https://github.com/bitcoin/bips/blob/master/bip-0183.mediawiki)
* [Bitcoin Core](https://github.com/bitcoin/bitcoin)
* [Rust Bitcoin](https://github.com/rust-bitcoin/rust-bitcoin)
* [Rust Miniscript](https://github.com/rust-bitcoin/rust-miniscript)
* [Rust Bitcoin Kernel](https://github.com/TheCharlatan/rust-bitcoinkernel)
