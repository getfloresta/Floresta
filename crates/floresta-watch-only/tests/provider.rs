// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(feature = "bdk-provider")]

mod common;

use std::collections::HashSet;
use std::ops::Add;
use std::str::FromStr;

use bitcoin::Amount;
use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::TxOut;
use bitcoin::Txid;
use floresta_watch_only::models::GetBalanceParams;
use floresta_watch_only::provider::new_provider;
use floresta_watch_only::provider::WalletProvider;
use floresta_watch_only::provider::WalletProviderError;
use floresta_watch_only::provider::WalletProviderEvent;

use crate::common::create_block_with_transaction;
use crate::common::create_block_with_transactions;
use crate::common::create_script_buff;
use crate::common::generate_blocks;
use crate::common::generate_random_path_tmpdir;
use crate::common::TransactionInner;
use crate::common::DESCRIPTOR;
use crate::common::DESCRIPTOR_ID;
use crate::common::DESCRIPTOR_SECOND;
use crate::common::DESCRIPTOR_SECOND_ID;

pub fn get_path_to_test_db() -> String {
    let path = generate_random_path_tmpdir();

    path.add("/provider.db3")
}

fn create_test_provider() -> Box<dyn WalletProvider> {
    new_provider(&get_path_to_test_db(), Network::Regtest, false).unwrap()
}

fn create_test_provider_initialized() -> Box<dyn WalletProvider> {
    let mut provider = create_test_provider();
    provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
        .unwrap();
    provider
        .persist_descriptor(DESCRIPTOR_SECOND_ID, DESCRIPTOR_SECOND)
        .unwrap();

    provider
}

fn add_blocks_to_provider(provider: &dyn WalletProvider, quantity: u32) {
    let last_processed_block = provider.get_last_processed_block().unwrap();
    let blocks = generate_blocks(quantity, Some(last_processed_block.hash));
    let mut height = last_processed_block.height + 1;

    for block in blocks {
        provider.block_process(&block, height).unwrap();
        height += 1;
    }
}

fn check_descriptor_in_keychain(
    provider: &dyn WalletProvider,
    id: &str,
    expected_descriptor: &str,
) {
    let result = provider.get_descriptor(id).unwrap();

    assert_eq!(
        result, expected_descriptor,
        "Descriptor should match expected value"
    );
}

#[test]
fn test_persist_descriptor_initial_creation() {
    let mut provider = create_test_provider();

    provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
        .unwrap();

    check_descriptor_in_keychain(provider.as_ref(), DESCRIPTOR_ID, DESCRIPTOR);
}

#[test]
fn test_persist_descriptor_add_second_descriptor() {
    let mut provider = create_test_provider();

    provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
        .unwrap();

    check_descriptor_in_keychain(provider.as_ref(), DESCRIPTOR_ID, DESCRIPTOR);

    let result = provider.persist_descriptor(DESCRIPTOR_SECOND_ID, DESCRIPTOR_SECOND);

    assert!(result.is_ok(), "Failed to persist second descriptor");

    check_descriptor_in_keychain(provider.as_ref(), DESCRIPTOR_SECOND_ID, DESCRIPTOR_SECOND);
}

#[test]
fn test_persist_descriptor_duplicate_id_fails() {
    let mut provider = create_test_provider();

    provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
        .unwrap();

    let err = provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR_SECOND)
        .unwrap_err();

    assert!(matches!(
        err,
        WalletProviderError::DescriptorAlreadyExists(_)
    ));
}

#[test]
fn test_persist_descriptor_duplicate_descriptor_fails() {
    let mut provider = create_test_provider();

    provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
        .unwrap();

    let err = provider
        .persist_descriptor(DESCRIPTOR_SECOND_ID, DESCRIPTOR)
        .unwrap_err();

    assert!(matches!(
        err,
        WalletProviderError::DescriptorAlreadyExists(_)
    ));
}

