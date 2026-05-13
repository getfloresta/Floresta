// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::Deserialize;

#[derive(Default, Debug, Deserialize)]
pub struct Wallet {
    pub xpubs: Option<Vec<String>>,
    pub descriptors: Option<Vec<String>>,
    pub addresses: Option<Vec<String>>,
}

#[derive(Default, Debug, Deserialize)]
pub struct ConfigFile {
    pub wallet: Wallet,
    /// Optional auth token for RPC access. If not set, a random token
    /// is generated and saved to `<data_dir>/.cookie` on startup,
    /// following Bitcoin Core's cookie auth model.
    pub rpc_auth_token: Option<String>,
    /// Optional auth token for Electrum protocol access.
    /// If not set, electrum connections are allowed without auth.
    pub electrum_auth_token: Option<String>,
}

impl ConfigFile {
    pub fn from_file(filename: &str) -> Result<Self, FlorestadError> {
        let file = std::fs::read_to_string(filename)?;
        Ok(toml::from_str(&file)?)
    }
}
