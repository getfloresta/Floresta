use bitcoin::block::Header as BlockHeader;
use bitcoin::BlockHash;
use bitcoin::Txid;
use corepc_types::v29::GetTxOut;
use jsonrpc::arg;
pub use jsonrpc::Client;
use serde_json::Number;
use serde_json::Value;

use crate::rpc::FlorestaRPC;
use crate::rpc_types;
use crate::rpc_types::*;

type Result<T> = std::result::Result<T, rpc_types::Error>;

impl FlorestaRPC for Client {
    fn find_tx_out(
        &self,
        tx_id: Txid,
        outpoint: u32,
        script: String,
        height_hint: u32,
    ) -> Result<Value> {
        let args = arg([
            Value::String(tx_id.to_string()),
            Value::Number(Number::from(outpoint)),
            Value::String(script),
            Value::Number(Number::from(height_hint)),
        ]);
        Ok(self.call("findtxout", Some(&args))?)
    }

    fn uptime(&self) -> Result<u32> {
        Ok(self.call("uptime", None)?)
    }

    fn get_memory_info(&self, mode: String) -> Result<GetMemInfoRes> {
        let args = arg([Value::String(mode)]);

        Ok(self.call("getmemoryinfo", Some(&args))?)
    }

    fn get_rpc_info(&self) -> Result<GetRpcInfoRes> {
        Ok(self.call("getrpcinfo", None)?)
    }

    fn add_node(&self, node: String, command: AddNodeCommand, v2transport: bool) -> Result<Value> {
        let args = arg([
            Value::String(node),
            Value::String(command.to_string()),
            Value::Bool(v2transport),
        ]);

        Ok(self.call("addnode", Some(&args))?)
    }

    fn stop(&self) -> Result<String> {
        Ok(self.call("stop", None)?)
    }

    fn rescanblockchain(
        &self,
        start_height: Option<u32>,
        stop_height: Option<u32>,
        use_timestamp: bool,
        confidence: RescanConfidence,
    ) -> Result<bool> {
        let start_height = start_height.unwrap_or(0u32);

        let stop_height = stop_height.unwrap_or(0u32);

        let args = arg([
            Value::Number(Number::from(start_height)),
            Value::Number(Number::from(stop_height)),
            Value::Bool(use_timestamp),
            serde_json::to_value(&confidence).expect("RescanConfidence implements Ser/De"),
        ]);

        Ok(self.call("rescanblockchain", Some(&args))?)
    }

    fn get_roots(&self) -> Result<Vec<String>> {
        Ok(self.call("getroots", None)?)
    }

    fn get_block(&self, hash: BlockHash, verbosity: Option<u32>) -> Result<GetBlockRes> {
        let verbosity = verbosity.unwrap_or(0);

        let args = arg([
            Value::String(hash.to_string()),
            Value::Number(Number::from(verbosity)),
        ]);

        match verbosity {
            0 => Ok(GetBlockRes::Serialized(self.call("getblock", Some(&args))?)),

            1 => Ok(GetBlockRes::Verbose(self.call("getblock", Some(&args))?)),

            _ => Err(rpc_types::Error::InvalidVerbosity),
        }
    }

    fn get_block_count(&self) -> Result<u32> {
        Ok(self.call("getblockcount", None)?)
    }

    fn get_tx_out(&self, tx_id: Txid, outpoint: u32) -> Result<GetTxOut> {
        let args = arg([
            Value::String(tx_id.to_string()),
            Value::Number(Number::from(outpoint)),
        ]);

        Ok(self.call("gettxout", Some(&args))?)
    }

    fn get_txout_proof(&self, txids: Vec<Txid>, blockhash: Option<BlockHash>) -> Result<String> {
        let args = arg([
            serde_json::to_value(txids)
                .expect("Unreachable, Vec<Txid> can be parsed into a json value"),
            blockhash
                .map(|b| Value::String(b.to_string()))
                .unwrap_or(Value::Null), // Why serde_json doesnt already maps None to null ?
        ]);

        Ok(self.call("gettxoutproof", Some(&args))?)
    }

    fn get_peer_info(&self) -> Result<Vec<PeerInfo>> {
        Ok(self.call("getpeerinfo", None)?)
    }

    fn get_best_block_hash(&self) -> Result<BlockHash> {
        Ok(self.call("getbestblockhash", None)?)
    }

    fn get_block_hash(&self, height: u32) -> Result<BlockHash> {
        let args = arg(Value::Number(Number::from(height)));
        Ok(self.call("getblockhash", Some(&args))?)
    }

    fn get_transaction(&self, tx_id: Txid, verbosity: Option<bool>) -> Result<Value> {
        let verbosity = verbosity.unwrap_or(false);

        let args = arg([Value::String(tx_id.to_string()), Value::Bool(verbosity)]);

        Ok(self.call("gettransaction", Some(&args))?)
    }

    fn load_descriptor(&self, descriptor: String) -> Result<bool> {
        let args = arg([Value::String(descriptor)]);

        Ok(self.call("loaddescriptor", Some(&args))?)
    }

    fn get_block_filter(&self, height: u32) -> Result<String> {
        let args = arg([Value::Number(Number::from(height))]);

        Ok(self.call("getblockfilter", Some(&args))?)
    }

    fn get_block_header(&self, hash: BlockHash) -> Result<BlockHeader> {
        let args = arg([Value::String(hash.to_string())]);

        Ok(self.call("getblockheader", Some(&args))?)
    }

    fn get_blockchain_info(&self) -> Result<GetBlockchainInfoRes> {
        Ok(self.call("getblockchaininfo", None)?)
    }

    fn send_raw_transaction(&self, tx: String) -> Result<Txid> {
        let args = arg([Value::String(tx)]);
        Ok(self.call("sendrawtransaction", Some(&args))?)
    }

    fn list_descriptors(&self) -> Result<Vec<String>> {
        Ok(self.call("listdescriptors", None)?)
    }

    fn ping(&self) -> Result<()> {
        Ok(self.call("ping", None)?)
    }
}
