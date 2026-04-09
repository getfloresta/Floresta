// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adaptation of Bitcoin Core's `src/test/transaction_tests.cpp`, using the vendored JSON test
//! vectors in `testdata/bitcoin-core`. We parse them and check the flags that are supported by
//! Floresta's consensus.

#![cfg(all(feature = "bitcoinkernel", feature = "test-utils"))]

mod util;

use std::collections::HashMap;

use bitcoin::OutPoint;
use bitcoin::Transaction;
use bitcoinkernel::VERIFY_ALL;
use floresta_chain::pruned_utreexo::consensus::Consensus;
use floresta_chain::pruned_utreexo::utxo_data::UtxoData;
use floresta_chain::BlockchainError;
use serde_json::Value;
use util::exclude_individual_flags;
use util::fill_flags;
use util::fmt_shift_flags;
use util::trim_flags;
use util::VERIFY_FLAGS_COUNT;

// The dummy height that we use for all the test transactions
const TX_HEIGHT: u32 = 100_000;

/// Defines the types for auto-deserializing the test vectors and the required conversions.
mod parse {
    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::Amount;
    use bitcoin::TxOut;
    use bitcoin::Txid;
    use serde::Deserialize;
    use util::parse_flags;
    use util::parse_script;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct TestCase {
        prevouts: Vec<PrevOut>,
        tx_hex: String,
        verify_flags: String, // comma-separated verifyFlags to use or exclude
    }

    #[derive(Debug, Deserialize)]
    struct PrevOut {
        txid: Txid,
        vout: i64,
        spk: String, // scriptPubKey in asm format
        #[serde(default)]
        amount: Option<Amount>, // `None` for pre-SegWit data
    }

    impl From<PrevOut> for (OutPoint, UtxoData) {
        fn from(prev: PrevOut) -> Self {
            let txid = prev.txid;
            let is_coinbase = prev.vout == -1;

            let vout = match is_coinbase {
                true => u32::MAX,
                false => prev.vout as u32,
            };

            let utxo_data = UtxoData {
                txout: TxOut {
                    value: prev.amount.unwrap_or(Amount::ONE_BTC * 100),
                    script_pubkey: parse_script(&prev.spk).unwrap(),
                },
                is_coinbase,
                creation_height: 0.into(),
                creation_time: 0,
            };

            (OutPoint { txid, vout }, utxo_data)
        }
    }

    // The tested transaction with coins it spends, and the flags to use/exclude
    type ParsedCase = (Transaction, HashMap<OutPoint, UtxoData>, u32);

    pub fn test_vectors(data: &str) -> Vec<(ParsedCase, Value)> {
        // Top-level is an array; keep each original entry (Value) for diagnostics
        let rows: Vec<Value> = serde_json::from_str(data).expect("top-level JSON array");

        rows.into_iter()
            .filter_map(|entry| {
                // Only keep entries that deserialize as our TestCase (all except comment strings)
                let case: TestCase = serde_json::from_value(entry.clone()).ok()?;

                let tx: Transaction = deserialize_hex(&case.tx_hex).unwrap();
                let flags = parse_flags(&case.verify_flags);
                let mut coins: HashMap<_, _> = case.prevouts.into_iter().map(Into::into).collect();

                let out_value = tx
                    .output
                    .iter()
                    .fold(Amount::ZERO, |acc, out| acc + out.value);

                // If a test case tries to spend the theoretical maximum amount, make sure the
                // input has enough money as well.
                if out_value == Amount::MAX_MONEY {
                    coins
                        .iter_mut()
                        .for_each(|(_, utxo)| utxo.txout.value = Amount::MAX_MONEY);
                }

                Some(((tx, coins, flags), entry))
            })
            .collect()
    }
}

fn verify_tx(
    tx: &Transaction,
    coins: &HashMap<OutPoint, UtxoData>,
    flags: u32,
) -> Result<(), BlockchainError> {
    if tx.is_coinbase() {
        return Ok(Consensus::verify_coinbase(tx)?);
    }
    let mut coins = coins.clone();
    Consensus::verify_transaction(tx, &mut coins, TX_HEIGHT, true, flags).map(|_| ())
}

