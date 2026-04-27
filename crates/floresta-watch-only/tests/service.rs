// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(all(feature = "bdk-provider", feature = "sqlite"))]
mod common;

use bitcoin::Amount;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::Transaction;
use bitcoin::TxOut;
use floresta_watch_only::models::ImportDescriptor;
use floresta_watch_only::service::new_wallet;
use floresta_watch_only::service::Wallet;

use crate::common::create_block_with_coinbase;
use crate::common::create_block_with_transaction;
use crate::common::generate_blocks;
use crate::common::generate_random_path_tmpdir;
use crate::common::TransactionInner;
use crate::common::DESCRIPTOR;
use crate::common::DESCRIPTOR_SECOND;

const WALLET_NAME: &str = "test_wallet";
const AMOUNT: Amount = Amount::from_sat(10_000_000);

fn create_wallet() -> Box<dyn Wallet> {
    let data_dir = generate_random_path_tmpdir();
    new_wallet(&data_dir, Network::Bitcoin).expect("Failed to create wallet")
}

fn create_wallet_initialized() -> Box<dyn Wallet> {
    let data_dir = generate_random_path_tmpdir();
    let wallet = new_wallet(&data_dir, Network::Regtest).expect("Failed to create wallet");

    wallet.create_wallet(WALLET_NAME).unwrap();

    let descriptor = ImportDescriptor {
        descriptor: DESCRIPTOR.to_string(),
        label: Some("receiving".to_string()),
        is_active: true,
        is_change: false,
    };

    wallet.push_descriptor(&descriptor).unwrap();

    let descriptor = ImportDescriptor {
        descriptor: DESCRIPTOR_SECOND.to_string(),
        label: Some("change".to_string()),
        is_active: true,
        is_change: true,
    };

    wallet.push_descriptor(&descriptor).unwrap();

    wallet
}

fn create_my_output(wallet: &dyn Wallet, is_change: bool) -> TxOut {
    TxOut {
        value: AMOUNT,
        script_pubkey: wallet.new_address(is_change).unwrap().script_pubkey(),
    }
}

fn create_spent_transaction(
    wallet: &dyn Wallet,
    outpoint: OutPoint,
    my_output: Option<bool>,
) -> Transaction {
    let mut tx_inner = TransactionInner {
        outpoint: vec![outpoint],
        txo: vec![],
    };

    if let Some(is_change) = my_output {
        tx_inner.txo.push(create_my_output(wallet, is_change));
    }

    tx_inner.to_transaction()
}

fn create_transaction(wallet: &dyn Wallet, is_change: bool) -> Transaction {
    let tx_inner = TransactionInner {
        outpoint: vec![],
        txo: vec![create_my_output(wallet, is_change)],
    };

    tx_inner.to_transaction()
}

fn create_block_with_wallet_transaction(
    wallet: &dyn Wallet,
    prevhash: Option<BlockHash>,
    is_change: bool,
) -> (Block, Transaction) {
    let my_transaction = create_transaction(wallet, is_change);

    let block = create_block_with_transaction(prevhash, &my_transaction);
    (block, my_transaction)
}

fn create_block_with_wallet_transaction_and_spend(
    wallet: &dyn Wallet,
    prevhash: Option<BlockHash>,
    outpoint: OutPoint,
    is_returned: Option<bool>,
) -> Block {
    let spent_transaction = create_spent_transaction(wallet, outpoint, is_returned);

    create_block_with_transaction(prevhash, &spent_transaction)
}

fn create_block_with_wallet_transaction_coinbase(
    wallet: &dyn Wallet,
    prevhash: Option<BlockHash>,
    is_change: bool,
) -> Block {
    let txo = create_my_output(wallet, is_change);
    create_block_with_coinbase(prevhash, txo.script_pubkey, AMOUNT.to_sat())
}

fn mine_blocks(wallet: &dyn Wallet, count: u32) {
    let last_check_point = wallet.get_balances().unwrap().last_processed_block;
    let current_prev_hash = Some(last_check_point.hash);
    let mut current_height = last_check_point.height + 1;

    let blocks = generate_blocks(count, current_prev_hash);
    for block in blocks {
        wallet.process_block(&block, current_height).unwrap();
        current_height += 1;
    }
}

