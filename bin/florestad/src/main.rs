// SPDX-License-Identifier: MIT OR Apache-2.0

//! This is a modular-(ish) utreexo powered wallet backend and fully validating node, it's
//! developed as an experiment to showcase utreexo. This wallet also comes with an Electrum
//! server out-of-the-box, for people to try out with their favorite wallet.
//! This codebase consists of three main parts: a blockchain backend, that gets all information
//! we need from the network. An Electrum Server that talks full Electrum protocol and can be
//! used with any wallet that understands this protocol. Finally, it has the `AddressCache`,
//! a watch-only wallet that keeps track of your wallet's transactions.

// Coding conventions (lexicographically sorted)
#![deny(arithmetic_overflow)]
#![deny(clippy::all)]
#![deny(missing_docs)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(non_upper_case_globals)]

mod cli;
#[cfg(unix)]
mod daemonize;
mod logger;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;

use bitcoin::Network;
use clap::Parser;
use cli::Cli;
use floresta_node::Config;
use floresta_node::Florestad;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tokio::time::timeout;
use tracing::info;
use tracing::Level;

#[cfg(unix)]
use crate::daemonize::Daemon;
use crate::logger::start_logger;
use crate::logger::LOG_FILE;

fn main() {
    let params = Cli::parse();
    params.validate();

    // If not provided defaults to `$HOME/.floresta`. Uses a subdirectory for non-mainnet networks.
    let data_dir = data_dir_path(params.data_dir, params.network);

    // Create the data directory if it doesn't exist
    fs::create_dir_all(&data_dir).unwrap_or_else(|e| {
        eprintln!("Could not create data dir {data_dir:?}: {e}");
        exit(1);
    });

    let debug_log_file =
        resolve_debug_log_file(params.nodebuglogfile, params.debuglogfile.as_deref(), &data_dir);

    let config = Config {
        data_dir,
        disable_dns_seeds: params.connect.is_some() || params.disable_dns_seeds,
        network: params.network,
        debug: params.debug,
        cfilters: !params.no_cfilters,
        proxy: params.proxy,
        assume_utreexo: !params.no_assume_utreexo,
        connect: params.connect,
        wallet_xpub: params.wallet_xpub,
        config_file: params.config_file,
        #[cfg(unix)]
        log_to_stdout: !params.daemon,
        #[cfg(not(unix))]
        log_to_stdout: true,
        debug_log_file,
        assume_valid: params.assume_valid,
        #[cfg(feature = "zmq-server")]
        zmq_address: params.zmq_address,
        #[cfg(feature = "json-rpc")]
        json_rpc_address: params.rpc_address,
        generate_cert: params.generate_cert,
        wallet_descriptor: params.wallet_descriptor,
        filters_start_height: params.filters_start_height,
        user_agent: env!("USER_AGENT").to_owned(),
        assumeutreexo_value: None,
        electrum_address: params.electrum_address,
        enable_electrum_tls: params.enable_electrum_tls,
        electrum_address_tls: params.electrum_address_tls,
        tls_cert_path: params.tls_cert_path,
        tls_key_path: params.tls_key_path,
        allow_v1_fallback: params.allow_v1_fallback,
        backfill: !params.no_backfill,
    };

    #[cfg(unix)]
    if params.daemon {
        let mut daemon = Daemon::new(&config.data_dir);
        if let Some(pid_file) = params.pid_file {
            daemon = daemon.pid_file(pid_file);
        }

        daemon.fork().expect("failed to daemonize");
    }

    let log_level = match config.debug {
        true => Level::DEBUG,
        false => Level::INFO,
    };

    // The guard must stay alive until the end of `main` to flush file logs when dropped.
    let _logger_guard = start_logger(
        config.debug_log_file.as_deref(),
        config.log_to_stdout,
        log_level,
    );

    let _rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .max_blocking_threads(2)
        .thread_keep_alive(Duration::from_secs(60))
        .thread_name("florestad")
        .build()
        .unwrap();

    let signal = Arc::new(RwLock::new(false));
    let _signal = signal.clone();

    _rt.spawn(async move {
        // This is used to signal the runtime to stop gracefully.
        // It will be set to true when we receive a Ctrl-C or a stop signal.
        tokio::signal::ctrl_c().await.unwrap();
        let mut sig = signal.write().await;
        *sig = true;
    });

    let florestad = Florestad::from(config);
    _rt.block_on(async {
        florestad.start().await.unwrap_or_else(|e| {
            eprintln!("Failed to start florestad: {e}");
            exit(1);
        });

        // wait for shutdown
        loop {
            if florestad.should_stop().await || *_signal.read().await {
                info!("Stopping Floresta");
                florestad.stop().await;
                let _ = timeout(Duration::from_secs(10), florestad.wait_shutdown()).await;
                break;
            }

            sleep(Duration::from_secs(5)).await;
        }
    });

    // Drop `florestad` and the runtime.
    // They are dropped outside the async block to avoid a nested
    // drop of the runtime due to the RPC server, which panics.
    drop(florestad);
    drop(_rt);
    // Flush logs to the file system when dropped.
    drop(_logger_guard);
}

