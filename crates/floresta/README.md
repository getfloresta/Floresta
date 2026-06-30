# floresta

<p>
    <a href="https://crates.io/crates/floresta"><img src="https://img.shields.io/crates/v/floresta.svg"/></a>
    <a href="https://docs.rs/floresta"><img src="https://img.shields.io/badge/docs.rs-floresta-green"/></a>
    <a href="https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/"><img src="https://img.shields.io/badge/rustc-1.85.0%2B-orange.svg?label=MSRV"/></a>
    <a href="https://github.com/getfloresta/Floresta/blob/master/LICENSE.md"><img src="https://img.shields.io/badge/License-MIT%2FApache--2.0-red.svg"/></a>
</p>

`floresta` is a modular and extensible framework for building Utreexo-based Bitcoin nodes.

## Getting Started

This crate provides convenient re-exports for all of Floresta's crates, which can be used to
build lightweight Bitcoin applications.

This crate has no default features: all re-exported crates are granularly locked behind feature
flags, in order to allow consumers to only pull in what's needed for their specific use case.
In addition, each individual crate's features are also locked behind a feature flag.

Usage examples are available under the [`examples`](./examples) directory.

## Re-exported Crates

| Crate                         | Feature Flag      | Description                                                  |
|:------------------------------|:------------------|:-------------------------------------------------------------|
| [`floresta-chain`]            | `chain`           | Chain state, validation, accumulator, and block processing   |
| [`floresta-common`]           | `common`          | Shared primitives used across Floresta crates                |
| [`floresta-compact-filters`]  | `compact-filters` | Compact Block Filter storage and querying                    |
| [`floresta-domain`]           | `domain`          | Domain traits and types for composing node components        |
| [`floresta-electrum`]         | `electrum`        | Electrum server implementation backed by Floresta components |
| [`floresta-mempool`]          | `mempool`         | Transaction mempool and policy logic                         |
| [`floresta-metrics`]          | `metrics`         | Prometheus metrics registry and exporter                     |
| [`floresta-node`]             | `node`            | High-level full node orchestration                           |
| [`floresta-rpc`]              | `rpc`             | RPC method traits, data types, and JSON-RPC client           |
| [`floresta-watch-only`]       | `watch-only`      | Watch-only wallet indexing and storage                       |
| [`floresta-wire`]             | `wire`            | Bitcoin P2P networking and wire protocol                     |

## Forwarded Features

| Crate                   | Feature Flag                 | Enables                               |
|:------------------------|:-----------------------------|:--------------------------------------|
| [`floresta-chain`]      | `chain-bitcoinkernel`        | `floresta-chain/bitcoinkernel`        |
| [`floresta-chain`]      | `chain-flat-chainstore`      | `floresta-chain/flat-chainstore`      |
| [`floresta-chain`]      | `chain-metrics`              | `floresta-chain/metrics`              |
| [`floresta-common`]     | `common-std`                 | `floresta-common/std`                 |
| [`floresta-node`]       | `node-compact-filters`       | `floresta-node/compact-filters`       |
| [`floresta-node`]       | `node-json-rpc`              | `floresta-node/json-rpc`              |
| [`floresta-node`]       | `node-metrics`               | `floresta-node/metrics`               |
| [`floresta-node`]       | `node-zmq-server`            | `floresta-node/zmq-server`            |
| [`floresta-rpc`]        | `rpc-clap`                   | `floresta-rpc/clap`                   |
| [`floresta-rpc`]        | `rpc-jsonrpc`                | `floresta-rpc/with-jsonrpc`           |
| [`floresta-watch-only`] | `watch-only-memory-database` | `floresta-watch-only/memory-database` |
| [`floresta-watch-only`] | `watch-only-std`             | `floresta-watch-only/std`             |
| [`floresta-wire`]       | `wire-metrics`               | `floresta-wire/metrics`               |

## Minimum Supported Rust Version

This library should compile with any combination of features on Rust 1.85.0.

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

[`floresta-chain`]: ../floresta-chain
[`floresta-common`]: ../floresta-common
[`floresta-compact-filters`]: ../floresta-compact-filters
[`floresta-domain`]: ../floresta-domain
[`floresta-electrum`]: ../floresta-electrum
[`floresta-mempool`]: ../floresta-mempool
[`floresta-metrics`]: ../floresta-metrics
[`floresta-node`]: ../floresta-node
[`floresta-rpc`]: ../floresta-rpc
[`floresta-watch-only`]: ../floresta-watch-only
[`floresta-wire`]: ../floresta-wire
