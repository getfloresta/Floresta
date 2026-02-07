use bitcoin::block::Header;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::consensus::Encodable;
use bitcoin::constants::genesis_block;
use bitcoin::hashes::Hash;
use bitcoin::Address;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::MerkleBlock;
use bitcoin::OutPoint;
use bitcoin::Script;
use bitcoin::ScriptBuf;
use bitcoin::Txid;
use bitcoin::VarInt;
use corepc_types::v29::GetTxOut;
use corepc_types::v30::GetBlockVerboseOne;
use corepc_types::ScriptPubkey;
use floresta_chain::extensions::HeaderExt;
use floresta_chain::extensions::WorkExt;
use miniscript::descriptor::checksum;
use serde_json::Value;
use tracing::debug;

use super::res::GetBlockchainInfoRes;
use super::res::GetTxOutProof;
use super::res::JsonRpcError;
use super::server::RpcChain;
use super::server::RpcImpl;
use crate::json_rpc::res::RescanConfidence;

impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
    async fn get_block_inner(&self, hash: BlockHash) -> Result<Block, JsonRpcError> {
        let is_genesis = self.chain.get_block_hash(0).unwrap().eq(&hash);

        if is_genesis {
            return Ok(genesis_block(self.network));
        }

        self.node
            .get_block(hash)
            .await
            .map_err(|e| JsonRpcError::Node(e.to_string()))
            .and_then(|block| block.ok_or(JsonRpcError::BlockNotFound))
    }

    /// Return the block that contains the given Txid
    pub fn get_block_by_txid(&self, txid: &Txid) -> Result<Block, JsonRpcError> {
        let height = self
            .wallet
            .get_height(txid)
            .ok_or(JsonRpcError::TxNotFound)?;
        let blockhash = self.chain.get_block_hash(height).unwrap();
        self.chain
            .get_block(&blockhash)
            .map_err(|_| JsonRpcError::BlockNotFound)
    }

    pub fn get_rescan_interval(
        &self,
        use_timestamp: bool,
        start: Option<u32>,
        stop: Option<u32>,
        confidence: Option<RescanConfidence>,
    ) -> Result<(u32, u32), JsonRpcError> {
        let start = start.unwrap_or(0u32);
        let stop = stop.unwrap_or(0u32);

        if use_timestamp {
            let confidence = confidence.unwrap_or(RescanConfidence::Medium);
            // `get_block_height_by_timestamp` already does the time validity checks.

            let start_height = self.get_block_height_by_timestamp(start, &confidence)?;

            let stop_height = self.get_block_height_by_timestamp(stop, &RescanConfidence::Exact)?;

            return Ok((start_height, stop_height));
        }

        let (tip, _) = self
            .chain
            .get_best_block()
            .map_err(|_| JsonRpcError::Chain)?;

        if stop > tip {
            return Err(JsonRpcError::InvalidRescanVal);
        }

        Ok((start, stop))
    }

    /// Retrieves the height of the block that was mined in the given timestamp.
    ///
    /// `timestamp` has an alias, 0 will directly refer to the network's genesis timestamp.
    pub fn get_block_height_by_timestamp(
        &self,
        timestamp: u32,
        confidence: &RescanConfidence,
    ) -> Result<u32, JsonRpcError> {
        /// Simple helper to avoid code reuse.
        fn get_block_time<BlockChain: RpcChain>(
            provider: &RpcImpl<BlockChain>,
            at: u32,
        ) -> Result<u32, JsonRpcError> {
            let hash = provider.get_block_hash(at)?;
            let block = provider.get_block_header(hash)?;
            Ok(block.time)
        }

        let genesis_timestamp = genesis_block(self.network).header.time;

        if timestamp == 0 || timestamp == genesis_timestamp {
            return Ok(0);
        };

        let (tip_height, _) = self
            .chain
            .get_best_block()
            .map_err(|_| JsonRpcError::BlockNotFound)?;

        let tip_time = get_block_time(self, tip_height)?;

        if timestamp < genesis_timestamp || timestamp > tip_time {
            return Err(JsonRpcError::InvalidTimestamp);
        }

        let adjusted_target = timestamp.saturating_sub(confidence.as_secs());

        let mut high = tip_height;
        let mut low = 0;
        let max_iters = tip_height.ilog2() + 1;
        for _ in 0..max_iters {
            let cut = (high + low) / 2;

            let block_timestamp = get_block_time(self, cut)?;

            if block_timestamp == adjusted_target {
                debug!("found a precise block; returning {cut}");
                return Ok(cut);
            }

            if high - low <= 2 {
                debug!("didn't find a precise block; returning {low}");
                return Ok(low);
            }

            if block_timestamp > adjusted_target {
                high = cut;
            } else {
                low = cut;
            }
        }

        // This is pretty much unreachable.
        Err(JsonRpcError::BlockNotFound)
    }
}