#[test]
fn test_wallet_creation() {
    // Create wallet service
    let wallet = create_wallet();

    // Create a new wallet
    wallet
        .create_wallet("test_wallet")
        .expect("Failed to create wallet");

    // Verify wallet was created
    let result = wallet.get_descriptors().unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_wallet_initialization() {
    // Create wallet service
    let wallet = create_wallet_initialized();

    // Verify descriptors were added
    let result = wallet.get_descriptors().unwrap();

    assert_eq!(result.len(), 2);
    for descriptor in [DESCRIPTOR, DESCRIPTOR_SECOND] {
        assert!(result.iter().any(|d| d == descriptor));
    }
}

#[test]
fn test_wallet_balances_empty() {
    // Create wallet service
    let wallet = create_wallet_initialized();

    // Verify balance is zero
    let balance = wallet.get_balances().unwrap();

    let amount = Amount::from_sat(0);

    assert_eq!(balance.total(), amount);
    assert_eq!(balance.trusted, amount);
    assert_eq!(balance.untrusted_pending, amount);
    assert_eq!(balance.immature, amount);
    assert_eq!(balance.used, None);
    assert_eq!(balance.last_processed_block.height, 0);
}

#[test]
fn test_wallet_balances_coinbase() {
    // Create wallet service
    let wallet = create_wallet_initialized();

    // Create a block with a transaction that pays to the wallet
    let block = create_block_with_wallet_transaction_coinbase(wallet.as_ref(), None, false);

    // Process the block
    wallet.process_block(&block, 0).unwrap();

    // Verify balance is updated
    let balance = wallet.get_balances().unwrap();
    assert_eq!(balance.total(), AMOUNT);
    assert_eq!(balance.trusted, Amount::from_sat(0));
    assert_eq!(balance.untrusted_pending, Amount::from_sat(0));
    assert_eq!(balance.immature, AMOUNT);
    assert_eq!(balance.used, None);
    assert_eq!(balance.last_processed_block.height, 0);
    assert_eq!(balance.last_processed_block.hash, block.block_hash());

    mine_blocks(wallet.as_ref(), 101);

    // Verify balance is updated
    let balance = wallet.get_balances().unwrap();
    assert_eq!(balance.total(), AMOUNT);
    assert_eq!(balance.trusted, AMOUNT);
    assert_eq!(balance.untrusted_pending, Amount::from_sat(0));
    assert_eq!(balance.immature, Amount::from_sat(0));
    assert_eq!(balance.used, None);
    assert_eq!(balance.last_processed_block.height, 101);
}

#[test]
fn test_wallet_balances_with_transaction() {
    // Create wallet service
    let wallet = create_wallet_initialized();

    // Create a block with a transaction that pays to the wallet
    let (block, _) = create_block_with_wallet_transaction(wallet.as_ref(), None, false);

    // Process the block
    wallet.process_block(&block, 0).unwrap();

    // Verify balance is updated
    let balance = wallet.get_balances().unwrap();
    assert_eq!(balance.total(), AMOUNT);
    assert_eq!(balance.trusted, AMOUNT);
    assert_eq!(balance.untrusted_pending, Amount::from_sat(0));
    assert_eq!(balance.immature, Amount::from_sat(0));
    assert_eq!(balance.used, None);
    assert_eq!(balance.last_processed_block.height, 0);
    assert_eq!(balance.last_processed_block.hash, block.block_hash());

    mine_blocks(wallet.as_ref(), 101);

    // Verify balance is updated
    let balance = wallet.get_balances().unwrap();
    assert_eq!(balance.total(), AMOUNT);
    assert_eq!(balance.trusted, AMOUNT);
    assert_eq!(balance.untrusted_pending, Amount::from_sat(0));
    assert_eq!(balance.immature, Amount::from_sat(0));
    assert_eq!(balance.used, None);
    assert_eq!(balance.last_processed_block.height, 101);
}

#[test]
fn test_wallet_balances_with_transaction_spent() {
    // Create wallet service
    let wallet = create_wallet_initialized();

    // Create a block with a transaction that pays to the wallet
    let (block, tx) = create_block_with_wallet_transaction(wallet.as_ref(), None, false);

    // Process the block
    wallet.process_block(&block, 0).unwrap();

    // Verify balance is updated
    let balance = wallet.get_balances().unwrap();
    assert_eq!(balance.total(), AMOUNT);
    assert_eq!(balance.trusted, AMOUNT);
    assert_eq!(balance.untrusted_pending, Amount::from_sat(0));
    assert_eq!(balance.immature, Amount::from_sat(0));
    assert_eq!(balance.used, None);
    assert_eq!(balance.last_processed_block.height, 0);
    assert_eq!(balance.last_processed_block.hash, block.block_hash());

    let outpoint = OutPoint {
        txid: tx.compute_txid(),
        vout: 0,
    };
    let block = create_block_with_wallet_transaction_and_spend(
        wallet.as_ref(),
        Some(block.block_hash()),
        outpoint,
        None,
    );
    wallet.process_block(&block, 1).unwrap();
    let expect_amount = Amount::from_sat(0);

    // Verify balance is updated
    let balance = wallet.get_balances().unwrap();
    assert_eq!(balance.total(), expect_amount);
    assert_eq!(balance.trusted, expect_amount);
    assert_eq!(balance.untrusted_pending, expect_amount);
    assert_eq!(balance.immature, expect_amount);
    assert_eq!(balance.used, None);
    assert_eq!(balance.last_processed_block.height, 1);
    assert_eq!(balance.last_processed_block.hash, block.block_hash());
}
