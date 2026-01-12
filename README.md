> [!WARNING]
> We are currently in the process of migrating the old, superseded P2P protocol for Utreexo with the new one as described
> by [BIP-183](https://github.com/bitcoin/bips/pull/1923). Due to not having bridge nodes running this new protocol on mainnet
> yet, Floresta is currently only able to sync on signet. We expect to have mainnet support back as soon as we finish the new
> protocol and implement it in both Floresta and [Utreexod](https://github.com/utreexo/utreexod).

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
    <a href="https://getfloresta.org">Project Homepage</a>
    <span> | </span>
    <a href="https://docs.getfloresta.sh">Documentation</a>
  </h4>
</div>

Floresta is a lightweight and embeddable Bitcoin client designed for users and applications that want strong validation guarantees without the operational overhead of traditional full nodes.

It can be run as a standalone fully validating node or embedded as a library, allowing developers to reuse the same client components across different applications and deployments.

## Name

Floresta is the Portuguese word for forest. It is a reference to the Utreexo accumulator, which is a forest of Merkle trees. It's pronounced /floˈɾɛstɐ/.

## Architecture

Floresta is composed of two main parts: `libfloresta` and `florestad`.

`libfloresta` is a collection of reusable components that can be integrated into Bitcoin applications. `florestad` builds on top of `libfloresta` to provide a full node daemon, including a watch-only wallet and an Electrum server.

If you only want to run a node, you can use `florestad` directly, either by building it from source using the instructions in the [documentation](/doc/README.md) or by downloading a pre-built binary from the [latest release](https://github.com/getfloresta/Floresta/releases/latest) page.

## Design

Floresta is written in Rust and implements modern Bitcoin validation techniques such as [Utreexo](https://eprint.iacr.org/2019/611), [PoW Fraud Proofs](https://blog.dlsouza.lol/2023/09/28/pow-fraud-proof.html), and pruning to significantly reduce resource requirements while preserving trust and security.

## For developers

Detailed documentation for `libfloresta` is available [here](https://docs.getfloresta.sh/floresta/). Additionally, the [floresta-docs](https://josesk999.github.io/floresta-docs/) mdBook provides an in-depth look at the libraries' architecture and internals.

Further information can be found in the [doc folder](/doc).

## Community

If you want to discuss this project, you can join our Discord server [here](https://discord.gg/5Wj8fjjS93). To report security vulnerabilities, please see our [Security Policy](SECURITY.md).

## Contributing

Contributions are welcome, feel free to open an issue or a pull request. Check out our [CONTRIBUTING.md](CONTRIBUTING.md) for more information on best practices and guidelines.

If you want to contribute but don't know where to start, take a look at the issues, there's a few of them marked as `good first issue`.

## License

This project is licensed under the MIT License - see the [LICENSE](https://github.com/getfloresta/Floresta/blob/master/LICENSE) file for details.

## Acknowledgments

* [Utreexo](https://eprint.iacr.org/2019/611)
* [Bitcoin Core](https://github.com/bitcoin/bitcoin)
* [Rust Bitcoin](https://github.com/rust-bitcoin/rust-bitcoin)
* [Rust Miniscript](https://github.com/rust-bitcoin/rust-miniscript)

## Consensus Implementation

One of the most challenging parts of working with Bitcoin is keeping up with the consensus rules. Given its nature as a consensus protocol, it's very important to make sure that the implementation is correct. Instead of reimplementing a Script interpreter, we use [`rust-bitcoinconsensus`](https://github.com/rust-bitcoin/rust-bitcoinconsensus/) to verify transactions. This is a bind around a shared library that is part of Bitcoin Core. This way, we can be sure that the consensus rules are the same as Bitcoin Core, at least for scripts.

Although tx validation is arguably the hardest part in this process. This integration can be further improved by using `libbitcoinkernel`, that will increase the scope of `libbitcoinconsensus` to outside scripts, but this is still a work in progress.
