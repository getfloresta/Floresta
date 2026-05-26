use std::{
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt as _;

use crate::{parse_asset_id, BitAssetData, DutchAuctionParams, Error, NativeBitAssetsWallet};

#[derive(Debug, Deserialize)]
pub struct EmbeddedWalletConfig {
    pub path: PathBuf,
    pub rpc_url: String,
    #[serde(default, alias = "quicUrl", alias = "bitassetsLiteWalletQuicUrl")]
    pub bitassets_lite_wallet_quic_url: Option<String>,
    #[serde(default)]
    pub seed_hex: Option<String>,
    #[serde(default)]
    pub create: bool,
    #[serde(default = "default_persist_seed")]
    pub persist_seed: bool,
}

pub struct EmbeddedBitAssetsWallet {
    wallet: NativeBitAssetsWallet,
    bitassets_lite_wallet_quic_url: Option<String>,
}

impl EmbeddedBitAssetsWallet {
    pub fn open(config: EmbeddedWalletConfig) -> Result<Self, Error> {
        Self::open_with_seed_storage(config)
    }

    pub fn open_with_seed_storage(config: EmbeddedWalletConfig) -> Result<Self, Error> {
        let mut wallet = NativeBitAssetsWallet::open_with_seed_storage(
            config.path,
            config.rpc_url,
            config.seed_hex.as_deref(),
            config.create,
            config.persist_seed,
        )?;
        let bitassets_lite_wallet_quic_url = config
            .bitassets_lite_wallet_quic_url
            .filter(|url| !url.trim().is_empty());
        wallet.set_quic_enabled(bitassets_lite_wallet_quic_url.is_some());
        Ok(Self {
            wallet,
            bitassets_lite_wallet_quic_url,
        })
    }

    pub fn get_new_address(&mut self) -> Result<String, Error> {
        self.wallet.get_new_address()
    }

    pub fn wallet_info_json(&self) -> Result<String, Error> {
        to_json_string(self.wallet.wallet_info())
    }

    pub fn sync_json(&mut self) -> Result<String, Error> {
        if let Some(quic_url) = self.bitassets_lite_wallet_quic_url.clone() {
            match self.sync_quic_once(&quic_url) {
                Ok(info) => return to_json_string(info),
                Err(err) => {
                    self.wallet.set_quic_error(err.to_string());
                    return Err(err);
                }
            }
        }
        to_json_string(self.wallet.sync()?)
    }

    pub fn list_utxos_json(&self) -> Result<String, Error> {
        to_json_string(self.wallet.list_utxos())
    }

    pub fn get_balance_json(&self, asset_id: Option<&str>) -> Result<String, Error> {
        to_json_string(self.wallet.get_balance(asset_id))
    }

    pub fn transfer_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: TransferParams = serde_json::from_str(params_json)?;
        to_json_string(self.wallet.transfer(
            &params.destination_address,
            &params.asset_id,
            params.amount,
            params.fee_sats.unwrap_or(0),
            params.memo.map(String::into_bytes),
        )?)
    }

    pub fn reserve_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: ReserveParams = serde_json::from_str(params_json)?;
        to_json_string(
            self.wallet
                .reserve(&params.name, params.fee_sats.unwrap_or(0))?,
        )
    }

    pub fn register_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: RegisterParams = serde_json::from_str(params_json)?;
        to_json_string(self.wallet.register(
            &params.name,
            params.initial_supply,
            params.bitasset_data,
            params.fee_sats.unwrap_or(0),
        )?)
    }

    pub fn amm_mint_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: AmmMintParams = serde_json::from_str(params_json)?;
        to_json_string(self.wallet.amm_mint(
            &params.asset0,
            &params.asset1,
            params.amount0,
            params.amount1,
            params.lp_token_mint,
            params.fee_sats.unwrap_or(0),
        )?)
    }

    pub fn amm_swap_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: AmmSwapParams = serde_json::from_str(params_json)?;
        to_json_string(self.wallet.amm_swap(
            &params.asset_spend,
            &params.asset_receive,
            params.amount_spend,
            params.amount_receive,
            params.fee_sats.unwrap_or(0),
        )?)
    }

    pub fn amm_burn_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: AmmBurnParams = serde_json::from_str(params_json)?;
        to_json_string(self.wallet.amm_burn(
            &params.asset0,
            &params.asset1,
            params.amount0,
            params.amount1,
            params.lp_token_burn,
            params.fee_sats.unwrap_or(0),
        )?)
    }

    pub fn dutch_auction_create_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: DutchAuctionCreateParams = serde_json::from_str(params_json)?;
        to_json_string(self.wallet.dutch_auction_create(
            DutchAuctionParams {
                start_block: params.start_block,
                duration: params.duration,
                base_asset: parse_asset_id(&params.base_asset)?,
                base_amount: params.base_amount,
                quote_asset: parse_asset_id(&params.quote_asset)?,
                initial_price: params.initial_price,
                final_price: params.final_price,
            },
            params.fee_sats.unwrap_or(0),
        )?)
    }

    pub fn dutch_auction_bid_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: DutchAuctionBidParams = serde_json::from_str(params_json)?;
        to_json_string(self.wallet.dutch_auction_bid(
            &params.auction_id,
            &params.base_asset,
            &params.quote_asset,
            params.bid_size,
            params.receive_quantity,
            params.fee_sats.unwrap_or(0),
        )?)
    }

    pub fn dutch_auction_collect_json(&mut self, params_json: &str) -> Result<String, Error> {
        let params: DutchAuctionCollectParams = serde_json::from_str(params_json)?;
        to_json_string(self.wallet.dutch_auction_collect(
            &params.auction_id,
            &params.base_asset,
            &params.quote_asset,
            params.amount_base,
            params.amount_quote,
            params.fee_sats.unwrap_or(0),
        )?)
    }

    fn sync_quic_once(&mut self, quic_url: &str) -> Result<Value, Error> {
        let remote = bitassets_quic_remote(quic_url)?;
        let script_hashes = self.wallet.script_hashes()?;
        if script_hashes.is_empty() {
            return self
                .wallet_info_json()
                .and_then(|json| serde_json::from_str(&json).map_err(Error::from));
        }
        let from_block_hash = self.wallet.last_tip_hash();
        let request = json!({
            "type": "subscribe",
            "script_hashes": script_hashes,
            "from_block_hash": from_block_hash,
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| Error::Rpc(format!("could not start QUIC runtime: {err}")))?;
        let update = runtime.block_on(async {
            let endpoint = bitassets_quic_endpoint(remote)?;
            let connection = endpoint
                .connect(remote, "localhost")
                .map_err(|err| Error::Rpc(format!("QUIC connect setup failed: {err}")))?
                .await
                .map_err(|err| Error::Rpc(format!("QUIC connect failed: {err}")))?;
            let (mut send, mut recv) = connection
                .open_bi()
                .await
                .map_err(|err| Error::Rpc(format!("QUIC stream open failed: {err}")))?;
            let mut request = serde_json::to_vec(&request)?;
            request.push(b'\n');
            send.write_all(&request)
                .await
                .map_err(|err| Error::Rpc(format!("QUIC subscribe write failed: {err}")))?;
            send.finish()
                .map_err(|err| Error::Rpc(format!("QUIC subscribe finish failed: {err}")))?;

            let mut buffer = Vec::<u8>::new();
            loop {
                let chunk =
                    tokio::time::timeout(Duration::from_secs(15), recv.read_buf(&mut buffer))
                        .await
                        .map_err(|_| Error::Rpc("QUIC lite-wallet sync timed out".to_string()))?
                        .map_err(|err| Error::Rpc(format!("QUIC read failed: {err}")))?;
                if chunk == 0 && buffer.is_empty() {
                    return Err(Error::Rpc("QUIC lite-wallet stream closed".to_string()));
                }
                while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                    let line = buffer.drain(..=newline).collect::<Vec<_>>();
                    let line = &line[..line.len().saturating_sub(1)];
                    if line.is_empty() {
                        continue;
                    }
                    return bitassets_quic_update_from_message(line);
                }
                if chunk == 0 {
                    if !buffer.is_empty() {
                        return bitassets_quic_update_from_message(&buffer);
                    }
                    return Err(Error::Rpc(
                        "QUIC lite-wallet stream ended without update".to_string(),
                    ));
                }
            }
        })?;
        self.wallet.apply_quic_update(&update)
    }
}

