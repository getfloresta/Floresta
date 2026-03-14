// SPDX-License-Identifier: MIT OR Apache-2.0

#![doc = include_str!("../../../README.md")]
// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]

/// Components to build a utreexo-aware, consensus enforcing Bitcoin node.
pub use floresta_chain as chain;
/// Useful data structures and traits used by the other crates.
pub use floresta_common as common;
#[cfg(feature = "electrum-server")]
/// An electrum server implementation
pub use floresta_electrum as electrum;
#[cfg(feature = "watch-only-wallet")]
/// A watch-only wallet implementation, optimized for electrum servers.
pub use floresta_watch_only as wallet;
/// The transport used to fetch network data.
pub use floresta_wire as wire;
