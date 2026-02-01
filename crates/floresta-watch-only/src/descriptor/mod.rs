use core::fmt;
use core::str::FromStr;

use bitcoin::Network;
use bitcoin::ScriptBuf;
use floresta_common::impl_error_from;
use miniscript::descriptor::ConversionError;
use miniscript::Descriptor;
use miniscript::DescriptorPublicKey;
use miniscript::Error as MiniscriptError;

mod slip132;

use crate::descriptor::slip132::generate_descriptor_from_xpub;
use crate::descriptor::slip132::is_xpub_mainnet;
use crate::descriptor::slip132::Error as Slip132Error;

#[derive(Debug)]
pub enum DescriptorError {
    /// Error parsing xpub
    XpubParseError(Slip132Error),

    /// Error xpub network mismatch
    XpubNetworkMismatch(String),

    /// Error in miniscript
    MiniscriptError(MiniscriptError),

    DeriveDescriptorError(ConversionError),
}

impl_error_from!(DescriptorError, Slip132Error, XpubParseError);
impl_error_from!(DescriptorError, MiniscriptError, MiniscriptError);
impl_error_from!(DescriptorError, ConversionError, DeriveDescriptorError);

impl fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DescriptorError::XpubParseError(err) => write!(f, "Xpub parse error: {}", err),
            DescriptorError::XpubNetworkMismatch(key) => {
                write!(
                    f,
                    "The inserted Xpub does not operate in this network: {}",
                    key
                )
            }
            DescriptorError::MiniscriptError(err) => write!(f, "Miniscript error: {}", err),
            DescriptorError::DeriveDescriptorError(err) => {
                write!(f, "Derive descriptor error: {}", err)
            }
        }
    }
}

pub fn parse_xpub(xpub: &str, network: Network) -> Result<Vec<String>, DescriptorError> {
    // Check if the xpub network matches the expected network
    let is_mainnet = is_xpub_mainnet(xpub)?;
    if (is_mainnet && network != Network::Bitcoin) || (!is_mainnet && network == Network::Bitcoin) {
        return Err(DescriptorError::XpubNetworkMismatch(xpub.to_string()));
    }

    // Parses the descriptor and get an external and change descriptors
    let main_desc = generate_descriptor_from_xpub(xpub, false)?;
    let change_desc = generate_descriptor_from_xpub(xpub, true)?;

    Ok(vec![
        Descriptor::<DescriptorPublicKey>::from_str(&main_desc)?.to_string(),
        Descriptor::<DescriptorPublicKey>::from_str(&change_desc)?.to_string(),
    ])
}

/// Takes an array of descriptors as `String`, performs sanity checks on each one
/// and returns list of parsed descriptors.
pub fn parse_descriptors(
    descriptors: &[String],
) -> Result<Vec<Descriptor<DescriptorPublicKey>>, DescriptorError> {
    let descriptors = descriptors
        .iter()
        .map(|descriptor| parse_and_split_descriptor(descriptor))
        .collect::<Result<Vec<Vec<_>>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    Ok(descriptors)
}

/// Parses a descriptor string, validates it, and splits it into single descriptors.
pub fn parse_and_split_descriptor(
    descriptor: &str,
) -> Result<Vec<Descriptor<DescriptorPublicKey>>, DescriptorError> {
    let descriptor = Descriptor::<DescriptorPublicKey>::from_str(descriptor)?;
    descriptor.sanity_check()?;

    let descriptors = descriptor.into_single_descriptors()?;

    Ok(descriptors)
}

/// Derives addresses from a list of descriptors.
/// Parses each descriptor, validates it, and derives the specified number of addresses
/// starting from the given index.
pub fn derive_addresses_from_list_descriptors(
    descriptors: &[String],
    index: u32,
    quantity: u32,
) -> Result<Vec<ScriptBuf>, DescriptorError> {
    let mut addresses = Vec::new();
    for desc in descriptors {
        addresses.extend_from_slice(&derive_addresses_from_descriptor(desc, index, quantity)?);
    }

    Ok(addresses)
}