fn bitassets_quic_remote(quic_url: &str) -> Result<SocketAddr, Error> {
    quic_url
        .to_socket_addrs()
        .map_err(|err| Error::Rpc(format!("invalid BitAssets QUIC peer {quic_url}: {err}")))?
        .next()
        .ok_or_else(|| Error::Rpc(format!("BitAssets QUIC peer {quic_url} did not resolve")))
}

fn bitassets_quic_update_from_message(line: &[u8]) -> Result<Value, Error> {
    let message: Value = serde_json::from_slice(line)?;
    let message_type = message
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Rpc("QUIC message missing type".to_string()))?;
    if message_type == "error" {
        let message = message
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("plain-bitassets lite-wallet error");
        return Err(Error::Rpc(message.to_string()));
    }
    match message_type {
        "snapshot" | "confirmed" | "mempool" => message
            .get("update")
            .cloned()
            .ok_or_else(|| Error::Rpc(format!("{message_type} message missing update"))),
        other => Err(Error::Rpc(format!("unknown QUIC message type {other}"))),
    }
}

fn bitassets_quic_endpoint(remote: SocketAddr) -> Result<quinn::Endpoint, Error> {
    let bind_addr: SocketAddr = if remote.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    }
    .parse()
    .map_err(|err| Error::Rpc(format!("invalid QUIC bind address: {err}")))?;
    let mut endpoint = quinn::Endpoint::client(bind_addr)
        .map_err(|err| Error::Rpc(format!("could not create QUIC endpoint: {err}")))?;
    endpoint.set_default_client_config(bitassets_quic_client_config()?);
    Ok(endpoint)
}