#[test]
fn test_persist_multiple_descriptors_sequentially() {
    let mut provider = create_test_provider();

    let descriptor_configs = vec![
            ("receiving", "wpkh(tpubDBxWyYwpXjpaBVxm3UTZYJ7BMzSH45eZvsMge5Bk1UKpUGRgNxoAtQyV5ZumNycg4RRNdWwGb2LEPfSBwPUY4EVNNa2oDUR9vwRNohLjnuL/0/*)#q5xmwtdg"),
            ("change", "wpkh(tpubDBxWyYwgC5Hbz6SYUpPcg3GUAcbtCDAxz5pgXK9hJ4pPGHff9sX1ckjpPCeNJDSNrffArawsmAvTfbKNvxAJBrRaHDCXDcDdbaUU3c7w6cr/0/*)#5zhtjjl7"),
            ("savings", "wpkh(tpubD9iPRr2awBsAyCzKmEC46MMHC8vQAfxK2XmJrpuAgZ4yy1h5rkCEPoomRqFJHqXHWZCdHYghVJmUG1bfUXidh5HevfLWQf44W9BzwKRSWgG/0/*)#m0drp6yl"),
        ];

    for (id, descriptor) in &descriptor_configs {
        provider.persist_descriptor(id, descriptor).unwrap();

        check_descriptor_in_keychain(provider.as_ref(), id, descriptor);
    }
}

#[test]
fn test_list_script_buff() {
    let provider = create_test_provider_initialized();

    let script_bufs = provider.list_script_buff(None).unwrap();

    assert!(
        !script_bufs.is_empty(),
        "Script buffers should not be empty"
    );

    for script_buf in &script_bufs {
        assert!(script_buf.is_p2wpkh(), "Script buffer should be P2WPKH");
    }
}

#[test]
fn test_list_script_buff_with_ids() {
    let provider = create_test_provider_initialized();

    let all_script_bufs = provider.list_script_buff(None).unwrap();
    let receiving_script_bufs = provider
        .list_script_buff(Some(HashSet::from([DESCRIPTOR_ID.to_string()])))
        .unwrap();
    let change_script_bufs = provider
        .list_script_buff(Some(HashSet::from([DESCRIPTOR_SECOND_ID.to_string()])))
        .unwrap();

    assert_eq!(receiving_script_bufs.len(), 30); // Default index is 30, so we should have 30 script buffers for each descriptor
    assert_eq!(change_script_bufs.len(), 30);
    assert_eq!(
        all_script_bufs.len(),
        receiving_script_bufs.len() + change_script_bufs.len()
    );
}

#[test]
fn test_get_transactions_from_empty_wallet() {
    let provider = create_test_provider_initialized();

    let transactions = provider.get_transactions().unwrap();

    // New wallet without transactions should be empty
    assert!(
        transactions.is_empty(),
        "New wallet should have no transactions"
    );
}

#[test]
fn test_get_transaction_not_found() {
    let provider = create_test_provider_initialized();

    let nonexistent_txid =
        Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();

    let result = provider.get_transaction(&nonexistent_txid);

    assert!(
        result.is_err(),
        "Should fail to get nonexistent transaction"
    );
    assert!(matches!(
        result.unwrap_err(),
        WalletProviderError::TransactionNotFound(_)
    ));
}

#[test]
fn test_get_transaction_by_wallet_delegates_to_get_transaction() {
    let provider = create_test_provider_initialized();

    let nonexistent_txid =
        Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();

    let result = provider.get_transaction_by_wallet(
        HashSet::from([DESCRIPTOR_ID.to_string()]),
        &nonexistent_txid,
    );

    assert!(
        result.is_err(),
        "Should delegate to get_transaction and fail for nonexistent txid"
    );
}

#[test]
fn test_get_transactions_by_wallet_delegates_to_get_transactions() {
    let provider = create_test_provider_initialized();

    // With empty wallet, should return empty
    let transactions =
        provider.get_transactions_by_wallet(HashSet::from([DESCRIPTOR_ID.to_string()]));

    assert!(transactions.is_ok(), "Should successfully get transactions");
    assert!(
        transactions.unwrap().is_empty(),
        "New wallet should have no transactions"
    );
}

#[test]
fn test_get_balance_empty_wallet() {
    let provider = create_test_provider_initialized();

    let balance = provider.get_balance(
        HashSet::from([DESCRIPTOR_ID.to_string()]),
        GetBalanceParams {
            minconf: 1,
            avoid_reuse: false,
        },
    );

    assert!(
        balance.is_ok(),
        "Should successfully get balance from empty wallet"
    );
    assert_eq!(
        balance.unwrap(),
        Amount::from_sat(0),
        "Empty wallet should have zero balance"
    );
}