/// Derives addresses from a single descriptor string.
/// Splits the descriptor into single descriptors and derives addresses for each one.
pub fn derive_addresses_from_descriptor(
    descriptor: &str,
    index: u32,
    quantity: u32,
) -> Result<Vec<ScriptBuf>, DescriptorError> {
    let descriptors = parse_and_split_descriptor(descriptor)?;

    let mut addresses = Vec::with_capacity(descriptors.len() * quantity as usize);
    for desc in descriptors {
        addresses.extend_from_slice(&derive_addresses_from_parsed_descriptor(
            desc, index, quantity,
        )?);
    }

    Ok(addresses)
}

/// Derives addresses from a parsed descriptor.
/// Generates the specified number of addresses starting from the given index.
pub fn derive_addresses_from_parsed_descriptor(
    descriptor: Descriptor<DescriptorPublicKey>,
    index: u32,
    quantity: u32,
) -> Result<Vec<ScriptBuf>, DescriptorError> {
    let mut addresses = Vec::with_capacity(quantity as usize);
    for i in index..index + quantity {
        let address = descriptor.at_derivation_index(i)?.script_pubkey();
        addresses.push(address);
    }

    Ok(addresses)
}

#[cfg(test)]
mod test {
    use std::vec;

    use bitcoin::Network;

    use super::*;

    struct TestCase {
        xpub: &'static str,
        default_descriptor: &'static str,
        main_descriptor: &'static str,
        change_descriptor: &'static str,
        main_address: &'static str,
        change_address: &'static str,
        main_script: &'static str,
        change_script: &'static str,
        network: Network,
    }

    const TEST_CASE_XPUB: TestCase = TestCase {
        xpub: "xpub6CPimhNogJosVzpueNmrWEfSHc2YTXG1ZyE6TBV4Nx6UxZ7zKSGYv9hKxNjiFY5o1vz7QeZa2m6vQmyndDrkECk8cShWYWxe1gqa1xJEkgs",
        default_descriptor: "pkh(xpub6CPimhNogJosVzpueNmrWEfSHc2YTXG1ZyE6TBV4Nx6UxZ7zKSGYv9hKxNjiFY5o1vz7QeZa2m6vQmyndDrkECk8cShWYWxe1gqa1xJEkgs/<0;1>/*)",
        main_descriptor: "pkh(xpub6CPimhNogJosVzpueNmrWEfSHc2YTXG1ZyE6TBV4Nx6UxZ7zKSGYv9hKxNjiFY5o1vz7QeZa2m6vQmyndDrkECk8cShWYWxe1gqa1xJEkgs/0/*)#32jmvyn7",
        change_descriptor: "pkh(xpub6CPimhNogJosVzpueNmrWEfSHc2YTXG1ZyE6TBV4Nx6UxZ7zKSGYv9hKxNjiFY5o1vz7QeZa2m6vQmyndDrkECk8cShWYWxe1gqa1xJEkgs/1/*)#q7h633rx",
        main_address: "1JHazecJrjbxBMQgRcyV3JCQJwVbHBjH5t",
        change_address: "1JbCXSeZHizJDQANsgtLBjo5y24JNMyGTB",
        main_script: "OP_DUP OP_HASH160 OP_PUSHBYTES_20 bd9d2ba0e12d433a4b3c81fbf6457f41a4b37ffe OP_EQUALVERIFY OP_CHECKSIG",
        change_script: "OP_DUP OP_HASH160 OP_PUSHBYTES_20 c0f1e6c8977d40f9a8ffe9b06120ae4c2833e9ef OP_EQUALVERIFY OP_CHECKSIG",
        network: Network::Bitcoin,
    };

