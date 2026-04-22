// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::rpc::JsonRPCClient;

// Define a Client struct that wraps a jsonrpc::Client
#[derive(Debug)]
pub struct Client(jsonrpc::Client);

/// Authentication method for the JSON-RPC client.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    UserPass { user: String, pass: String },
    CookieFile(String),
    None,
}

impl AuthMethod {
    /// Resolve the authentication method into `(user, pass)`,
    /// returning `(None, None)` when no authentication is required.
    ///
    /// # Errors
    /// Returns an error string if a cookie file path is provided but the file
    /// cannot be read or parsed.
    pub fn resolve(self) -> Result<(Option<String>, Option<String>), String> {
        match self {
            AuthMethod::UserPass { user, pass } => Ok((Some(user), Some(pass))),
            AuthMethod::CookieFile(path) => {
                let contents = fs::read_to_string(Path::new(&path))
                    .map_err(|e| format!("Failed to read cookie file '{path}': {e}"))?;
                let line = contents
                    .lines()
                    .next()
                    .ok_or_else(|| format!("Cookie file '{path}' is empty"))?;
                let mut parts = line.splitn(2, ':');
                let user = parts
                    .next()
                    .ok_or_else(|| format!("Malformed cookie file '{path}'"))?
                    .to_string();
                let pass = parts
                    .next()
                    .ok_or_else(|| {
                        format!("Malformed cookie file '{path}': missing ':' separator")
                    })?
                    .to_string();
                Ok((Some(user), Some(pass)))
            }
            AuthMethod::None => Ok((None, None)),
        }
    }
}

// Configuration struct for JSON-RPC client
pub struct JsonRPCConfig {
    pub url: String,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub cookie_file: Option<String>,
}

impl Client {
    // Constructor to create a new Client with a URL
    pub fn new(url: String) -> Self {
        let client =
            jsonrpc::Client::simple_http(&url, None, None).expect("Failed to create client");
        Self(client)
    }

    // Constructor to create a new Client with a configuration
    pub fn new_with_config(config: JsonRPCConfig) -> Self {
        let (user, pass) = if let Some(cookie_path) = config.cookie_file {
            AuthMethod::CookieFile(cookie_path)
                .resolve()
                .expect("Failed to read cookie file")
        } else {
            (config.user.clone(), config.pass.clone())
        };
        let client =
            jsonrpc::Client::simple_http(&config.url, user, pass).expect("Failed to create client");
        Self(client)
    }

    /// Constructor to create a new Client using a cookie file.
    pub fn new_with_cookie(url: String, cookie_file: String) -> Self {
        let (user, pass) = AuthMethod::CookieFile(cookie_file)
            .resolve()
            .expect("Failed to read cookie file");
        let client =
            jsonrpc::Client::simple_http(&url, user, pass).expect("Failed to create client");
        Self(client)
    }

    /// Constructor to create a new Client with explicit username and password.
    pub fn new_with_auth(url: String, user: String, pass: String) -> Self {
        let client = jsonrpc::Client::simple_http(&url, Some(user), Some(pass))
            .expect("Failed to create client");
        Self(client)
    }

    // Method to make an RPC call
    pub fn rpc_call<Response>(
        &self,
        method: &str,
        params: &[serde_json::Value],
    ) -> Result<Response, crate::rpc_types::Error>
    where
        Response: for<'a> serde::de::Deserialize<'a> + Debug,
    {
        // Serialize parameters to raw JSON value
        let raw = serde_json::value::to_raw_value(params)?;
        // Build the RPC request
        let req = self.0.build_request(method, Some(&*raw));
        // Send the request and handle the response
        let resp = self
            .0
            .send_request(req)
            .map_err(crate::rpc_types::Error::from);

        // Deserialize and return the result
        Ok(resp?.result()?)
    }
}

// Implement the JsonRPCClient trait for Client
impl JsonRPCClient for Client {
    fn call<T: for<'a> serde::de::Deserialize<'a> + Debug>(
        &self,
        method: &str,
        params: &[serde_json::Value],
    ) -> Result<T, crate::rpc_types::Error> {
        self.rpc_call(method, params)
    }
}

// Struct to represent a JSON-RPC response
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse<Res> {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<Res>,
    pub error: Option<serde_json::Value>,
}
