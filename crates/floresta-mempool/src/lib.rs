// SPDX-License-Identifier: MIT OR Apache-2.0

//! A Bitcoin transaction mempool for Floresta.
//!
//! This crate provides an in-memory holding area for unconfirmed transactions.
//! Today, transactions enter the mempool only when submitted locally — through the
//! `sendrawtransaction` RPC or an Electrum broadcast. Transactions announced by
//! peers are not accepted yet.
//!
//! # Overview
//!
//! The mempool currently performs the following functions:
//!
//! - **Acceptance**: applies context-free structural checks (non-empty inputs and
//!   outputs, script size limits, no duplicate inputs, output amounts in range)
//!   and rejects transactions that conflict with ones already held. It does *not*
//!   verify Utreexo proofs, scripts, or signatures, nor check that inputs exist
//!   or are unspent.
//! - **Dependency tracking**: records parent/child relationships between held
//!   transactions, used for conflict handling and block assembly.
//! - **Block template construction**: assembles candidate (unsolved) blocks from
//!   held transactions, parents before children, up to a weight limit.

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]

pub mod mempool;

pub use mempool::Mempool;