    const TEST_CASE_YPUB: TestCase = TestCase {
        xpub: "ypub6XmBfjfmuYD1bjv5RCEHU8jD1NPGZh6NRTGDB8ndQsd7MPnzhDhAsdrF9sK8Z4G9FvcFBHoGsZqhsDHtenca3K5QigYWVKXvkAx6HBxVGYM",
        default_descriptor: "sh(wpkh(xpub6CvvN4zrkrfXkSixaqSfG3dhqQEpd56sWLjzPjtk2sFEJHymSZXcFaC78fMYZ9cDrHVSRpCiQuV9yvgKw6CZF5PorLr5uQiSUStStZjpSSV/<0;1>/*))",
        main_descriptor: "sh(wpkh(xpub6CvvN4zrkrfXkSixaqSfG3dhqQEpd56sWLjzPjtk2sFEJHymSZXcFaC78fMYZ9cDrHVSRpCiQuV9yvgKw6CZF5PorLr5uQiSUStStZjpSSV/0/*))#657qlqhe",
        change_descriptor: "sh(wpkh(xpub6CvvN4zrkrfXkSixaqSfG3dhqQEpd56sWLjzPjtk2sFEJHymSZXcFaC78fMYZ9cDrHVSRpCiQuV9yvgKw6CZF5PorLr5uQiSUStStZjpSSV/1/*))#uhk9ydud",
        main_address: "31sQy1RG4Y6sCtCpmXrtiJooqzBozRUTU6",
        change_address: "33kzJbaR4EDzEoigsKuLata1svSqNGsdSo",
        main_script: "OP_HASH160 OP_PUSHBYTES_20 01f764ff1e1f27740b0b638b0251bec1bece0964 OP_EQUAL",
        change_script: "OP_HASH160 OP_PUSHBYTES_20 16b0903438a739fc09bb7e894895df291bb8ee19 OP_EQUAL",
        network: Network::Bitcoin,
    };

    const TEST_CASE_ZPUB: TestCase = TestCase {
        xpub: "zpub6rFvSvP5VbpXwej2L5WseLfxfdUzSczs9DK9v9mpXgXNqjFhtfUTRGkQKr7sXKNyrrzhd2LCysGqts1oT3b1PJji16xWzcmNMfhmZ8kkLZ1",
        default_descriptor: "wpkh(xpub6CbPqb3FCEjaF4LnfMwdEAUxKhC6ZP1sJzGiMMz3mfmcjXdFPM9LB9S8HSChXW593am685964YZk8Hng1ekynqNWGRZfpo8PpDaUmyvQqvY/<0;1>/*)",
        main_descriptor: "wpkh(xpub6CbPqb3FCEjaF4LnfMwdEAUxKhC6ZP1sJzGiMMz3mfmcjXdFPM9LB9S8HSChXW593am685964YZk8Hng1ekynqNWGRZfpo8PpDaUmyvQqvY/0/*)#z2djk607",
        change_descriptor: "wpkh(xpub6CbPqb3FCEjaF4LnfMwdEAUxKhC6ZP1sJzGiMMz3mfmcjXdFPM9LB9S8HSChXW593am685964YZk8Hng1ekynqNWGRZfpo8PpDaUmyvQqvY/1/*)#n7gnt0lx",
        main_address: "bc1qz4ta3h4ga6hdqa090wfpr83asyz5z40t272wez",
        change_address: "bc1qjeq39p3mpvmwqwkpaqe9hdjgfhfa8w5z87tnp4",
        main_script: "OP_0 OP_PUSHBYTES_20 1557d8dea8eeaed075e57b92119e3d81054155eb",
        change_script: "OP_0 OP_PUSHBYTES_20 964112863b0b36e03ac1e8325bb6484dd3d3ba82",
        network: Network::Bitcoin,
    };

