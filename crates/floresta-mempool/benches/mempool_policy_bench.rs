// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::hint::black_box;

use bitcoin::absolute;
use bitcoin::hashes::Hash;
use bitcoin::Amount;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::Sequence;
use bitcoin::Transaction;
use bitcoin::TxIn;
use bitcoin::TxOut;
use bitcoin::Txid;
use bitcoin::Witness;
use criterion::criterion_group;
use criterion::criterion_main;
use criterion::Criterion;
use floresta_mempool::Mempool;
use floresta_mempool::MempoolError;
use floresta_mempool::MempoolPolicy;
use floresta_mempool::UtxoData;

fn p2wpkh_script(tag: u8) -> ScriptBuf {
    let mut bytes = Vec::with_capacity(22);
    bytes.extend_from_slice(&[0x00, 0x14]);
    bytes.extend_from_slice(&[tag; 20]);
    ScriptBuf::from_bytes(bytes)
}

fn make_utxo(value: u64, script_pubkey: ScriptBuf) -> UtxoData {
    UtxoData {
        txout: TxOut {
            value: Amount::from_sat(value),
            script_pubkey,
        },
        is_coinbase: false,
        creation_height: 0,
        creation_time: 0,
    }
}

fn input(previous_output: OutPoint) -> TxIn {
    TxIn {
        previous_output,
        script_sig: ScriptBuf::new(),
        sequence: Sequence::MAX,
        witness: Witness::new(),
    }
}

fn prevout(tag: u8) -> OutPoint {
    OutPoint {
        txid: Txid::from_slice(&[tag; 32]).unwrap(),
        vout: 0,
    }
}

fn standard_tx(
    tag: u8,
    input_value: u64,
    output_value: u64,
    script_sig_len: usize,
) -> (Transaction, HashMap<OutPoint, UtxoData>) {
    let previous_output = prevout(tag);
    let mut context = HashMap::new();
    context.insert(previous_output, make_utxo(input_value, p2wpkh_script(tag)));

    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: absolute::LockTime::from_consensus(0),
        input: vec![TxIn {
            script_sig: ScriptBuf::from_bytes(vec![1; script_sig_len]),
            ..input(previous_output)
        }],
        output: vec![TxOut {
            value: Amount::from_sat(output_value),
            script_pubkey: p2wpkh_script(tag.wrapping_add(1)),
        }],
    };

    (tx, context)
}

fn bench_accept_to_mempool(c: &mut Criterion) {
    c.bench_function("accept_to_mempool/policy_pass", |b| {
        b.iter(|| {
            let (tx, context) = standard_tx(1, 100_000, 98_000, 0);
            let mut mempool = Mempool::new(10_000_000);
            black_box(mempool.accept_to_mempool(tx, context)).unwrap();
        });
    });

    c.bench_function("accept_to_mempool/fee_too_low", |b| {
        b.iter(|| {
            let (tx, context) = standard_tx(2, 100_000, 99_999, 0);
            let mut mempool = Mempool::with_policy(
                10_000_000,
                MempoolPolicy {
                    min_relay_fee_sat_per_vbyte: 5,
                    ..MempoolPolicy::default()
                },
            );
            assert!(matches!(
                black_box(mempool.accept_to_mempool(tx, context)),
                Err(MempoolError::FeeTooLow)
            ));
        });
    });

    c.bench_function("accept_to_mempool/exceeds_max_weight", |b| {
        b.iter(|| {
            let (tx, context) = standard_tx(3, 500_000, 100_000, 80);
            let mut mempool = Mempool::with_policy(
                10_000_000,
                MempoolPolicy {
                    min_relay_fee_sat_per_vbyte: 0,
                    max_standard_tx_weight: 200,
                    max_standard_script_sig_size: 1_650,
                    ..MempoolPolicy::default()
                },
            );
            assert!(matches!(
                black_box(mempool.accept_to_mempool(tx, context)),
                Err(MempoolError::ExceedsMaxWeight)
            ));
        });
    });
}

criterion_group!(benches, bench_accept_to_mempool);
criterion_main!(benches);
