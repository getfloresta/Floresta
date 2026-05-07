use bitcoin::Address;
use bitcoin::BlockHash;
use bitcoin::ScriptBuf;
use bitcoin::Transaction;
use bitcoin::TxIn;
use bitcoin::TxOut;
use bitcoin::Txid;
use bitcoin::consensus::deserialize;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::hashes::Hash;
use bitcoin::hashes::hex::FromHex;
use bitcoin::hex::DisplayHex;
use floresta_rpc::rpc_interfaces::RawTransactionRpc;
use floresta_rpc::rpc_types::RawTx;
use floresta_rpc::rpc_types::RawTxResp;
use floresta_rpc::rpc_types::ScriptPubKeyJson;
use floresta_rpc::rpc_types::ScriptSigJson;
use floresta_rpc::rpc_types::TxInJson;
use floresta_rpc::rpc_types::TxOutJson;
use floresta_watch_only::CachedTransaction;
use floresta_wire::node_interface::MempoolMethods;

use super::server::RpcChain;
use super::server::RpcImpl;
use crate::json_rpc::res::jsonrpc_interface::JsonRpcError;

impl<Blockchain: RpcChain> RawTransactionRpc for RpcImpl<Blockchain> {
    type Error = JsonRpcError;

    async fn get_raw_transaction(
        &self,
        tx_id: Txid,
        verbosity: Option<u32>,
    ) -> Result<RawTxResp, JsonRpcError> {
        let verbosity = verbosity.unwrap_or(0);
        if verbosity > 1 {
            return Err(JsonRpcError::InvalidVerbosityLevel);
        }

        let tx = self
            .wallet
            .get_transaction(&tx_id)
            .ok_or(JsonRpcError::TxNotFound)?;

        if verbosity == 0 {
            let hex = serialize_hex(&tx.tx);
            return Ok(RawTxResp::Zero(hex));
        }

        let raw_tx = self.make_raw_transaction(tx);

        Ok(RawTxResp::One(Box::new(raw_tx?)))
    }

    async fn send_raw_transaction(&self, tx: String) -> Result<Txid, JsonRpcError> {
        let tx_hex = Vec::from_hex(&tx).map_err(|_| JsonRpcError::InvalidHex)?;
        let tx: Transaction =
            deserialize(&tx_hex).map_err(|e| JsonRpcError::Decode(e.to_string()))?;

        Ok(self
            .node
            .broadcast_transaction(tx)
            .await
            .map_err(|e| JsonRpcError::Node(e.to_string()))??)
    }
}

impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
    fn make_vin(&self, input: TxIn) -> TxInJson {
        let txid = serialize_hex(&input.previous_output.txid);
        let vout = input.previous_output.vout;
        let sequence = input.sequence.0;
        TxInJson {
            txid,
            vout,
            script_sig: ScriptSigJson {
                asm: input.script_sig.to_asm_string(),
                hex: input.script_sig.to_hex_string(),
            },
            witness: input
                .witness
                .iter()
                .map(|w| w.to_hex_string(bitcoin::hex::Case::Upper))
                .collect(),
            sequence,
        }
    }

    fn get_script_type(script: ScriptBuf) -> Option<&'static str> {
        if script.is_p2pkh() {
            return Some("p2pkh");
        }
        if script.is_p2sh() {
            return Some("p2sh");
        }
        if script.is_p2wpkh() {
            return Some("v0_p2wpkh");
        }
        if script.is_p2wsh() {
            return Some("v0_p2wsh");
        }
        None
    }

    fn make_vout(&self, output: TxOut, n: u32) -> TxOutJson {
        let value = output.value;
        TxOutJson {
            value: value.to_sat(),
            n,
            script_pub_key: ScriptPubKeyJson {
                asm: output.script_pubkey.to_asm_string(),
                hex: output.script_pubkey.to_hex_string(),
                req_sigs: 0, // This field is deprecated
                // `Address::from_script` can fail for nonstandard scripts. Bitcoin Core
                // omits the `address` field entirely when `ExtractDestination` fails:
                // https://github.com/bitcoin/bitcoin/blob/f50d53c84736f8ada8419346c4d1734d5a6686d4/src/core_io.cpp#L424
                address: Address::from_script(&output.script_pubkey, self.network)
                    .ok()
                    .map(|a| a.to_string()),
                type_: Self::get_script_type(output.script_pubkey)
                    .unwrap_or("nonstandard")
                    .to_string(),
            },
        }
    }

    pub(super) fn make_raw_transaction(
        &self,
        tx: CachedTransaction,
    ) -> Result<RawTx, JsonRpcError> {
        let raw_tx = tx.tx;
        let in_active_chain = tx.height != 0;
        let hex = serialize_hex(&raw_tx);
        let txid = serialize_hex(&raw_tx.compute_txid());
        let block_hash = self
            .chain
            .get_block_hash(tx.height)
            .unwrap_or(BlockHash::all_zeros());
        let tip = self.chain.get_height().map_err(|_| JsonRpcError::Chain)?;
        let confirmations = if in_active_chain {
            tip - tx.height + 1
        } else {
            0
        };

        Ok(RawTx {
            in_active_chain,
            hex,
            txid,
            hash: serialize_hex(&raw_tx.compute_wtxid()),
            size: raw_tx.total_size() as u32,
            vsize: raw_tx.vsize() as u32,
            weight: raw_tx.weight().to_wu() as u32,
            version: raw_tx.version.0 as u32,
            locktime: raw_tx.lock_time.to_consensus_u32(),
            vin: raw_tx
                .input
                .iter()
                .map(|input| self.make_vin(input.clone()))
                .collect(),
            vout: raw_tx
                .output
                .into_iter()
                .enumerate()
                .map(|(i, output)| self.make_vout(output, i as u32))
                .collect(),
            blockhash: serialize_hex(&block_hash),
            confirmations,
            blocktime: self
                .chain
                .get_block_header(&block_hash)
                .map(|h| h.time)
                .unwrap_or(0),
            time: self
                .chain
                .get_block_header(&block_hash)
                .map(|h| h.time)
                .unwrap_or(0),
        })
    }
}
