// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::FlorestadError;

#[derive(Default, Debug, Deserialize)]
pub struct Wallet {
    pub xpubs: Option<Vec<String>>,
    pub descriptors: Option<Vec<String>>,
    pub addresses: Option<Vec<String>>,
}

#[cfg(feature = "json-rpc")]
#[derive(Default, Debug, Deserialize)]
pub struct Rpc {
    pub user: Option<String>,
    pub password: Option<String>,
}

#[derive(Default, Debug, Deserialize)]
pub struct ConfigFile {
    pub wallet: Wallet,
    #[cfg(feature = "json-rpc")]
    #[serde(default)]
    pub rpc: Rpc,
}

impl ConfigFile {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, FlorestadError> {
        let config_file = fs::read_to_string(path.as_ref())?;

        Ok(toml::from_str(&config_file)?)
    }
}
