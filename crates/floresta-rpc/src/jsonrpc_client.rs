// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;

use serde::Deserialize;

use crate::rpc::JsonRPCClient;

// Define a Client struct that wraps a jsonrpc::Client
#[derive(Debug)]
pub struct Client(jsonrpc::Client);

// Configuration struct for JSON-RPC client
pub struct JsonRPCConfig {
    pub url: String,
    pub user: Option<String>,
    pub pass: Option<String>,
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
        let client =
            jsonrpc::Client::simple_http(&config.url, config.user.clone(), config.pass.clone())
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