    const TEST_CASE_TPUB: TestCase = TestCase {
        xpub: "tpubDC73PMTHeKDXnFwNFz8CLBy2VVx4D85WW2vbzwVLwCD9zkQ6Vj97muhLRTbKvmue1PyVQLwizvBW6v2SD1LnzbeuHnRsDYQZGE8urTZHMn5",
        default_descriptor: "pkh(tpubDC73PMTHeKDXnFwNFz8CLBy2VVx4D85WW2vbzwVLwCD9zkQ6Vj97muhLRTbKvmue1PyVQLwizvBW6v2SD1LnzbeuHnRsDYQZGE8urTZHMn5/<0;1>/*)",
        main_descriptor: "pkh(tpubDC73PMTHeKDXnFwNFz8CLBy2VVx4D85WW2vbzwVLwCD9zkQ6Vj97muhLRTbKvmue1PyVQLwizvBW6v2SD1LnzbeuHnRsDYQZGE8urTZHMn5/0/*)#8zp7ryrl",
        change_descriptor: "pkh(tpubDC73PMTHeKDXnFwNFz8CLBy2VVx4D85WW2vbzwVLwCD9zkQ6Vj97muhLRTbKvmue1PyVQLwizvBW6v2SD1LnzbeuHnRsDYQZGE8urTZHMn5/1/*)#kkyl73n8",
        main_address: "mhk8YjtyHigqGMiEGaf8cnNW9Game9exC6",
        change_address: "mmuYagUFFQtAzw8Ts7afED6HFboCy4e8WR",
        main_script: "OP_DUP OP_HASH160 OP_PUSHBYTES_20 186e37d051208d814da8988b596e515ac79c0336 OP_EQUALVERIFY OP_CHECKSIG",
        change_script: "OP_DUP OP_HASH160 OP_PUSHBYTES_20 461686e57db4157808a9d4e935ae35d60fae0676 OP_EQUALVERIFY OP_CHECKSIG",
        network: Network::Testnet,
    };

    const TEST_CASE_UPUB: TestCase = TestCase {
        xpub: "upub5E3Vhaq9uVmz426B5FME1csAY8tvQ8vRqt7WnGyiJ4CoknpyM2WJk4B6uSh2kud3r8RJHTzS5jLFnWNRThKZyew6tDX2eXGMyTvfa8AVwyK",
        default_descriptor: "sh(wpkh(tpubDCuv8pfb4pMsshrP2WhBqoV3PARvDPPz8rGUV1iWmz6LfNwNBDr5kgpMD6eaH8Y3rxJd9UHyzpDx8Yhj1eQrFoSCYqMc5nP4Nbi1VvJmNco/<0;1>/*))",
        main_descriptor: "sh(wpkh(tpubDCuv8pfb4pMsshrP2WhBqoV3PARvDPPz8rGUV1iWmz6LfNwNBDr5kgpMD6eaH8Y3rxJd9UHyzpDx8Yhj1eQrFoSCYqMc5nP4Nbi1VvJmNco/0/*))#sh4fvsj4",
        change_descriptor: "sh(wpkh(tpubDCuv8pfb4pMsshrP2WhBqoV3PARvDPPz8rGUV1iWmz6LfNwNBDr5kgpMD6eaH8Y3rxJd9UHyzpDx8Yhj1eQrFoSCYqMc5nP4Nbi1VvJmNco/1/*))#k5avhaep",
        main_address: "2NBfJvMZadWb8mwtV3F4FXTqAJs3pkYNdn8",
        change_address: "2MznomgtTHMBvsMqPwwE3sSLzj6F8w3Mnyi",
        main_script: "OP_HASH160 OP_PUSHBYTES_20 ca005d4bd7b470a9e12710789cac3812c16146a4 OP_EQUAL",
        change_script: "OP_HASH160 OP_PUSHBYTES_20 52c1f9cdaa84c6552678051e6322a8f5ff6687ae OP_EQUAL",
        network: Network::Testnet,
    };

