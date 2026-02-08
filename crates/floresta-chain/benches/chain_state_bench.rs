use std::collections::HashMap;
use std::fs::File;
use std::io::Cursor;

use bitcoin::block::Header as BlockHeader;
use bitcoin::consensus::deserialize;
use bitcoin::consensus::Decodable;
use bitcoin::Block;
use bitcoin::Network;
use bitcoin::OutPoint;
use criterion::criterion_group;
use criterion::criterion_main;
use criterion::BatchSize;
use criterion::Criterion;
use criterion::SamplingMode;
use floresta_chain::pruned_utreexo::utxo_data::UtxoData;
use floresta_chain::pruned_utreexo::UpdatableChainstate;
use floresta_chain::AssumeValidArg;
use floresta_chain::ChainState;
use floresta_chain::FlatChainStore;
use floresta_chain::FlatChainStoreConfig;
use rustreexo::accumulator::proof::Proof;

/// Reads the first 151 blocks (or 150 blocks on top of genesis) from `regtest_blocks.txt`
fn read_blocks_txt() -> Vec<Block> {
    let blocks: Vec<_> = include_str!("../testdata/regtest_blocks.txt")
        .lines()
        .take(151)
        .map(|b| deserialize(&hex::decode(b).unwrap()).unwrap())
        .collect();

    assert_eq!(
        blocks.len(),
        151,
        "Expected 151 blocks in regtest_blocks.txt"
    );
    blocks
}

/// Returns the first 10,237 mainnet headers
fn read_mainnet_headers() -> Vec<BlockHeader> {
    let file = include_bytes!("../testdata/headers.zst");
    let uncompressed: Vec<u8> = zstd::decode_all(Cursor::new(file)).unwrap();
    let mut buffer = uncompressed.as_slice();

    // Read all headers into a vector
    let mut headers = Vec::new();
    while let Ok(header) = BlockHeader::consensus_decode(&mut buffer) {
        headers.push(header);
    }
    assert_eq!(
        headers.len(),
        10_237,
        "Expected 10,237 headers in headers.zst"
    );

    headers
}

fn setup_test_chain(
    network: Network,
    assume_valid_arg: AssumeValidArg,
) -> ChainState<FlatChainStore> {
    let test_id = rand::random::<u64>();
    let config = FlatChainStoreConfig {
        block_index_size: Some(32_768),
        headers_file_size: Some(32_768),
        fork_file_size: Some(10_000), // Will be rounded up to 16,384
        cache_size: Some(10),
        file_permission: Some(0o660),
        path: format!("./tmp-db/{test_id}/"),
    };

    let chainstore = FlatChainStore::new(config).unwrap();
    ChainState::new(chainstore, network, assume_valid_arg)
}

fn decode_block_and_inputs(
    block_file: File,
    stxos_file: File,
) -> (Block, HashMap<OutPoint, UtxoData>) {
    let block_bytes = zstd::decode_all(block_file).unwrap();
    let block: Block = deserialize(&block_bytes).unwrap();

    // Get utxos spent in the block
    let stxos_bytes = zstd::decode_all(stxos_file).unwrap();
    let mut stxos: Vec<UtxoData> =
        serde_json::from_slice(&stxos_bytes).expect("Failed to deserialize JSON");

    let inputs = block
        .txdata
        .iter()
        .skip(1) // Skip the coinbase transaction
        .flat_map(|tx| &tx.input)
        .map(|txin| (txin.previous_output, stxos.remove(0)))
        .collect();

    assert!(stxos.is_empty(), "Moved all stxos to the inputs map");

    (block, inputs)
}

fn initialize_chainstore_benchmark(c: &mut Criterion) {
    c.bench_function("initialize_chainstore", |b| {
        b.iter_batched(
            || {
                let test_id = rand::random::<u64>();
                FlatChainStoreConfig::new(format!("./tmp-db/{test_id}/"))
            },
            |config| FlatChainStore::new(config).unwrap(),
            BatchSize::SmallInput,
        )
    });
}

fn accept_mainnet_headers_benchmark(c: &mut Criterion) {
    let headers = read_mainnet_headers();

    c.bench_function("accept_10k_mainnet_headers", |b| {
        b.iter_batched(
            || setup_test_chain(Network::Bitcoin, AssumeValidArg::Hardcoded),
            |chain| {
                headers
                    .iter()
                    .for_each(|header| chain.accept_header(*header).unwrap())
            },
            BatchSize::SmallInput,
        )
    });
}

