#![no_main]

use core::str;
use core::str::FromStr;

use floresta_wire::address_man::LocalAddress;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = match str::from_utf8(data) {
        Ok(s) => LocalAddress::from_str(s),
        Err(_) => return,
    };
});
