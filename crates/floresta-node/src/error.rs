use std::net::AddrParseError;

use bitcoin::consensus::encode;
use floresta_chain::BlockValidationErrors;
use floresta_chain::BlockchainError;
use floresta_chain::FlatChainstoreError;
use floresta_common::impl_error_from;
#[cfg(feature = "compact-filters")]
use floresta_compact_filters::FlatFilterStoreError;
use floresta_watch_only::kv_database::KvDatabaseError;
use floresta_watch_only::WatchOnlyError;
use tokio_rustls::rustls::pki_types;

use crate::slip132;
#[derive(Debug)]
pub enum FlorestadError {
    /// Encoding/decoding error.
    Encode(encode::Error),

    /// Integer parsing error.
    ParseNum(std::num::ParseIntError),

    /// Proof validation failure.
    Rustreexo(String),

    /// Generic IO operation error.
    Io(std::io::Error),

    // Block validation error, such as a missing transaction or an invalid proof.
    BlockValidation(BlockValidationErrors),

    /// Script validation error, such as an invalid script or a failed evaluation.
    ScriptValidation(bitcoin::blockdata::script::Error),

    /// Blockchain backend error, such as a missing block.
    Blockchain(BlockchainError),

    /// Deserializing JSON error.
    SerdeJson(serde_json::Error),

    /// TOML parsing error.
    TomlParsing(toml::de::Error),

    /// Parsing registered HD version bytes from slip132.
    WalletInput(slip132::Error),

    /// Parsing a bitcoin address.
    AddressParsing(bitcoin::address::ParseError),

    /// Parsing miniscript error.
    Miniscript(miniscript::Error),

    /// Parsing a private key in PEM format.
    InvalidPrivKey(pki_types::pem::Error),

    /// Parsing a certificate from PEM format.
    InvalidCert(pki_types::pem::Error),

    /// Configuring TLS settings.
    CouldNotConfigureTLS(tokio_rustls::rustls::Error),

    /// Generating a PKCS#8 keypair.
    CouldNotGenerateKeypair(rcgen::Error),

    /// Generating a certificate parameter.
    CouldNotGenerateCertParam(rcgen::Error),

    /// Generating a self-signed certificate.
    CouldNotGenerateSelfSignedCert(rcgen::Error),

    /// Writing a file to the filesystem.
    CouldNotWriteFile(String, std::io::Error),

    /// Data directory doesn't exist or is not writable.
    InvalidDataDir(String),

    /// Obtaining a lock on the data directory.
    CouldNotOpenKvDatabase(KvDatabaseError),

    /// Initializing the watch-only wallet.
    CouldNotInitializeWallet(WatchOnlyError<KvDatabaseError>),

    /// Setting up the watch-only wallet.
    CouldNotSetupWallet(String),

    /// Invalid assumed valid value.
    InvalidAssumeValid(bitcoin::hex::HexToArrayError),

    /// Failed to create a chain provider.
    CouldNotCreateChainProvider(String),

    /// Failed to create an Electrum server.
    CouldNotCreateElectrumServer(Box<dyn std::error::Error>),

    /// Failed to bind the Electrum server to a socket.
    FailedToBindElectrumServer(std::io::Error),

    /// Failed to create the TLS data directory.
    CouldNotCreateTLSDataDir(String, std::io::Error),

    /// Failed to provide a valid xpub.
    InvalidProvidedXpub(String, slip132::Error),

    /// Failed to obtain the wallet cache.
    CouldNotObtainWalletCache(WatchOnlyError<KvDatabaseError>),

    /// Failed to push a descriptor to the wallet.
    CouldNotPushDescriptor(String),

    /// The network is unsupported.
    UnsupportedNetwork(bitcoin::Network),

    /// Invalid Ip address error.
    InvalidIpAddress(AddrParseError),

    /// Ip address not found error.
    NoIPAddressesFound(String),

    /// Resolve a hostname error.
    CouldNotResolveHostname(std::io::Error),

    /// Create a flat chain store error.
    CouldNotCreateFlatChainStore(FlatChainstoreError),

    /// Load a flat chain store error.
    CouldNotLoadFlatChainStore(BlockchainError),

    #[cfg(feature = "compact-filters")]
    /// Load a filter headers store
    CouldNotLoadFilterHeadersStore(FlatFilterStoreError),
}