/// Assert that `verify_tx(tx, coins, flags)` succeeds when `expected` and fails when `!expected`.
/// This function helps debugging all the test data when the assertion fails.
#[track_caller]
pub fn assert_tx(
    tx: &Transaction,
    coins: &HashMap<OutPoint, UtxoData>,
    flags: u32,
    should_pass: bool,
    raw: &Value,
) {
    let res = verify_tx(tx, coins, flags);
    match (res, should_pass) {
        (Err(e), true) => panic!(
            "Tx unexpectedly failed with flags = {}\nerror: {:?}\n\n{:#?}\n\n{:#?}\n\nRaw JSON vector: {}",
            fmt_shift_flags(flags), e, tx, coins, raw
        ),
        (Ok(_), false) => panic!(
            "Tx unexpectedly passed with flags = {}\n\n{:#?}\n\n{:#?}\n\nRaw JSON vector: {}",
            fmt_shift_flags(flags), tx, coins, raw
        ),
        _ => {} // expected fail or success
    }
}

fn rand_subset(flags: u32) -> u32 {
    // Keep a random subset of enabled bits
    trim_flags(flags & rand::random::<u32>())
}

fn rand_superset(flags: u32) -> u32 {
    // Add random extra bits to the enabled set
    fill_flags(flags | rand::random::<u32>())
}

#[test]
fn verify_tx_valid() {
    let file_data = include_str!("../testdata/bitcoin-core/data/tx_valid.json");
    let test_vectors = parse::test_vectors(file_data);
    assert_eq!(test_vectors.len(), 120, "Expecting 120 test cases");

    for ((tx, coins_map, excluded_flags), json) in test_vectors {
        // Use the JSON's *excluded* flags to compute the enabled ones
        let flags = VERIFY_ALL & !excluded_flags;
        // Sanity-check
        assert_eq!(fill_flags(flags), flags, "Bad test flags: {json:?}");

        // 1) Under those flags the tx *must* pass
        assert_tx(&tx, &coins_map, flags, true, &json);

        for bit in (0..VERIFY_FLAGS_COUNT).map(|i| 1 << i) {
            // 2a) Disabling each flag in turn should still pass
            let flags_less = trim_flags(flags & !bit);
            assert_tx(&tx, &coins_map, flags_less, true, &json);

            // 2b) Any random extra-disable combo must pass
            assert_tx(&tx, &coins_map, rand_subset(flags), true, &json);
        }

        // 3) Check that flags are maximal: adding back *any* excluded flag must fail
        if tx.is_coinbase() {
            // Skipping a valid coinbase (it won't fail because the script is not evaluated)
            continue;
        }
        for flags_excl_one in exclude_individual_flags(excluded_flags) {
            let flags_more = trim_flags(!flags_excl_one);
            assert_tx(&tx, &coins_map, flags_more, false, &json);
        }
    }
}

#[test]
fn verify_tx_invalid() {
    let file_data = include_str!("../testdata/bitcoin-core/data/tx_invalid.json");
    let test_vectors = parse::test_vectors(file_data);
    assert_eq!(test_vectors.len(), 93, "Expecting 93 test cases");

    // In this test the flags are the list of *enabled* flags
    for ((tx, coins_map, flags), json) in test_vectors {
        // Sanity-check
        assert_eq!(fill_flags(flags), flags, "Bad test flags: {json:?}");

        // 1) Under exactly those flags the tx *must* fail
        assert_tx(&tx, &coins_map, flags, false, &json);

        for bit in (0..VERIFY_FLAGS_COUNT).map(|i| 1 << i) {
            // 2a) Enabling any additional flag must still fail
            let flags_more = fill_flags(flags | bit);
            assert_tx(&tx, &coins_map, flags_more, false, &json);

            // 2b) Any random extra-enable combo must fail
            assert_tx(&tx, &coins_map, rand_superset(flags), false, &json);
        }

        // 3) Check that flags are minimal: removing *any* enabled flag makes it succeed
        for flags_less in exclude_individual_flags(flags) {
            // Skip unsupported libbitcoinkernel flags to avoid a non-validation failure
            if (flags_less & !VERIFY_ALL) != 0 {
                continue;
            }
            assert_tx(&tx, &coins_map, flags_less, true, &json);
        }
    }
}