    const TEST_CASE_VPUB: TestCase = TestCase {
        xpub: "vpub5Zrsj9pYeJLwTfggbSQYZDdpEpZ4M1qB1EUKfXB9bjsookSNjM6c6eFTYfjb8KcGJV4ZqAYScBvC7hyDbbWKCHVcC6RETNJUfwUFvnHJM8Y",
        default_descriptor: "wpkh(tpubDDu2riz4ewPMS4FmiLxtBKABuswcDeKEP674as24hfPTfEjYJtGpVDEZq7jYedsLufq5whFS4cTLaTgxRrBagCK6zNZPJibgoMBxTvUcVFf/<0;1>/*)",
        main_descriptor: "wpkh(tpubDDu2riz4ewPMS4FmiLxtBKABuswcDeKEP674as24hfPTfEjYJtGpVDEZq7jYedsLufq5whFS4cTLaTgxRrBagCK6zNZPJibgoMBxTvUcVFf/0/*)#f8w55tty",
        change_descriptor: "wpkh(tpubDDu2riz4ewPMS4FmiLxtBKABuswcDeKEP674as24hfPTfEjYJtGpVDEZq7jYedsLufq5whFS4cTLaTgxRrBagCK6zNZPJibgoMBxTvUcVFf/1/*)#cnt4f7mu",
        main_address: "tb1q7e5q2y0mpvesst3jxhe45q0e2q9gdkfd6zxzqa",
        change_address: "tb1qzplphjt68gs0lwvxrq70t9j9cva8ky7r7ucz2g",
        main_script: "OP_0 OP_PUSHBYTES_20 f6680511fb0b33082e3235f35a01f9500a86d92d",
        change_script: "OP_0 OP_PUSHBYTES_20 107e1bc97a3a20ffb986183cf59645c33a7b13c3",
        network: Network::Testnet,
    };

    const TEST_CASE_VPUB_REGTEST: TestCase = TestCase {
        xpub: "vpub5Zrsj9pYeJLwTfggbSQYZDdpEpZ4M1qB1EUKfXB9bjsookSNjM6c6eFTYfjb8KcGJV4ZqAYScBvC7hyDbbWKCHVcC6RETNJUfwUFvnHJM8Y",
        default_descriptor: "wpkh(tpubDDu2riz4ewPMS4FmiLxtBKABuswcDeKEP674as24hfPTfEjYJtGpVDEZq7jYedsLufq5whFS4cTLaTgxRrBagCK6zNZPJibgoMBxTvUcVFf/<0;1>/*)",
        main_descriptor: "wpkh(tpubDDu2riz4ewPMS4FmiLxtBKABuswcDeKEP674as24hfPTfEjYJtGpVDEZq7jYedsLufq5whFS4cTLaTgxRrBagCK6zNZPJibgoMBxTvUcVFf/0/*)#f8w55tty",
        change_descriptor: "wpkh(tpubDDu2riz4ewPMS4FmiLxtBKABuswcDeKEP674as24hfPTfEjYJtGpVDEZq7jYedsLufq5whFS4cTLaTgxRrBagCK6zNZPJibgoMBxTvUcVFf/1/*)#cnt4f7mu",
        main_address: "bcrt1q7e5q2y0mpvesst3jxhe45q0e2q9gdkfdctl0h5",
        change_address: "bcrt1qzplphjt68gs0lwvxrq70t9j9cva8ky7ru4p0ap",
        main_script: "OP_0 OP_PUSHBYTES_20 f6680511fb0b33082e3235f35a01f9500a86d92d",
        change_script: "OP_0 OP_PUSHBYTES_20 107e1bc97a3a20ffb986183cf59645c33a7b13c3",
        network: Network::Regtest,
    };

    const TEST_CASES: [&TestCase; 7] = [
        &TEST_CASE_XPUB,
        &TEST_CASE_YPUB,
        &TEST_CASE_ZPUB,
        &TEST_CASE_TPUB,
        &TEST_CASE_UPUB,
        &TEST_CASE_VPUB,
        &TEST_CASE_VPUB_REGTEST,
    ];

    #[test]
    fn test_parse_xpub_valid_cases() {
        let cases = TEST_CASES;

        for &tc in &cases {
            let descriptors_string = parse_xpub(tc.xpub, tc.network).unwrap();
            assert_eq!(descriptors_string.len(), 2);
            assert_eq!(descriptors_string[0], tc.main_descriptor);
            assert_eq!(descriptors_string[1], tc.change_descriptor);

            let descriptors = parse_descriptors(&descriptors_string).unwrap();
            assert_eq!(descriptors.len(), 2);

            let main_desc = descriptors[0].clone();
            let main_address = main_desc
                .at_derivation_index(0)
                .unwrap()
                .address(tc.network)
                .unwrap();
            assert_eq!(main_address.to_string(), tc.main_address);

            let change_desc = descriptors[1].clone();
            let change_address = change_desc
                .at_derivation_index(0)
                .unwrap()
                .address(tc.network)
                .unwrap();
            assert_eq!(change_address.to_string(), tc.change_address);
        }
    }