// blockchain rpcs
impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
    // dumputxoutset

    // getbestblockhash
    pub(super) fn get_best_block_hash(&self) -> Result<BlockHash, JsonRpcError> {
        Ok(self.chain.get_best_block().unwrap().1)
    }

    // getblock
    pub(super) async fn get_block(
        &self,
        hash: BlockHash,
    ) -> Result<GetBlockVerboseOne, JsonRpcError> {
        let block = self.get_block_inner(hash).await?;
        let header = &block.header;

        let height = header.get_height(&self.chain)?;

        let median_time = header.calculate_median_time_past(&self.chain)?;

        let chain_work = header.calculate_chain_work(&self.chain)?.to_string_hex();

        let confirmations = header.get_confirmations(&self.chain)? as i64;

        let version_hex = header.get_version_hex();

        let next_block_hash = header
            .get_next_block_hash(&self.chain)?
            .map(|h| h.to_string());

        let bits = header.get_bits_hex();

        let difficulty = header.get_difficulty();

        let target = header.get_target_hex();

        // Stripped size is the size of the block without witness data
        // Header + VarInt for number of transactions + sum of base sizes of each transaction
        let tx_count_varint_size = VarInt::from(block.txdata.len()).size();
        let total_tx_base_size: usize = block.txdata.iter().map(|tx| tx.base_size()).sum();
        let stripped_size_bytes = Header::SIZE + tx_count_varint_size + total_tx_base_size;

        let stripped_size = Some(stripped_size_bytes as i64);

        let previous_block_hash = (header.prev_blockhash != BlockHash::all_zeros())
            .then_some(header.prev_blockhash.to_string());

        let tx = block
            .txdata
            .iter()
            .map(|tx| tx.compute_txid().to_string())
            .collect();

        let block = GetBlockVerboseOne {
            bits,
            chain_work,
            confirmations,
            difficulty,
            hash: header.block_hash().to_string(),
            height: height as i64,
            merkle_root: header.merkle_root.to_string(),
            nonce: header.nonce as i64,
            previous_block_hash,
            size: block.total_size() as i64,
            time: header.time as i64,
            tx,
            version: header.version.to_consensus(),
            version_hex,
            weight: block.weight().to_wu(),
            median_time: Some(median_time as i64),
            n_tx: block.txdata.len() as i64,
            next_block_hash,
            stripped_size,
            target,
        };

        Ok(block)
    }

    pub(super) async fn get_block_serialized(
        &self,
        hash: BlockHash,
    ) -> Result<String, JsonRpcError> {
        let block = self.get_block_inner(hash).await?;
        Ok(serialize_hex(&block))
    }

    // getblockchaininfo
    pub(super) fn get_blockchain_info(&self) -> Result<GetBlockchainInfoRes, JsonRpcError> {
        let (height, hash) = self.chain.get_best_block().unwrap();
        let validated = self.chain.get_validation_index().unwrap();
        let ibd = self.chain.is_in_ibd();
        let latest_header = self.chain.get_block_header(&hash).unwrap();
        let latest_work = latest_header
            .calculate_chain_work(&self.chain)?
            .to_string_hex();
        let latest_block_time = latest_header.time;
        let leaf_count = self.chain.acc().leaves as u32;
        let root_count = self.chain.acc().roots.len() as u32;
        let root_hashes = self
            .chain
            .acc()
            .roots
            .into_iter()
            .map(|r| r.to_string())
            .collect();

        let validated_blocks = self.chain.get_validation_index().unwrap();

        let validated_percentage = if height != 0 {
            validated_blocks as f32 / height as f32
        } else {
            0.0
        };

        Ok(GetBlockchainInfoRes {
            best_block: hash.to_string(),
            height,
            ibd,
            validated,
            latest_work,
            latest_block_time,
            leaf_count,
            root_count,
            root_hashes,
            chain: self.network.to_string(),
            difficulty: latest_header.difficulty(self.chain.get_params()) as u64,
            progress: validated_percentage,
        })
    }

    // getblockcount
    pub(super) fn get_block_count(&self) -> Result<u32, JsonRpcError> {
        Ok(self.chain.get_height().unwrap())
    }

    // getblockfilter
    // getblockfrompeer (just call getblock)

    // getblockhash
    pub(super) fn get_block_hash(&self, height: u32) -> Result<BlockHash, JsonRpcError> {
        self.chain
            .get_block_hash(height)
            .map_err(|_| JsonRpcError::BlockNotFound)
    }

    // getblockheader
    pub(super) fn get_block_header(&self, hash: BlockHash) -> Result<Header, JsonRpcError> {
        self.chain
            .get_block_header(&hash)
            .map_err(|_| JsonRpcError::BlockNotFound)
    }

    // getblockstats
    // getchainstates
    // getchaintips
    // getchaintxstats
    // getdeploymentinfo
    // getdifficulty
    // getmempoolancestors
    // getmempooldescendants
    // getmempoolentry
    // getmempoolinfo
    // getrawmempool

    /// Check if the script is anchor type
    fn is_anchor_type(script: &Script) -> bool {
        script.as_bytes().starts_with(&[0x51, 0x02, 0x4e, 0x73])
    }

    /// Returns a label about the scriptPubKey type
    /// (pubkey, pubkeyhash, multisig, nulldata, scripthash, witness_v0_keyhash, witness_v0_scripthash, witness_v1_taproot, anchor, nonstandard)
    fn get_script_type_label(script: &Script) -> &'static str {
        if script.is_p2pk() {
            return "pubkey";
        }

        if script.is_p2pkh() {
            return "pubkeyhash";
        }

        if script.is_multisig() {
            return "multisig";
        }

        if script.is_op_return() {
            return "nulldata";
        }

        if script.is_p2sh() {
            return "scripthash";
        }

        if script.is_p2wpkh() {
            return "witness_v0_keyhash";
        }

        if script.is_p2wsh() {
            return "witness_v0_scripthash";
        }

        if script.is_p2tr() {
            return "witness_v1_taproot";
        }

        if Self::is_anchor_type(script) {
            return "anchor";
        }

        "nonstandard"
    }

    fn get_script_type_descriptor(script: &Script, address: &Option<Address>) -> String {
        let get_addr_str = || {
            address
                .as_ref()
                .expect("address should be Some")
                .to_string()
        };

        if script.is_p2pk() {
            let addr = get_addr_str();
            return format!("pk({addr}");
        }

        if let Some(addr) = address {
            return format!("addr({addr})");
        }

        if script.is_op_return() {
            let hex = script.to_hex_string();
            return format!("raw({hex})");
        }

        if Self::is_anchor_type(script) {
            let addr = get_addr_str();
            return format!("addr({addr})");
        }

        let hex = script.to_hex_string();
        format!("raw({hex})")
    }

    /// Parses the serialized opcodes in a [ScriptBuf] as numbers and it's hashes.
    /// This differs from `ScriptBuf::to_asm_string` in that, `rust-bitcoin` will
    /// show the the human representation of the opcode. It does not omit the number representations of
    /// `OP_PUSHDATA_<N>` and `OP_PUSHBYTE<N>`. This method do the opposite: it not show the human
    /// representation and omit the last opcodes, so it can be compliant with bitcoin-core.
    /// For reference see <https://en.bitcoin.it/wiki/Script#Opcodes>
    fn to_core_asm_string(script: &ScriptBuf) -> Result<String, JsonRpcError> {
        let mut asm = vec![];
        let bytes = script.as_bytes();
        let mut i = 0usize;

        // little reused helper to hex string
        let to_hex_string = |r: &[u8]| r.iter().map(|b| format!("{b:02x}")).collect::<String>();

        while i < bytes.len() {
            let byte = bytes[i];
            i += 1;

            match byte {
                // OP_0
                0x00 => asm.push(format!("{}", 0)),
                // OP_PUSHDATA_<N>: The next N bytes is data to be pushed onto the stack
                0x01..=0x4b => {
                    let pushed_bytes = byte as usize;
                    let hex = to_hex_string(&bytes[i..i + pushed_bytes]);
                    asm.push(hex);
                    i += pushed_bytes;
                }
                // OP_PUSHBYTE1: the next byte contains the number of bytes to be pushed onto the stack.
                0x4c => {
                    let pushed_bytes = bytes[i] as usize;
                    i += 1;
                    let hex = to_hex_string(&bytes[i..i + pushed_bytes]);
                    asm.push(hex);
                    i += pushed_bytes;
                }
                // OP_PUSHBYTE2: the next two bytes contain the number of bytes to be pushed onto the stack in little endian order.
                0x4d => {
                    let pushed_bytes = u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
                    i += 2;
                    let hex = to_hex_string(&bytes[i..i + pushed_bytes]);
                    asm.push(hex);
                    i += pushed_bytes;
                }
                // OP_PUSHBYTE4: the next four bytes contain the number of bytes to be pushed onto the stack in little endian order.
                0x4e => {
                    let pushed_bytes =
                        u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]])
                            as usize;
                    i += 4;
                    let hex = to_hex_string(&bytes[i..i + pushed_bytes]);
                    asm.push(hex);
                    i += pushed_bytes;
                }
                // OP_1 to OP_16
                0x51..=0x60 => {
                    // 0x50 is OP_RESERVED
                    let reserved = 0x50;
                    asm.push(format!("{}", byte - reserved));
                }
                // Any other opcode that should  be pushed
                another_one => {
                    asm.push(format!("{another_one:02x}"));
                }
            }
        }

        Ok(asm.join(" "))
    }

    /// gettxout: returns details about an unspent transaction output.
    pub(super) fn get_tx_out(
        &self,
        txid: Txid,
        outpoint: u32,
        _include_mempool: bool,
    ) -> Result<Option<GetTxOut>, JsonRpcError> {
        let res = match (
            self.wallet.get_transaction(&txid),
            self.wallet.get_height(&txid),
            self.wallet.get_utxo(&OutPoint {
                txid,
                vout: outpoint,
            }),
        ) {
            (Some(cached_tx), Some(height), Some(txout)) => {
                let is_coinbase = cached_tx.tx.is_coinbase();
                let Ok((bestblock_height, bestblock_hash)) = self.chain.get_best_block() else {
                    return Err(JsonRpcError::BlockNotFound);
                };

                let script = txout.script_pubkey.as_script();
                let network = self.chain.get_params().network;
                let address = Address::from_script(script, network).ok();

                let base_descriptor = Self::get_script_type_descriptor(script, &address);
                let descriptor: Option<String> = match checksum::desc_checksum(&base_descriptor) {
                    Ok(checksum) => Some(format!("{base_descriptor}#{checksum}")),
                    Err(_) => None,
                };

                let asm = Self::to_core_asm_string(&txout.script_pubkey)?;
                let script_pubkey = ScriptPubkey {
                    asm,
                    hex: txout.script_pubkey.to_hex_string(),
                    descriptor,
                    address: address.as_ref().map(ToString::to_string),
                    type_: Self::get_script_type_label(script).to_string(),
                    // Deprecated in Bitcoin Core v22, require flags in Bitcoin Core.
                    // Set to None as not required for consensus.
                    addresses: None,
                    required_signatures: None,
                };

                Some(GetTxOut {
                    best_block: bestblock_hash.to_string(),
                    confirmations: bestblock_height - height + 1,
                    value: txout.value.to_btc(),
                    script_pubkey,
                    coinbase: is_coinbase,
                })
            }
            _ => None,
        };
        Ok(res)
    }

    /// Computes the necessary information for the RPC `gettxoutproof [txids] blockhash (optional)`
    ///
    /// This function has two paths, when blockhash is inserted and when isn't.
    ///
    /// Specifying the blockhash will make this function go after the block and search
    /// for the transactions inside it, building a merkle proof from the block with its
    /// indexes. Not specifying will redirect it to search for the merkle proof on our
    /// watch-only wallet which may not have the transaction cached.
    ///
    /// Not finding one of the specified transactions will raise [`JsonRpcError::TxNotFound`].
    pub(super) async fn get_txout_proof(
        &self,
        tx_ids: &[Txid],
        blockhash: Option<BlockHash>,
    ) -> Result<GetTxOutProof, JsonRpcError> {
        let block = match blockhash {
            Some(blockhash) => self.get_block_inner(blockhash).await?,
            // Using the first Txid to get the block should be fine since they are expected to all
            // live in the same block, otherwise, theres no way they have a common proof.
            None => self.get_block_by_txid(&tx_ids[0])?,
        };

        // Before building the merkle block we try to remove all txids
        // that aren't present in the block we found, meaning that
        // at least one of the txids doesn't belong to the block which
        // in case needs to make the command fails.
        //
        // this makes the use MerkleBlock::from_block_with_predicate useless.
        let targeted_txids: Vec<Txid> = block
            .txdata
            .iter()
            .filter_map(|tx| {
                let txid = tx.compute_txid();
                if tx_ids.contains(&txid) {
                    Some(txid)
                } else {
                    None
                }
            })
            .collect();

        if targeted_txids.len() != tx_ids.len() {
            return Err(JsonRpcError::TxNotFound);
        };

        let merkle_block = MerkleBlock::from_block_with_predicate(&block, |tx| tx_ids.contains(tx));
        let mut bytes: Vec<u8> = Vec::new();
        merkle_block
            .consensus_encode(&mut bytes)
            .expect("This will raise if a writer error happens");
        Ok(GetTxOutProof(bytes))
    }

    // gettxoutsetinfo
    // gettxspendigprevout
    // importmempool
    // loadtxoutset
    // preciousblock
    // pruneblockchain
    // savemempool
    // scanblocks
    // scantxoutset
    // verifychain
    // verifytxoutproof

    // floresta flavored rpcs. These are not part of the bitcoin rpc spec
    // findtxout
    pub(super) fn find_tx_out(
        &self,
        txid: Txid,
        vout: u32,
        script: ScriptBuf,
        _height: u32,
    ) -> Result<Value, JsonRpcError> {
        if let Some(txout) = self.wallet.get_utxo(&OutPoint { txid, vout }) {
            return Ok(serde_json::to_value(txout).unwrap());
        }

        // if we are on IBD, we don't have any filters to find this txout.
        if self.chain.is_in_ibd() {
            return Err(JsonRpcError::InInitialBlockDownload);
        }

        self.wallet.cache_address(script.clone());
        todo!()
    }

    // getroots
    pub(super) fn get_roots(&self) -> Result<Vec<String>, JsonRpcError> {
        let hashes = self.chain.get_root_hashes();
        Ok(hashes.iter().map(|h| h.to_string()).collect())
    }

    pub(super) fn list_descriptors(&self) -> Result<Vec<String>, JsonRpcError> {
        let descriptors = self
            .wallet
            .get_descriptors()
            .map_err(|e| JsonRpcError::Wallet(e.to_string()))?;
        Ok(descriptors)
    }
}
