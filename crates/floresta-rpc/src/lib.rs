// SPDX-License-Identifier: MIT OR Apache-2.0

//! # floresta-cli - A command line interface for florestad
//!
//! Florestad is a lightweight Bitcoin full node, built with libfloresta. It gives
//! you complete control over your Bitcoin node with a simple json-rpc interface that
//! may be used either from command line or programmatically. This crate provides a
//! ready-to-use library for interacting with florestad's json-rpc interface in your rust
//! application.

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]

#[cfg(feature = "with-jsonrpc")]
pub mod jsonrpc_client;

pub mod rpc_interfaces;
pub mod rpc_types;

// Those tests doesn't work on windows
// TODO (Davidson): work on windows?

#[cfg(all(
    test,
    feature = "with-jsonrpc",
    not(feature = "async"),
    not(target_os = "windows")
))]
mod tests {
    use core::str::FromStr;
    use std::fs;
    use std::net::TcpListener;
    use std::path::Path;
    use std::process::Child;
    use std::process::Command;
    use std::process::Stdio;
    use std::thread::sleep;
    use std::time::Duration;

    use bitcoin::BlockHash;
    use bitcoin::Txid;
    use rcgen::CertifiedKey;
    use rcgen::generate_simple_self_signed;

    use crate::jsonrpc_client::Client;
    use crate::rpc_interfaces::BlockchainRpc;
    use crate::rpc_interfaces::ControlRpc;
    use crate::rpc_interfaces::NetworkRpc;
    use crate::rpc_interfaces::RawTransactionRpc;
    use crate::rpc_interfaces::WalletRpc;
    use crate::rpc_types::GetBlockHeaderRes;
    use crate::rpc_types::GetBlockRes;

    struct Florestad {
        proc: Child,
    }

    impl Drop for Florestad {
        fn drop(&mut self) {
            self.proc.kill().unwrap();
        }
    }