fn data_dir_path(dir: Option<String>, network: Network) -> String {
    // base dir: provided `dir` or $HOME/.floresta or "./.floresta"
    let mut base: PathBuf = dir
        .as_ref()
        .map(|s| s.trim_end_matches(['/', '\\']).into())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".floresta")
        });

    // network-specific subdir
    match network {
        Network::Bitcoin => {} // no subdir
        Network::Signet => base.push("signet"),
        Network::Testnet => base.push("testnet3"),
        Network::Testnet4 => base.push("testnet4"),
        Network::Regtest => base.push("regtest"),
    }

    base.to_string_lossy().into_owned()
}

/// Resolves the debug log file path from the CLI flags.
///
/// File logging is enabled by default (matching Bitcoin Core). The user can:
///   - Override the path with `--debuglogfile=<path>`
///   - Disable file logging entirely with `--nodebuglogfile`
///
/// Relative paths are prefixed by the net-specific `data_dir`.
/// Returns `None` when file logging is disabled, or `Some(absolute_path)` otherwise.
fn resolve_debug_log_file(
    no_debug_log_file: bool,
    debug_log_file: Option<&str>,
    data_dir: &str,
) -> Option<String> {
    if no_debug_log_file {
        return None;
    }

    let raw = debug_log_file.unwrap_or(LOG_FILE);
    let path = PathBuf::from(raw);
    let absolute = if path.is_absolute() {
        path
    } else {
        PathBuf::from(data_dir).join(path)
    };
    Some(absolute.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_dir_path() {
        let net = Network::Bitcoin;
        let default_expected = dirs::home_dir()
            .unwrap_or(PathBuf::from("."))
            .join(".floresta");

        assert_eq!(
            data_dir_path(None, net),
            default_expected.display().to_string(),
        );

        // Using other made-up directories
        let mut path = Some("path/to/dir".into());
        assert_eq!(data_dir_path(path, net), "path/to/dir");

        path = Some("path/to/dir/".into());
        assert_eq!(data_dir_path(path, net), "path/to/dir");

        // Test removing the '\' separator
        path = Some(format!("path{}", '\\'));
        assert_eq!(data_dir_path(path, net), "path");

        // Test removing many separators
        path = Some("path///".into());
        assert_eq!(data_dir_path(path, net), "path");

        // Using other networks
        for &(net, suffix) in &[
            (Network::Testnet, "testnet3"),
            (Network::Testnet4, "testnet4"),
            (Network::Signet, "signet"),
            (Network::Regtest, "regtest"),
        ] {
            let expected = PathBuf::from("path").join(suffix);

            assert_eq!(
                data_dir_path(Some("path///".into()), net),
                expected.display().to_string(),
            );
        }
    }

    /// Default: no flags → file logging enabled at `<data_dir>/debug.log`.
    #[test]
    fn test_resolve_debug_log_file_default() {
        let result = resolve_debug_log_file(false, None, "/home/user/.floresta");
        let expected = PathBuf::from("/home/user/.floresta").join(LOG_FILE);
        assert_eq!(result, Some(expected.to_string_lossy().into_owned()));
    }

    /// `--nodebuglogfile` → file logging disabled.
    #[test]
    fn test_resolve_debug_log_file_disabled() {
        let result = resolve_debug_log_file(true, None, "/home/user/.floresta");
        assert_eq!(result, None);
    }

    /// `--nodebuglogfile` overrides `--debuglogfile`.
    #[test]
    fn test_resolve_debug_log_file_disabled_overrides_custom() {
        let result =
            resolve_debug_log_file(true, Some("custom.log"), "/home/user/.floresta");
        assert_eq!(result, None);
    }

    /// `--debuglogfile custom.log` (relative) → `<data_dir>/custom.log`.
    #[test]
    fn test_resolve_debug_log_file_relative_path() {
        let result =
            resolve_debug_log_file(false, Some("custom.log"), "/home/user/.floresta");
        let expected = PathBuf::from("/home/user/.floresta").join("custom.log");
        assert_eq!(result, Some(expected.to_string_lossy().into_owned()));
    }

    /// `--debuglogfile /tmp/floresta.log` (absolute) → used as-is.
    #[test]
    fn test_resolve_debug_log_file_absolute_path() {
        let result = resolve_debug_log_file(
            false,
            Some("/tmp/floresta.log"),
            "/home/user/.floresta",
        );
        assert_eq!(result, Some("/tmp/floresta.log".to_string()));
    }

    /// Relative path with subdirectory: `--debuglogfile logs/node.log`.
    #[test]
    fn test_resolve_debug_log_file_relative_subdir() {
        let result =
            resolve_debug_log_file(false, Some("logs/node.log"), "/home/user/.floresta");
        let expected = PathBuf::from("/home/user/.floresta").join("logs/node.log");
        assert_eq!(result, Some(expected.to_string_lossy().into_owned()));
    }

    /// Works correctly with a network-specific data directory (e.g. signet).
    #[test]
    fn test_resolve_debug_log_file_network_specific_datadir() {
        let data_dir = data_dir_path(Some("/home/user/.floresta".into()), Network::Signet);
        let result = resolve_debug_log_file(false, None, &data_dir);
        let expected = PathBuf::from("/home/user/.floresta/signet").join(LOG_FILE);
        assert_eq!(result, Some(expected.to_string_lossy().into_owned()));
    }
}
