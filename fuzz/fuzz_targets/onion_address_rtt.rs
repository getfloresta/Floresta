#![no_main]

use floresta_wire::onion::OnionV3Addr;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(address) = OnionV3Addr::try_from(data) else {
        return;
    };

    let encoded = address.as_human_readable();
    let decoded = encoded.parse::<OnionV3Addr>();
    if let Ok(decoded) = decoded {
        assert_eq!(decoded, address);
    }
});
