// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::PathBuf;

use bitcoin::Network;
use clap::Parser;
use floresta_rpc::jsonrpc_client::Client;
use floresta_rpc::rpc_interfaces::RpcCommand;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = Client::new(get_host(&cli));
    let res = client.dispatch(cli.command)?;
    println!("{}", serde_json::to_string_pretty(&res)?);
    Ok(())
}

fn get_host(cmd: &Cli) -> String {
    if let Some(host) = cmd.rpc_host.clone() {
        return host;
    }

    // TODO(@luisschwab): use `NetworkExt` to append the correct port
    // once https://github.com/rust-bitcoin/rust-bitcoin/pull/4639 makes it into a release.
    match cmd.network {
        Network::Bitcoin => "http://127.0.0.1:8332".into(),
        Network::Signet => "http://127.0.0.1:38332".into(),
        Network::Testnet => "http://127.0.0.1:18332".into(),
        Network::Testnet4 => "http://127.0.0.1:48332".into(),
        Network::Regtest => "http://127.0.0.1:18442".into(),
    }
}

#[derive(Debug, Parser)]
#[command(author = "Davidson Souza", version = env!("CARGO_PKG_VERSION"), about = r#"
    A simple command line interface to the Floresta JSON RPC interface.
"#, long_about = None)]
pub struct Cli {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    pub config_file: Option<PathBuf>,
    /// Which network should we use
    #[arg(short, long, default_value_t=Network::Bitcoin)]
    pub network: Network,
    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,
    /// The RPC host to connect to
    #[arg(short = 'H', long, value_name = "URL")]
    pub rpc_host: Option<String>,
    /// The RPC username to use
    #[arg(short = 'u', long, value_name = "USERNAME")]
    pub rpc_user: Option<String>,
    /// The RPC password to use
    #[arg(short = 'P', long, value_name = "PASSWORD")]
    pub rpc_password: Option<String>,
    /// An actual RPC command to run
    #[command(subcommand)]
    pub command: RpcCommand,
}
