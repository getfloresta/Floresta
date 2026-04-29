// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::io;
use std::net::IpAddr;
use std::net::Ipv4Addr;

use bitcoin::Network;
use floresta_wire::bitcoin_socket_addr::BitcoinSocketAddr;
use floresta_wire::bitcoin_socket_addr::DnsResolver;
use libfuzzer_sys::fuzz_target;

/// A no-op resolver to avoid costly name lookup operations inside the fuzz target
///
/// It simply returns localhost to any name
struct FuzzResolver;

impl DnsResolver for FuzzResolver {
    type Error = io::Error;

    fn resolve(&self, _name: &str) -> Result<Vec<IpAddr>, Self::Error> {
        Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)])
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(address) = String::from_utf8(data.to_vec()) else {
        return;
    };

    let Ok(parsed_address) =
        BitcoinSocketAddr::parse_address(&address, Some(Network::Bitcoin), FuzzResolver)
    else {
        return;
    };

    // Make sure the Display version matches what our parser expects.
    let display_repr = parsed_address.to_string();
    display_repr.parse::<BitcoinSocketAddr>().unwrap();
});