#[test]
fn test_get_balance_with_transaction() {
    fn assert_balance(provider: &dyn WalletProvider, conf: u32, amount: u64) {
        let round = 8;
        for minconf in 0..round {
            let expected = if conf >= minconf { amount } else { 0 };
            let balance = provider
                .get_balance(
                    HashSet::from([DESCRIPTOR_ID.to_string()]),
                    GetBalanceParams {
                        minconf,
                        avoid_reuse: false,
                    },
                )
                .unwrap();
            assert_eq!(
                balance,
                Amount::from_sat(expected),
                "Balance should be {} with minconf {} and conf {}",
                expected,
                minconf,
                conf
            );
        }
    }

    let provider = create_test_provider_initialized();

    // Create a transaction and apply it to the wallet, then check balance
    let tx = TransactionInner {
        outpoint: vec![],
        txo: vec![TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: provider.new_address(DESCRIPTOR_ID).unwrap().script_pubkey(),
        }],
    }
    .to_transaction();

    provider.process_mempool_transactions(vec![&tx]).unwrap();

    assert_balance(provider.as_ref(), 0, 100_000);

    let block = create_block_with_transaction(None, &tx);
    provider.block_process(&block, 0).unwrap();

    assert_balance(provider.as_ref(), 1, 100_000);

    add_blocks_to_provider(provider.as_ref(), 5);

    assert_balance(provider.as_ref(), 6, 100_000);
}

#[test]
fn test_get_balances_empty_wallet() {
    let provider = create_test_provider_initialized();

    let balance = provider
        .get_balances(HashSet::from([DESCRIPTOR_ID.to_string()]))
        .unwrap();

    assert_eq!(balance.immature, Amount::from_sat(0));
    assert_eq!(balance.trusted, Amount::from_sat(0));
    assert_eq!(balance.untrusted_pending, Amount::from_sat(0));
}

#[test]
fn test_get_balance_with_zero_minconf() {
    let provider = create_test_provider_initialized();

    let balance = provider
        .get_balance(
            HashSet::from([DESCRIPTOR_ID.to_string()]),
            GetBalanceParams {
                minconf: 0,
                avoid_reuse: false,
            },
        )
        .unwrap();

    assert_eq!(balance, Amount::from_sat(0));
}

#[test]
fn test_sent_and_received_empty_wallet() {
    let provider = create_test_provider_initialized();

    let nonexistent_txid =
        Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();

    let result = provider.sent_and_received(
        HashSet::from([DESCRIPTOR_ID.to_string()]),
        &nonexistent_txid,
    );

    assert!(result.is_err(), "Should fail for nonexistent transaction");
}

#[test]
fn test_get_txo_with_unspent_filter() {
    let provider = create_test_provider_initialized();

    let outpoint = OutPoint {
        txid: Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
        vout: 0,
    };

    let result = provider.get_txo(&outpoint, Some(false));

    assert!(result.is_ok(), "Should handle unspent filter");
    assert!(
        result.unwrap().is_none(),
        "Should return None for nonexistent UTXO"
    );
}

#[test]
fn test_get_txo_with_spent_filter() {
    let provider = create_test_provider_initialized();

    let outpoint = OutPoint {
        txid: Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
        vout: 0,
    };

    let result = provider.get_txo(&outpoint, Some(true));

    assert!(result.is_ok(), "Should handle spent filter");
    assert!(
        result.unwrap().is_none(),
        "Should return None for nonexistent output"
    );
}

#[test]
fn test_get_txo_with_no_filter() {
    let provider = create_test_provider_initialized();

    let outpoint = OutPoint {
        txid: Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
        vout: 0,
    };

    let result = provider.get_txo(&outpoint, None);

    assert!(result.is_ok(), "Should handle no filter");
    assert!(
        result.unwrap().is_none(),
        "Should return None for nonexistent output"
    );
}

#[test]
fn test_get_script_hash_txos_empty() {
    let provider = create_test_provider_initialized();

    let script = ScriptBuf::new();

    let outputs = provider.get_local_output_by_script(script, None);

    assert!(
        outputs.is_ok(),
        "Should successfully get script hash outputs"
    );
    assert!(
        outputs.unwrap().is_empty(),
        "Empty wallet should have no outputs"
    );
}

