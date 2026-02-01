// SPDX-License-Identifier: MIT

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]

mod config_file;
mod error;
mod florestad;
#[cfg(feature = "json-rpc")]
mod json_rpc;
#[cfg(feature = "zmq-server")]
mod zmq;

pub use florestad::AssumeUtreexoValue;
pub use florestad::AssumeValidArg;
pub use florestad::Config;
pub use florestad::Florestad;