fn accept_headers_benchmark(c: &mut Criterion) {
    let blocks = read_blocks_txt();

    c.bench_function("accept_150_headers", |b| {
        b.iter_batched(
            || setup_test_chain(Network::Regtest, AssumeValidArg::Disabled),
            |chain| {
                blocks
                    .iter()
                    .for_each(|block| chain.accept_header(block.header).unwrap());
            },
            BatchSize::SmallInput,
        )
    });
}

fn connect_blocks_benchmark(c: &mut Criterion) {
    let blocks = read_blocks_txt();

    let setup_chain = || {
        let chain = setup_test_chain(Network::Regtest, AssumeValidArg::Disabled);
        // We need to accept the headers before connecting blocks
        blocks
            .iter()
            .for_each(|block| chain.accept_header(block.header).unwrap());

        chain
    };

    c.bench_function("connect_150_blocks", |b| {
        b.iter_batched(
            setup_chain,
            |chain| {
                blocks.iter().for_each(|block| {
                    chain
                        .connect_block(block, Proof::default(), HashMap::new(), Vec::new())
                        .unwrap();
                })
            },
            BatchSize::SmallInput,
        )
    });
}

fn validate_full_block_benchmark(c: &mut Criterion) {
    let block_file = File::open("./testdata/block_866342/raw.zst").unwrap();
    let stxos_file = File::open("./testdata/block_866342/spent_utxos.zst").unwrap();
    let (block, inputs) = decode_block_and_inputs(block_file, stxos_file);

    let chain = setup_test_chain(Network::Bitcoin, AssumeValidArg::Disabled);

    c.bench_function("validate_block_866342", |b| {
        b.iter_batched(
            || inputs.clone(),
            |inputs| chain.validate_block_no_acc(&block, 866342, inputs).unwrap(),
            BatchSize::LargeInput,
        )
    });
}

fn validate_many_inputs_block_benchmark(c: &mut Criterion) {
    if std::env::var("EXPENSIVE_BENCHES").is_err() {
        println!(
            "validate_many_inputs_block_benchmark ... \x1b[33mskipped\x1b[0m\n\
            > Set EXPENSIVE_BENCHES=1 to include this benchmark\n"
        );

        return;
    }

    let block_file = File::open("./testdata/block_367891/raw.zst").unwrap();
    let stxos_file = File::open("./testdata/block_367891/spent_utxos.zst").unwrap();
    let (block, inputs) = decode_block_and_inputs(block_file, stxos_file);

    let chain = setup_test_chain(Network::Bitcoin, AssumeValidArg::Disabled);

    // Create a group with the lowest possible sample size, as validating this block is very slow
    let mut group = c.benchmark_group("validate_block_367891");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);

    group.bench_function("validate_block_367891", |b| {
        b.iter_batched(
            || inputs.clone(),
            |inputs| chain.validate_block_no_acc(&block, 367891, inputs).unwrap(),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn chainstore_checksum_benchmark(c: &mut Criterion) {
    use floresta_chain::ChainStore;
    use floresta_chain::DiskBlockHeader;

    let headers = read_mainnet_headers();

    let setup_chain = || {
        let test_id = rand::random::<u64>();
        // The default config with the big mmap sizes that we use in `florestad`
        let config = FlatChainStoreConfig::new(format!("./tmp-db/{test_id}/"));
        let mut chainstore = FlatChainStore::new(config).unwrap();

        headers.iter().enumerate().for_each(|(i, header)| {
            let height = i as u32;
            let disk_header = DiskBlockHeader::HeadersOnly(*header, height);

            chainstore.save_header(&disk_header).unwrap();
            chainstore
                .update_block_index(height, header.block_hash())
                .unwrap();
        });

        chainstore
    };

    c.bench_function("flat_chainstore_checksum", |b| {
        b.iter_batched(
            setup_chain,
            |chainstore| chainstore.compute_checksum(),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    initialize_chainstore_benchmark,
    accept_mainnet_headers_benchmark,
    accept_headers_benchmark,
    connect_blocks_benchmark,
    validate_full_block_benchmark,
    validate_many_inputs_block_benchmark,
    chainstore_checksum_benchmark
);
criterion_main!(benches);