fn bitassets_quic_client_config() -> Result<quinn::ClientConfig, Error> {
    #[derive(Debug)]
    struct SkipServerVerification;

    impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &rustls::pki_types::CertificateDer,
            _intermediates: &[rustls::pki_types::CertificateDer],
            _server_name: &rustls::pki_types::ServerName,
            _ocsp_response: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &rustls::pki_types::CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls12_signature(
                message,
                cert,
                dss,
                &rustls::crypto::ring::default_provider().signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &rustls::pki_types::CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(
                message,
                cert,
                dss,
                &rustls::crypto::ring::default_provider().signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    let crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();
    let client_config = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
        .map_err(|err| Error::Rpc(format!("could not create QUIC rustls client config: {err}")))?;
    Ok(quinn::ClientConfig::new(Arc::new(client_config)))
}

fn default_persist_seed() -> bool {
    false
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferParams {
    #[serde(alias = "destinationAddress")]
    destination_address: String,
    #[serde(alias = "assetId")]
    asset_id: String,
    amount: u64,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
    #[serde(default)]
    memo: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReserveParams {
    name: String,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterParams {
    name: String,
    #[serde(alias = "initialSupply")]
    initial_supply: u64,
    #[serde(alias = "bitassetData")]
    bitasset_data: BitAssetData,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AmmMintParams {
    asset0: String,
    asset1: String,
    amount0: u64,
    amount1: u64,
    #[serde(alias = "lpTokenMint")]
    lp_token_mint: u64,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AmmSwapParams {
    #[serde(alias = "assetSpend")]
    asset_spend: String,
    #[serde(alias = "assetReceive")]
    asset_receive: String,
    #[serde(alias = "amountSpend")]
    amount_spend: u64,
    #[serde(alias = "amountReceive")]
    amount_receive: u64,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AmmBurnParams {
    asset0: String,
    asset1: String,
    amount0: u64,
    amount1: u64,
    #[serde(alias = "lpTokenBurn")]
    lp_token_burn: u64,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DutchAuctionCreateParams {
    #[serde(default, alias = "startBlock")]
    start_block: u32,
    duration: u32,
    #[serde(alias = "baseAsset")]
    base_asset: String,
    #[serde(alias = "baseAmount")]
    base_amount: u64,
    #[serde(alias = "quoteAsset")]
    quote_asset: String,
    #[serde(alias = "initialPrice", alias = "startPrice")]
    initial_price: u64,
    #[serde(alias = "finalPrice", alias = "endPrice")]
    final_price: u64,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DutchAuctionBidParams {
    #[serde(alias = "auctionId")]
    auction_id: String,
    #[serde(alias = "baseAsset")]
    base_asset: String,
    #[serde(alias = "quoteAsset")]
    quote_asset: String,
    #[serde(alias = "bidSize")]
    bid_size: u64,
    #[serde(alias = "receiveQuantity")]
    receive_quantity: u64,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DutchAuctionCollectParams {
    #[serde(alias = "auctionId")]
    auction_id: String,
    #[serde(alias = "baseAsset")]
    base_asset: String,
    #[serde(alias = "quoteAsset")]
    quote_asset: String,
    #[serde(alias = "amountBase")]
    amount_base: u64,
    #[serde(alias = "amountQuote")]
    amount_quote: u64,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

fn to_json_string(value: Value) -> Result<String, Error> {
    serde_json::to_string(&value).map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_SEED: &str = concat!(
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000"
    );

    #[test]
    fn mobile_facade_creates_wallet_and_serializes_basic_methods() {
        let dir = tempfile::tempdir().unwrap();
        let mut wallet = EmbeddedBitAssetsWallet::open(EmbeddedWalletConfig {
            path: dir.path().join("wallet.json"),
            rpc_url: "http://127.0.0.1:6004".to_string(),
            bitassets_lite_wallet_quic_url: None,
            seed_hex: Some(ZERO_SEED.to_string()),
            create: true,
            persist_seed: true,
        })
        .unwrap();

        let address = wallet.get_new_address().unwrap();
        assert!(!address.is_empty());
        let info: Value = serde_json::from_str(&wallet.wallet_info_json().unwrap()).unwrap();
        assert_eq!(info["enabled"], true);
        assert_eq!(info["address_count"], 1);
        let utxos: Value = serde_json::from_str(&wallet.list_utxos_json().unwrap()).unwrap();
        assert!(utxos["confirmed"].as_array().unwrap().is_empty());
    }

    #[test]
    fn mobile_config_defaults_to_not_persisting_seed() {
        let config: EmbeddedWalletConfig = serde_json::from_str(
            r#"{"path":"/tmp/floresta-bitassets-wallet.json","rpc_url":"http://127.0.0.1:6004","seed_hex":"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","create":true}"#,
        )
        .unwrap();

        assert!(!config.persist_seed);
        assert_eq!(config.bitassets_lite_wallet_quic_url, None);
    }

    #[test]
    fn mobile_config_accepts_quic_url_aliases() {
        let config: EmbeddedWalletConfig = serde_json::from_str(
            r#"{"path":"/tmp/floresta-bitassets-wallet.json","rpc_url":"http://127.0.0.1:6004","bitassetsLiteWalletQuicUrl":"192.168.1.236:6104","seed_hex":"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","create":true}"#,
        )
        .unwrap();

        assert_eq!(
            config.bitassets_lite_wallet_quic_url.as_deref(),
            Some("192.168.1.236:6104")
        );
    }

    #[test]
    fn mobile_open_honors_persist_seed_false() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallet.json");
        let mut wallet = EmbeddedBitAssetsWallet::open(EmbeddedWalletConfig {
            path: path.clone(),
            rpc_url: "http://127.0.0.1:6004".to_string(),
            bitassets_lite_wallet_quic_url: None,
            seed_hex: Some(ZERO_SEED.to_string()),
            create: true,
            persist_seed: false,
        })
        .unwrap();
        wallet.get_new_address().unwrap();

        let persisted = std::fs::read_to_string(&path).unwrap();
        assert!(persisted.contains("\"seed_hex\": \"\""));
        assert!(!persisted.contains(ZERO_SEED));
    }

    #[test]
    fn mobile_action_params_reject_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let mut wallet = EmbeddedBitAssetsWallet::open(EmbeddedWalletConfig {
            path: dir.path().join("wallet.json"),
            rpc_url: "http://127.0.0.1:6004".to_string(),
            bitassets_lite_wallet_quic_url: None,
            seed_hex: Some(ZERO_SEED.to_string()),
            create: true,
            persist_seed: false,
        })
        .unwrap();

        let err = wallet
            .reserve_json(r#"{"name":"asset-a","unexpectedDebugField":true}"#)
            .unwrap_err();
        assert!(
            err.to_string().contains("unknown field"),
            "unexpected error: {err}"
        );
        assert!(err.to_string().contains("unexpectedDebugField"));
    }

    #[test]
    fn mobile_facade_accepts_camel_case_and_rejects_nonzero_fee() {
        let dir = tempfile::tempdir().unwrap();
        let mut wallet = EmbeddedBitAssetsWallet::open(EmbeddedWalletConfig {
            path: dir.path().join("wallet.json"),
            rpc_url: "http://127.0.0.1:6004".to_string(),
            bitassets_lite_wallet_quic_url: None,
            seed_hex: Some(ZERO_SEED.to_string()),
            create: true,
            persist_seed: true,
        })
        .unwrap();

        let err = wallet
            .reserve_json(r#"{"name":"asset-a","feeSats":1}"#)
            .unwrap_err();
        assert!(err.to_string().contains("fee_sats=0"));
    }
}