#[test]
fn test_get_script_hash_txos_with_spent_filter() {
    let provider = create_test_provider_initialized();

    let script = ScriptBuf::new();

    let outputs_spent = provider.get_local_output_by_script(script.clone(), Some(true));
    let outputs_unspent = provider.get_local_output_by_script(script, Some(false));

    assert!(outputs_spent.is_ok());
    assert!(outputs_unspent.is_ok());
    assert!(outputs_spent.unwrap().is_empty());
    assert!(outputs_unspent.unwrap().is_empty());
}

#[test]
fn test_process_mempool_transactions_empty() {
    let provider = create_test_provider_initialized();

    let events = provider.process_mempool_transactions(vec![]);

    assert!(events.is_ok(), "Should handle empty mempool transactions");
    assert!(
        events.unwrap().is_empty(),
        "Empty transaction list should return empty events"
    );
}

#[test]
fn test_new_address_after_descriptor() {
    let provider = create_test_provider_initialized();

    let address_result = provider.new_address(DESCRIPTOR_ID);

    assert!(
        address_result.is_ok(),
        "Should successfully generate new address"
    );

    let address = address_result.unwrap();
    // Verify it's a valid address by checking it can be converted to string
    let addr_str = address.to_string();
    assert!(
        !addr_str.is_empty(),
        "Address should have valid string representation"
    );
}

#[test]
fn test_new_address_for_each_descriptor() {
    let provider = create_test_provider_initialized();

    let addr1 = provider.new_address(DESCRIPTOR_ID).unwrap();
    let addr2 = provider.new_address(DESCRIPTOR_SECOND_ID).unwrap();

    // Addresses from different descriptors may be different
    // (although they could theoretically be the same in rare cases)
    let addr1_str = addr1.to_string();
    let addr2_str = addr2.to_string();
    assert!(!addr1_str.is_empty());
    assert!(!addr2_str.is_empty());
}

#[test]
fn test_descriptor_persistence_through_reload() {
    let mut provider = create_test_provider();

    // First descriptor
    provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
        .unwrap();

    // Verify it's in the keychain
    check_descriptor_in_keychain(provider.as_ref(), DESCRIPTOR_ID, DESCRIPTOR);

    // Add second descriptor
    provider
        .persist_descriptor(DESCRIPTOR_SECOND_ID, DESCRIPTOR_SECOND)
        .unwrap();

    // Both should be present
    check_descriptor_in_keychain(provider.as_ref(), DESCRIPTOR_ID, DESCRIPTOR);

    check_descriptor_in_keychain(provider.as_ref(), DESCRIPTOR_SECOND_ID, DESCRIPTOR_SECOND);
}

#[test]
fn test_list_script_buff_with_nonexistent_keychain_id() {
    let provider = create_test_provider_initialized();

    let result = provider.list_script_buff(Some(HashSet::from(["nonexistent".to_string()])));

    assert!(result.is_ok(), "Should handle nonexistent keychain ID");
    assert!(
        result.unwrap().is_empty(),
        "Should return empty for nonexistent keychain"
    );
}

#[test]
fn test_list_script_buff_with_multiple_ids() {
    let provider = create_test_provider_initialized();

    let ids = HashSet::from([DESCRIPTOR_ID.to_string(), DESCRIPTOR_SECOND_ID.to_string()]);

    let result = provider.list_script_buff(Some(ids));

    assert!(result.is_ok());
    assert!(
        !result.unwrap().is_empty(),
        "Should have scripts for both descriptors"
    );
}

#[test]
fn test_keyring_error_handling() {
    let mut provider = create_test_provider();

    // First descriptor should work
    assert!(provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
        .is_ok());

    // Try to add descriptor with duplicate ID
    let dup_result = provider.persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR_SECOND);

    assert!(dup_result.is_err());
    assert!(matches!(
        dup_result.unwrap_err(),
        WalletProviderError::DescriptorAlreadyExists(_)
    ));
}

#[test]
fn test_descriptor_descriptor_conflict() {
    let mut provider = create_test_provider();

    provider
        .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
        .unwrap();

    // Same descriptor string, different ID, should also be rejected
    let result = provider.persist_descriptor(DESCRIPTOR_SECOND_ID, DESCRIPTOR);

    assert!(result.is_err());
}

