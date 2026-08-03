// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use core::str;
use core::str::FromStr;

use floresta_node::json_rpc::request::TipProof;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = match str::from_utf8(data) {
        Ok(s) => TipProof::from_str(s),
        Err(_) => return,
    };
});
