// SPDX-License-Identifier: MIT

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]
#![allow(clippy::manual_is_multiple_of)]

use serde::Deserialize;
use serde::Serialize;

pub mod electrum_protocol;
pub mod error;
pub mod request;

#[derive(Debug, Deserialize, Serialize)]
pub struct TransactionHistoryEntry {
    height: u32,
    tx_hash: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct MempoolTransaction {
    height: u32,
    tx_hash: String,
    fee: u32,
}