    #[test]
    fn test_parse_xpub_with_correct_network() {
        fn check(xpub: &str, network: Network) {
            let parsed = parse_xpub(xpub, network);
            assert!(parsed.is_ok());
        }

        let cases = TEST_CASES;

        for &tc in &cases {
            check(tc.xpub, tc.network);
        }
    }

    #[test]
    fn test_parse_xpub_with_wrong_network() {
        fn check(xpub: &str, network: Network) {
            let wrong_network = if network == Network::Bitcoin {
                vec![Network::Testnet, Network::Regtest, Network::Signet]
            } else {
                vec![Network::Bitcoin]
            };

            for net in wrong_network {
                let parsed = parse_xpub(xpub, net);
                let err = parsed.err().unwrap();
                assert!(
                    matches!(err, DescriptorError::XpubNetworkMismatch(actual) if actual == xpub),
                    "Expected XpubNetworkMismatch error"
                );
            }
        }

        let cases = TEST_CASES;

        for &tc in &cases {
            check(tc.xpub, tc.network);
        }
    }

    #[test]
    fn test_parse_descriptors_valid_cases() {
        // singlesig
        for cases in TEST_CASES {
            assert_eq!(
                parse_descriptors(&[cases.default_descriptor.to_owned()]).unwrap(),
                parse_descriptors(&[
                    cases.main_descriptor.to_owned(),
                    cases.change_descriptor.to_owned()
                ])
                .unwrap()
            );
        }

        // multisig
        assert_eq!(
            parse_descriptors(&[
                "wsh(sortedmulti(1,[6f826a6a/48h/0h/0h/2h]xpub6DsY48BAsvEMTRPbeSTu9jZXqEsTKr5T86WbRbXHp2gEVCNR3hALnMorFawVwnnHMMfjbyY8We9B4beh1fxqhcv6kgSeLgQxeXDqv3DaW7m/<0;1>/*,[a5b13c0e/48h/0h/0h/2h]xpub6Eqj1Hj3RezebC6cKiYYN2sAc1Wu33BWoaafnNgAbQwDkJdy7aXCYCmaMzb8rCpmh919UsehyV5Ywjo62hG4R2G2PGv4uqEDTUhYQw26BDJ/<0;1>/*))#nykmcu2v".to_owned()
            ]).unwrap(),
            parse_descriptors(&[
                "wsh(sortedmulti(1,[6f826a6a/48'/0'/0'/2']xpub6DsY48BAsvEMTRPbeSTu9jZXqEsTKr5T86WbRbXHp2gEVCNR3hALnMorFawVwnnHMMfjbyY8We9B4beh1fxqhcv6kgSeLgQxeXDqv3DaW7m/0/*,[a5b13c0e/48'/0'/0'/2']xpub6Eqj1Hj3RezebC6cKiYYN2sAc1Wu33BWoaafnNgAbQwDkJdy7aXCYCmaMzb8rCpmh919UsehyV5Ywjo62hG4R2G2PGv4uqEDTUhYQw26BDJ/0/*))#sw68w95x".to_owned(),
                "wsh(sortedmulti(1,[6f826a6a/48'/0'/0'/2']xpub6DsY48BAsvEMTRPbeSTu9jZXqEsTKr5T86WbRbXHp2gEVCNR3hALnMorFawVwnnHMMfjbyY8We9B4beh1fxqhcv6kgSeLgQxeXDqv3DaW7m/1/*,[a5b13c0e/48'/0'/0'/2']xpub6Eqj1Hj3RezebC6cKiYYN2sAc1Wu33BWoaafnNgAbQwDkJdy7aXCYCmaMzb8rCpmh919UsehyV5Ywjo62hG4R2G2PGv4uqEDTUhYQw26BDJ/1/*))#fafrqkpn".to_owned()
            ]).unwrap()
        );
    }

