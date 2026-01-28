// SPDX-License-Identifier: MIT

//! A Utreexo-based Bitcoin transaction mempool.
//!
//! This crate provides a transaction mempool implementation specifically designed for
//! [Utreexo](https://eprint.iacr.org/2019/611.pdf) nodes. Unlike traditional Bitcoin nodes
//! that maintain a complete UTXO set, Utreexo nodes use a compact cryptographic accumulator
//! to verify transaction validity, significantly reducing storage requirements.
//!
//! # Overview
//!
//! The mempool serves as a holding area for unconfirmed transactions, performing several
//! critical functions:
//!
//! - **Transaction validation**: Verifies Utreexo inclusion proofs for transaction inputs
//! - **Proof management**: Maintains a local accumulator to generate proofs for relay and mining
//! - **Block template construction**: Assembles candidate blocks for miners
//! - **Transaction relay**: Tracks which transactions to broadcast to peers

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]

pub mod mempool;

pub use mempool::Mempool;
