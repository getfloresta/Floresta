#![no_main]

use bitcoin::consensus::encode;
use bitcoin::p2p::address::AddrV2Message;
use bitcoin::p2p::ServiceFlags;
use floresta_wire::address_man::AddressMan;
use floresta_wire::address_man::LocalAddress;
use floresta_wire::address_man::ReachableNetworks;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut address_man =
        AddressMan::new(None, &[ReachableNetworks::IPv4, ReachableNetworks::IPv6]);
    let addrv2_msg_vec = encode::deserialize::<Vec<AddrV2Message>>(data);
    match addrv2_msg_vec {
        Err(_) => {}
        Ok(addrv2_vec) => {
            let mut local_addresses = Vec::<LocalAddress>::new();
            for addrv2 in addrv2_vec {
                let local_address = LocalAddress::from(addrv2);
                local_addresses.push(local_address);
            }
            address_man.push_addresses(&local_addresses);
        }
    }
    address_man.get_addresses_to_send();
    let available_flags = [
        ServiceFlags::NETWORK,
        ServiceFlags::GETUTXO,
        ServiceFlags::BLOOM,
        ServiceFlags::WITNESS,
        ServiceFlags::COMPACT_FILTERS,
        ServiceFlags::NETWORK_LIMITED,
        ServiceFlags::P2P_V2,
    ];
    for flag in available_flags {
        address_man.get_address_to_connect(flag, false);
    }
    address_man.rearrange_buckets();
});