#[test]
fn test_process_mempool_transactions() {
    let provider = create_test_provider_initialized();

    let wallet_tx = TransactionInner {
        outpoint: vec![],
        txo: vec![TxOut {
            value: Amount::from_sat(10_000_000),
            script_pubkey: provider.new_address(DESCRIPTOR_ID).unwrap().script_pubkey(),
        }],
    }
    .to_transaction();

    let wallet_tx_spent = TransactionInner {
        outpoint: vec![OutPoint {
            txid: wallet_tx.compute_txid(),
            vout: 0,
        }],
        txo: vec![],
    }
    .to_transaction();

    let non_wallet_tx = TransactionInner {
        outpoint: vec![],
        txo: vec![TxOut {
            value: Amount::from_sat(10_000_000),
            script_pubkey: create_script_buff(),
        }],
    }
    .to_transaction();

    let non_wallet_tx_spent = TransactionInner {
        outpoint: vec![OutPoint {
            txid: non_wallet_tx.compute_txid(),
            vout: 0,
        }],
        txo: vec![],
    }
    .to_transaction();

    let events = provider
        .process_mempool_transactions(vec![
            &wallet_tx,
            &wallet_tx_spent,
            &non_wallet_tx,
            &non_wallet_tx_spent,
        ])
        .unwrap();

    assert_eq!(events.len(), 3);

    let event = WalletProviderEvent::UpdateTransaction {
        tx: wallet_tx.clone(),
        output: wallet_tx.clone().output[0].clone(),
    };
    assert_eq!(events[0], event);

    let event = WalletProviderEvent::UnconfirmedTransactionInBlock {
        tx: wallet_tx.clone(),
    };
    assert_eq!(events[1], event);

    let event = WalletProviderEvent::UnconfirmedTransactionInBlock {
        tx: wallet_tx_spent.clone(),
    };
    assert_eq!(events[2], event);
}

#[test]
fn test_process_mempool_transactions_with_non_wallet_tx() {
    let provider = create_test_provider_initialized();

    let non_wallet_tx = TransactionInner {
        outpoint: vec![],
        txo: vec![TxOut {
            value: Amount::from_sat(10_000_000),
            script_pubkey: create_script_buff(),
        }],
    }
    .to_transaction();

    let non_wallet_tx_spent = TransactionInner {
        outpoint: vec![OutPoint {
            txid: non_wallet_tx.compute_txid(),
            vout: 0,
        }],
        txo: vec![],
    }
    .to_transaction();

    let events = provider
        .process_mempool_transactions(vec![&non_wallet_tx, &non_wallet_tx_spent])
        .unwrap();

    assert!(
        events.is_empty(),
        "Non-wallet transaction should not generate events"
    );
}

#[test]
fn test_process_block() {
    let provider = create_test_provider_initialized();

    let wallet_tx = TransactionInner {
        outpoint: vec![],
        txo: vec![TxOut {
            value: Amount::from_sat(10_000_000),
            script_pubkey: provider.new_address(DESCRIPTOR_ID).unwrap().script_pubkey(),
        }],
    }
    .to_transaction();

    let wallet_tx_spent = TransactionInner {
        outpoint: vec![OutPoint {
            txid: wallet_tx.compute_txid(),
            vout: 0,
        }],
        txo: vec![],
    }
    .to_transaction();

    let non_wallet_tx = TransactionInner {
        outpoint: vec![],
        txo: vec![TxOut {
            value: Amount::from_sat(10_000_000),
            script_pubkey: create_script_buff(),
        }],
    }
    .to_transaction();

    let non_wallet_tx_spent = TransactionInner {
        outpoint: vec![OutPoint {
            txid: non_wallet_tx.compute_txid(),
            vout: 0,
        }],
        txo: vec![],
    }
    .to_transaction();

    let block = create_block_with_transactions(
        None,
        vec![
            wallet_tx.clone(),
            wallet_tx_spent.clone(),
            non_wallet_tx.clone(),
            non_wallet_tx_spent.clone(),
        ],
    );

    let events = provider.block_process(&block, 0).unwrap();

    assert_eq!(events.len(), 3);

    let event = WalletProviderEvent::UpdateTransaction {
        tx: wallet_tx.clone(),
        output: wallet_tx.clone().output[0].clone(),
    };
    assert_eq!(events[0], event);

    let event = WalletProviderEvent::ConfirmedTransaction {
        tx: wallet_tx.clone(),
    };
    assert_eq!(events[1], event);

    let event = WalletProviderEvent::ConfirmedTransaction {
        tx: wallet_tx_spent.clone(),
    };
    assert_eq!(events[2], event);
}