    #[test]
    fn test_parse_and_split_descriptor_valid_cases() {
        for cases in TEST_CASES {
            let descriptors = parse_and_split_descriptor(cases.default_descriptor).unwrap();
            let expected_descriptor = [cases.main_descriptor, cases.change_descriptor]
                .iter()
                .map(|d| Descriptor::<DescriptorPublicKey>::from_str(d).unwrap())
                .collect::<Vec<_>>();

            assert_eq!(descriptors, expected_descriptor);
        }
    }

    #[test]
    fn test_derive_addresses_from_list_descriptors_valid_cases() {
        let cases = TEST_CASES;
        let all_default_descriptors: Vec<String> = cases
            .iter()
            .map(|tc| tc.default_descriptor.to_string())
            .collect();

        let all_script_buff: Vec<String> = cases
            .iter()
            .flat_map(|tc| vec![tc.main_script.to_string(), tc.change_script.to_string()])
            .collect();

        let addresses_derived =
            derive_addresses_from_list_descriptors(&all_default_descriptors, 0, 1).unwrap();

        assert_eq!(addresses_derived.len(), all_script_buff.len());
        assert_eq!(
            addresses_derived
                .iter()
                .map(|script| script.to_string())
                .collect::<Vec<_>>(),
            all_script_buff
        );
    }

    #[test]
    fn test_derive_addresses_from_descriptor_valid_cases() {
        let cases = TEST_CASES;
        for &tc in &cases {
            let list_script_buff =
                derive_addresses_from_descriptor(tc.default_descriptor, 0, 1).unwrap();
            assert_eq!(list_script_buff.len(), 2);
            assert_eq!(list_script_buff[0].to_string(), tc.main_script);
            assert_eq!(list_script_buff[1].to_string(), tc.change_script);

            let main_script_buff =
                derive_addresses_from_descriptor(tc.main_descriptor, 0, 1).unwrap();
            assert_eq!(main_script_buff.len(), 1);
            assert_eq!(main_script_buff[0].to_string(), tc.main_script);

            let change_script_buff =
                derive_addresses_from_descriptor(tc.change_descriptor, 0, 1).unwrap();
            assert_eq!(change_script_buff.len(), 1);
            assert_eq!(change_script_buff[0].to_string(), tc.change_script);
        }
    }

    #[test]
    fn test_derive_addresses_from_parsed_descriptor_valid_cases() {
        for &tc in &TEST_CASES {
            let descriptor = parse_and_split_descriptor(tc.main_descriptor).unwrap()[0].clone();
            let derived_addresses =
                derive_addresses_from_parsed_descriptor(descriptor, 0, 1).unwrap();

            assert_eq!(derived_addresses.len(), 1);
            assert_eq!(derived_addresses[0].to_string(), tc.main_script);
        }
    }

    #[test]
    fn test_invalid_descriptor_parsing() {
        fn check(result: Result<Vec<Descriptor<DescriptorPublicKey>>, DescriptorError>) {
            assert!(result.is_err());
            assert!(
                matches!(result, Err(DescriptorError::MiniscriptError(_))),
                "Expected MiniscriptError"
            );
        }

        let invalid_descriptor = "invalid(descriptor)";

        let result = parse_descriptors(&[invalid_descriptor.to_string()]);
        check(result);

        let result = parse_and_split_descriptor(invalid_descriptor);
        check(result);
    }

    #[test]
    fn test_derive_addresses_with_invalid_descriptor() {
        fn check(result: Result<Vec<ScriptBuf>, DescriptorError>) {
            assert!(
                matches!(result, Err(DescriptorError::MiniscriptError(_))),
                "Expected MiniscriptError"
            );
        }
        let invalid_descriptor = "invalid(descriptor)";

        let result = derive_addresses_from_descriptor(invalid_descriptor, 0, 1);
        check(result);

        let result =
            derive_addresses_from_list_descriptors(&[invalid_descriptor.to_string()], 0, 1);
        check(result);
    }
}