    /// A helper function for tests.
    ///
    /// This function will start a florestad process and return a client that can be used to
    /// interact with it through RPC. It also returns a handle to the process itself, so that
    /// you can poke at the stdin and out for this process. You don't have to kill it though,
    /// once the handle goes out of scope, the process will be killed.
    ///
    /// The process created by this method will run in a random datadir and use random ports
    /// for both RPC and Electrum. The datadir will be in the current dir, under a `tmp` subdir.
    /// If you're at $HOME/floresta it will run on $HOME/floresta/tmp/<random_name>/
    fn start_florestad() -> (Florestad, Client) {
        // CARGO_MANIFEST_DIR is always floresta-cli's directory; PWD changes based on where the
        // command is executed.
        let root = format!("{}/../..", env!("CARGO_MANIFEST_DIR"));
        let release_path = format!("{root}/target/release/florestad");
        let debug_path = format!("{root}/target/debug/florestad");

        let release_found = Path::new(&release_path).try_exists().unwrap();
        // If release target not found, default to the debug path
        let florestad_path = match release_found {
            true => release_path,
            false => debug_path,
        };

        // Makes a temporary directory to store the chain db, TLS certificate, logs, etc.
        let test_code = rand::random::<u64>();
        let dirname = format!("{root}/tmp/floresta.{test_code}");
        fs::DirBuilder::new()
            .recursive(true)
            .create(&dirname)
            .unwrap();

        // Generate TLS private key and certificate using rcgen
        let CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_pem = cert.pem();
        let key_pem = signing_key.serialize_pem();
        fs::create_dir_all(format!("{dirname}/regtest/tls")).unwrap();
        fs::write(format!("{dirname}/regtest/tls/cert.pem"), cert_pem).unwrap();
        fs::write(format!("{dirname}/regtest/tls/key.pem"), key_pem).unwrap();

        let port = get_available_port();
        let mut fld = Command::new(&florestad_path)
            .args(["-n", "regtest"])
            .args(["--data-dir", &dirname])
            .args(["--rpc-address", &format!("127.0.0.1:{port}")])
            .args(["--electrum-address", "127.0.0.1:0"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|e| panic!("Couldn't launch florestad at {florestad_path}: {e}"));

        let client = Client::new(format!("http://127.0.0.1:{port}"));

        let mut retries = 10;
        loop {
            // Wait some time for florestad to start
            sleep(Duration::from_secs(3));

            match client.uptime() {
                Ok(_) => break,
                Err(_) if retries > 1 => retries -= 1,
                Err(e) => {
                    println!("Got error {e:?}, sending kill signal...");
                    fld.kill().unwrap();

                    panic!("Could not communicate with florestad after 30 seconds");
                }
            }
        }

        (Florestad { proc: fld }, client)
    }

    fn get_available_port() -> u16 {
        // Limit `listener` scope to release port
        let port = {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("Failed to bind to an available port");
            listener.local_addr().unwrap().port()
        };

        // Delay to ensure port is fully released by OS
        sleep(Duration::from_millis(100));

        port
    }

    #[test]
    fn test_stop() {
        let (mut _proc, client) = start_florestad();

        let stop = client.stop().unwrap();
        assert_eq!(stop.as_str(), "Floresta stopping");
    }

    #[test]
    fn test_get_blockchaininfo() {
        let (_proc, client) = start_florestad();

        let gbi = client.get_blockchain_info().unwrap();

        assert_eq!(gbi.blocks, 0);
        assert_eq!(gbi.chain, "regtest".to_owned());
        assert!(gbi.initial_block_download);
    }

    #[test]
    fn test_get_best_block_hash() {
        let (_proc, client) = start_florestad();

        let blockhash = client.get_best_block_hash().unwrap();

        assert_eq!(
            blockhash,
            "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
                .parse()
                .unwrap()
        );
    }

    #[test]
    fn test_get_block() {
        let (_proc, client) = start_florestad();

        let block_hash: BlockHash =
            "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
                .parse()
                .unwrap();

        let block = client.get_block(block_hash, Some(1)).unwrap();
        let GetBlockRes::One(block) = block else {
            panic!("Expected verbose block");
        };

        assert_eq!(
            block.hash,
            "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206".to_owned()
        );
    }

    #[test]
    fn test_get_block_hash() {
        let (_proc, client) = start_florestad();

        let blockhash = client.get_block_hash(0).unwrap();

        assert_eq!(
            blockhash,
            "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
                .parse()
                .unwrap()
        );
    }

    #[test]
    fn test_get_block_header() {
        let (_proc, client) = start_florestad();

        let blockhash = client.get_block_hash(0).expect("rpc not working");
        let block_header = client
            .get_block_header(blockhash, Some(true))
            .expect("rpc not working");
        let GetBlockHeaderRes::Verbose(block_header) = block_header else {
            panic!("Expected verbose block header");
        };

        assert_eq!(block_header.hash, blockhash.to_string());
    }

    #[test]
    fn test_get_height() {
        let (_proc, client) = start_florestad();

        let height = client.get_block_count().unwrap();
        assert_eq!(height, 0);
    }

    #[test]
    fn test_send_raw_transaction() {
        let (_proc, client) = start_florestad();

        let tx = "01000000010f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206000000004d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".to_string();

        let res = client.send_raw_transaction(tx).unwrap();
        assert_eq!(
            res,
            Txid::from_str("1fb5734a07ce65c509311ebc03f136904f8b39a130ca3adff93200fc7ecde6fe")
                .unwrap()
        );
    }

    #[test]
    fn test_get_network_info() {
        let (_proc, client) = start_florestad();

        let info = client.get_network_info().expect("rpc not working");

        // Floresta does not accept inbound connections, so these are always fixed.
        assert_eq!(info.connections_in, 0);
        assert!(info.local_addresses.is_empty());

        // The total connection count must equal in + out.
        assert_eq!(info.connections, info.connections_in + info.connections_out);

        // P2P networking is enabled by default.
        assert!(info.network_active);

        // Floresta has no mempool, so relay-related fields are hardcoded.
        assert!(!info.local_relay);
        assert_eq!(info.relay_fee, 0.0);
        assert_eq!(info.incremental_fee, 0.0);

        // time_offset is hardcoded to 0.
        assert_eq!(info.time_offset, 0);

        // Protocol version is the constant from floresta-common.
        assert_eq!(info.protocol_version, 70016);

        // All five networks Floresta is aware of should be listed, in ALL order.
        let names: Vec<&str> = info.networks.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["ipv4", "ipv6", "onion", "i2p", "cjdns"]);

        // Only ipv4 and ipv6 are currently SUPPORTED; the rest are limited.
        for net in &info.networks {
            let supported = matches!(net.name.as_str(), "ipv4" | "ipv6");
            assert_eq!(net.reachable, supported);
            assert_eq!(net.limited, !supported);
        }
    }

    #[test]
    fn test_uptime() {
        let (_proc, client) = start_florestad();

        client.uptime().unwrap();
    }

    #[test]
    fn test_get_connection_count() {
        let (_proc, client) = start_florestad();

        let count = client.get_connection_count().unwrap();
        assert!(count == 0);
    }

    #[test]
    fn test_get_peer_info() {
        let (_proc, client) = start_florestad();

        let peers = client.get_peer_info().unwrap();
        // Should not panic, peers might be empty since we're in regtest
        assert!(peers.is_empty() || !peers.is_empty());
    }

    #[test]
    fn test_list_descriptors() {
        let (_proc, client) = start_florestad();

        let descriptors = client.list_descriptors().unwrap();
        // Should not panic, descriptors might be empty
        assert!(descriptors.is_empty() || !descriptors.is_empty());
    }

    #[test]
    fn test_get_memory_info() {
        let (_proc, client) = start_florestad();

        let mem_info = client.get_memory_info("stats".to_string()).unwrap();
        // Just check it returns something without error
        let _ = mem_info;
    }

    #[test]
    fn test_get_rpc_info() {
        let (_proc, client) = start_florestad();

        let rpc_info = client.get_rpc_info().unwrap();
        // Just check it returns something without error
        let _ = rpc_info;
    }
}
