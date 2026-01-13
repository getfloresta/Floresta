[![Docker Publish][docker-badge]][docker-url]
[![Functional][functional-badge]][functional-url]
[![Rust][rust-badge]][rust-url]
[![Docs][docs-badge]][docs-url]
[![License][license-badge]][license-url]

[docker-badge]: https://github.com/vinteumorg/Floresta/actions/workflows/docker.yml/badge.svg
[docker-url]: https://github.com/vinteumorg/Floresta/actions/workflows/docker.yml
[functional-badge]: https://github.com/vinteumorg/Floresta/actions/workflows/functional.yml/badge.svg
[functional-url]: https://github.com/vinteumorg/Floresta/actions/workflows/functional.yml
[rust-badge]: https://github.com/vinteumorg/Floresta/actions/workflows/rust.yml/badge.svg
[rust-url]: https://github.com/vinteumorg/Floresta/actions/workflows/rust.yml
[docs-badge]: https://img.shields.io/badge/docs-floresta-green
[docs-url]: https://docs.getfloresta.sh/floresta/
[license-badge]: https://img.shields.io/badge/license-MIT-blue
[license-url]: https://github.com/vinteumorg/Floresta/blob/master/LICENSE

# Floresta

Floresta is a lightweight and embeddable Bitcoin client designed for users and applications that want strong validation guarantees without the operational overhead of traditional full nodes.

It can be run as a standalone fully validating node or embedded as a library, allowing developers to reuse the same client components across different applications and deployments.

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
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details

## Acknowledgments
* [Utreexo](https://eprint.iacr.org/2019/611)
    * [BIPs 181](https://github.com/bitcoin/bips/blob/master/bip-0181.mediawiki)
    * [BIPs 182](https://github.com/bitcoin/bips/blob/master/bip-0182.mediawiki)
    * [BIPs 183](https://github.com/bitcoin/bips/blob/master/bip-0183.mediawiki)
* [Bitcoin Core](https://github.com/bitcoin/bitcoin)
* [Rust Bitcoin](https://github.com/rust-bitcoin/rust-bitcoin)
* [Rust Miniscript](https://github.com/rust-bitcoin/rust-miniscript)

## Consensus Implementation

One of the most challenging parts of working with Bitcoin is keeping up with the consensus rules. Given its nature as a consensus protocol, it's very important to make sure that the implementation is correct. Instead of reimplementing a Script interpreter, we use [`rust-bitcoinconsensus`](https://github.com/rust-bitcoin/rust-bitcoinconsensus/) to verify transactions. This is a bind around a shared library that is part of Bitcoin Core. This way, we can be sure that the consensus rules are the same as Bitcoin Core, at least for scripts.

Although tx validation is arguably the hardest part in this process. This integration can be further improved by using `libbitcoinkernel`, that will increase the scope of `libbitcoinconsensus` to outside scripts, but this is still a work in progress.
