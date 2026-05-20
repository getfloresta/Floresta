use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

use crate::{parse_asset_id, BitAssetData, DutchAuctionParams, Error, NativeBitAssetsWallet};

#[derive(Debug, Deserialize)]
pub struct EmbeddedWalletConfig {
    pub path: PathBuf,
    pub rpc_url: String,
    #[serde(default)]
    pub seed_hex: Option<String>,
    #[serde(default)]
    pub create: bool,
    #[serde(default = "default_persist_seed")]
    pub persist_seed: bool,
}

pub struct EmbeddedBitAssetsWallet {
    wallet: NativeBitAssetsWallet,
}

impl EmbeddedBitAssetsWallet {
    pub fn open(config: EmbeddedWalletConfig) -> Result<Self, Error> {
        Ok(Self {
            wallet: NativeBitAssetsWallet::open(
                config.path,
                config.rpc_url,
                config.seed_hex.as_deref(),
                config.create,
            )?,
        })
    }

    pub fn open_with_seed_storage(config: EmbeddedWalletConfig) -> Result<Self, Error> {
        Ok(Self {
            wallet: NativeBitAssetsWallet::open_with_seed_storage(
                config.path,
                config.rpc_url,
                config.seed_hex.as_deref(),
                config.create,
                config.persist_seed,
            )?,
        })
    }

    pub fn get_new_address(&mut self) -> Result<String, Error> {
        self.wallet.get_new_address()
    }

    pub fn wallet_info_json(&self) -> Result<String, Error> {
        to_json_string(self.wallet.wallet_info())
    }

    pub fn sync_json(&mut self) -> Result<String, Error> {
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
}

fn default_persist_seed() -> bool {
    true
}

#[derive(Debug, Deserialize)]
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
struct ReserveParams {
    name: String,
    #[serde(default, alias = "feeSats")]
    fee_sats: Option<u64>,
}

#[derive(Debug, Deserialize)]
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
    fn mobile_facade_accepts_camel_case_and_rejects_nonzero_fee() {
        let dir = tempfile::tempdir().unwrap();
        let mut wallet = EmbeddedBitAssetsWallet::open(EmbeddedWalletConfig {
            path: dir.path().join("wallet.json"),
            rpc_url: "http://127.0.0.1:6004".to_string(),
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