impl std::fmt::Display for FlorestadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "compact-filters")]
            FlorestadError::CouldNotLoadFilterHeadersStore(err) => {
                write!(f, "Failure while loading filter headers store: {err:?}")
            }
            FlorestadError::Encode(err) => write!(f, "Encode error: {err}"),
            FlorestadError::ParseNum(err) => write!(f, "int parse error: {err}"),
            FlorestadError::Rustreexo(err) => write!(f, "Rustreexo error: {err}"),
            FlorestadError::Io(err) => write!(f, "Io error {err}"),
            FlorestadError::ScriptValidation(err) => {
                write!(f, "Error during script evaluation: {err}")
            }
            FlorestadError::Blockchain(err) => {
                write!(f, "Error with our blockchain backend: {err:?}")
            }
            FlorestadError::SerdeJson(err) => write!(f, "Error serializing object {err}"),
            FlorestadError::WalletInput(err) => write!(f, "Error while parsing user input {err:?}"),
            FlorestadError::TomlParsing(err) => write!(f, "Error deserializing toml file {err}"),
            FlorestadError::AddressParsing(err) => write!(f, "Invalid address {err}"),
            FlorestadError::Miniscript(err) => write!(f, "Miniscript error: {err}"),
            FlorestadError::BlockValidation(err) => {
                write!(f, "Error while validating block: {err:?}")
            }
            FlorestadError::CouldNotConfigureTLS(err) => {
                write!(f, "Error while configuring TLS: {err:?}")
            }
            FlorestadError::InvalidPrivKey(err) => {
                write!(f, "Error while reading PKCS#8 private key {err:?}")
            }
            FlorestadError::InvalidCert(err) => {
                write!(f, "Error while reading PKCS#8 certificate {err:?}")
            }
            FlorestadError::CouldNotGenerateKeypair(err) => {
                write!(f, "Error while generating PKCS#8 keypair: {err}")
            }
            FlorestadError::CouldNotGenerateCertParam(err) => {
                write!(f, "Error while generating certificate param: {err}")
            }
            FlorestadError::CouldNotGenerateSelfSignedCert(err) => {
                write!(f, "Error while generating self-signed certificate: {err}")
            }
            FlorestadError::CouldNotWriteFile(path, err) => {
                write!(f, "Error while creating file {path}: {err}")
            }
            FlorestadError::InvalidDataDir(path) => {
                write!(f, "Data directory doesn't exist or is not writable: {path}")
            }
            FlorestadError::CouldNotOpenKvDatabase(err) => {
                write!(f, "Cannot open a key-value database: {err}")
            }
            FlorestadError::CouldNotInitializeWallet(err) => {
                write!(f, "Could not initialize wallet: {err}")
            }
            FlorestadError::CouldNotSetupWallet(err) => {
                write!(f, "Could not setup wallet: {err}")
            }
            FlorestadError::InvalidAssumeValid(error) => {
                write!(f, "Invalid assumed valid value: {error}")
            }
            FlorestadError::CouldNotCreateChainProvider(err) => {
                write!(f, "Could not create chain provider: {err}")
            }
            FlorestadError::CouldNotCreateElectrumServer(err) => {
                write!(f, "Could not create Electrum server: {err}")
            }
            FlorestadError::FailedToBindElectrumServer(err) => {
                write!(f, "Failed to bind Electrum server: {err}")
            }
            FlorestadError::CouldNotCreateTLSDataDir(path, err) => {
                write!(f, "Could not create TLS data directory {path}: {err}")
            }
            FlorestadError::InvalidProvidedXpub(xpub, err) => {
                write!(f, "Invalid provided xpub {xpub}: {err:?}")
            }
            FlorestadError::CouldNotObtainWalletCache(err) => {
                write!(f, "Could not obtain wallet cache: {err}")
            }
            FlorestadError::CouldNotPushDescriptor(err) => {
                write!(f, "Could not push descriptor to wallet: {err}")
            }
            FlorestadError::UnsupportedNetwork(err) => {
                write!(f, "Unsupported network: {err}")
            }
            FlorestadError::InvalidIpAddress(err) => {
                write!(f, "Invalid IP address: {err}")
            }
            FlorestadError::NoIPAddressesFound(hostname) => {
                write!(f, "No IP Addresses found for {hostname}")
            }
            FlorestadError::CouldNotResolveHostname(host) => {
                write!(f, "Could not resolve hostname: {host}")
            }
            FlorestadError::CouldNotCreateFlatChainStore(err) => {
                write!(f, "Failure while creating chainstore: {err:?}")
            }
            FlorestadError::CouldNotLoadFlatChainStore(err) => {
                write!(f, "Failure while loading flat chainstore: {err:?}")
            }
        }
    }
}

#[cfg(feature = "compact-filters")]
impl_error_from!(
    FlorestadError,
    FlatFilterStoreError,
    CouldNotLoadFilterHeadersStore
);
impl_error_from!(FlorestadError, encode::Error, Encode);
impl_error_from!(FlorestadError, std::num::ParseIntError, ParseNum);
impl_error_from!(FlorestadError, String, Rustreexo);
impl_error_from!(FlorestadError, std::io::Error, Io);
impl_error_from!(
    FlorestadError,
    bitcoin::blockdata::script::Error,
    ScriptValidation
);
impl_error_from!(FlorestadError, BlockchainError, Blockchain);
impl_error_from!(FlorestadError, serde_json::Error, SerdeJson);
impl_error_from!(FlorestadError, slip132::Error, WalletInput);
impl_error_from!(FlorestadError, toml::de::Error, TomlParsing);
impl_error_from!(FlorestadError, BlockValidationErrors, BlockValidation);
impl_error_from!(FlorestadError, bitcoin::address::ParseError, AddressParsing);
impl_error_from!(FlorestadError, miniscript::Error, Miniscript);
impl_error_from!(FlorestadError, pki_types::pem::Error, InvalidPrivKey);
impl_error_from!(
    FlorestadError,
    tokio_rustls::rustls::Error,
    CouldNotConfigureTLS
);
impl_error_from!(
    FlorestadError,
    WatchOnlyError<KvDatabaseError>,
    CouldNotInitializeWallet
);

impl std::error::Error for FlorestadError {}
