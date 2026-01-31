use std::str::FromStr;

use bitcoin::Network;
use floresta_common::impl_error_from;
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
}

impl_error_from!(DescriptorError, Slip132Error, XpubParseError);
impl_error_from!(DescriptorError, MiniscriptError, MiniscriptError);

pub fn parse_xpubs(
    xpubs: &[String],
    network: Network,
) -> Result<Vec<Descriptor<DescriptorPublicKey>>, DescriptorError> {
    let mut descriptors = Vec::new();
    for key in xpubs {
        // Check if the xpub network matches the expected network
        let is_mainnet = is_xpub_mainnet(key.as_str())?;
        if (is_mainnet && network != Network::Bitcoin)
            || (!is_mainnet && network == Network::Bitcoin)
        {
            return Err(DescriptorError::XpubNetworkMismatch(key.clone()));
        }

        // Parses the descriptor and get an external and change descriptors
        let main_desc = generate_descriptor_from_xpub(key.as_str(), false)?;
        let change_desc = generate_descriptor_from_xpub(key.as_str(), true)?;
        descriptors.push(Descriptor::<DescriptorPublicKey>::from_str(&main_desc)?);
        descriptors.push(Descriptor::<DescriptorPublicKey>::from_str(&change_desc)?);
    }
    Ok(descriptors)
}

/// Takes an array of descriptors as `String`, performs sanity checks on each one
/// and returns list of parsed descriptors.
pub fn parse_descriptors(
    descriptors: &[String],
) -> Result<Vec<Descriptor<DescriptorPublicKey>>, MiniscriptError> {
    let descriptors = descriptors
        .iter()
        .map(|descriptor| {
            let descriptor = Descriptor::<DescriptorPublicKey>::from_str(descriptor.as_str())?;
            descriptor.sanity_check()?;
            descriptor.into_single_descriptors()
        })
        .collect::<Result<Vec<Vec<_>>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    Ok(descriptors)
}

#[cfg(test)]
mod test {
    use bitcoin::Network;

    use super::*;

    const XPUB: &str = "xpub6CPimhNogJosVzpueNmrWEfSHc2YTXG1ZyE6TBV4Nx6UxZ7zKSGYv9hKxNjiFY5o1vz7QeZa2m6vQmyndDrkECk8cShWYWxe1gqa1xJEkgs";
    const YPUB: &str = "ypub6XmBfjfmuYD1bjv5RCEHU8jD1NPGZh6NRTGDB8ndQsd7MPnzhDhAsdrF9sK8Z4G9FvcFBHoGsZqhsDHtenca3K5QigYWVKXvkAx6HBxVGYM";
    const ZPUB: &str = "zpub6rFvSvP5VbpXwej2L5WseLfxfdUzSczs9DK9v9mpXgXNqjFhtfUTRGkQKr7sXKNyrrzhd2LCysGqts1oT3b1PJji16xWzcmNMfhmZ8kkLZ1";
    const XPUB_MAINNET: [&str; 3] = [XPUB, YPUB, ZPUB];

    const TPUB: &str = "tpubDC73PMTHeKDXnFwNFz8CLBy2VVx4D85WW2vbzwVLwCD9zkQ6Vj97muhLRTbKvmue1PyVQLwizvBW6v2SD1LnzbeuHnRsDYQZGE8urTZHMn5";
    const UPUB: &str = "upub5E3Vhaq9uVmz426B5FME1csAY8tvQ8vRqt7WnGyiJ4CoknpyM2WJk4B6uSh2kud3r8RJHTzS5jLFnWNRThKZyew6tDX2eXGMyTvfa8AVwyK";
    const VPUB: &str = "vpub5Zrsj9pYeJLwTfggbSQYZDdpEpZ4M1qB1EUKfXB9bjsookSNjM6c6eFTYfjb8KcGJV4ZqAYScBvC7hyDbbWKCHVcC6RETNJUfwUFvnHJM8Y";
    const XPUB_TESTNET: [&str; 3] = [TPUB, UPUB, VPUB];

    #[test]
    fn test_xpub_parsing() {
        // xpub | network | (main address, change address)
        let cases = &[
            (
                XPUB,
                Network::Bitcoin,
                [
                    "1JHazecJrjbxBMQgRcyV3JCQJwVbHBjH5t",
                    "1JbCXSeZHizJDQANsgtLBjo5y24JNMyGTB",
                ],
            ),
            (
                YPUB,
                Network::Bitcoin,
                [
                    "31sQy1RG4Y6sCtCpmXrtiJooqzBozRUTU6",
                    "33kzJbaR4EDzEoigsKuLata1svSqNGsdSo",
                ],
            ),
            (
                ZPUB,
                Network::Bitcoin,
                [
                    "bc1qz4ta3h4ga6hdqa090wfpr83asyz5z40t272wez",
                    "bc1qjeq39p3mpvmwqwkpaqe9hdjgfhfa8w5z87tnp4",
                ],
            ),
            (
                TPUB,
                Network::Testnet,
                [
                    "mhk8YjtyHigqGMiEGaf8cnNW9Game9exC6",
                    "mmuYagUFFQtAzw8Ts7afED6HFboCy4e8WR",
                ],
            ),
            (
                UPUB,
                Network::Testnet,
                [
                    "2NBfJvMZadWb8mwtV3F4FXTqAJs3pkYNdn8",
                    "2MznomgtTHMBvsMqPwwE3sSLzj6F8w3Mnyi",
                ],
            ),
            (
                VPUB,
                Network::Testnet,
                [
                    "tb1q7e5q2y0mpvesst3jxhe45q0e2q9gdkfd6zxzqa",
                    "tb1qzplphjt68gs0lwvxrq70t9j9cva8ky7r7ucz2g",
                ],
            ),
            (
                VPUB,
                Network::Regtest,
                [
                    "bcrt1q7e5q2y0mpvesst3jxhe45q0e2q9gdkfdctl0h5",
                    "bcrt1qzplphjt68gs0lwvxrq70t9j9cva8ky7ru4p0ap",
                ],
            ),
        ];

        for (descriptor, network, addresses) in cases {
            let parsed = parse_xpubs(&[descriptor.to_string()], *network).unwrap();
            assert_eq!(parsed.len(), 2);

            let main_desc = parsed[0].clone();
            let main_address = main_desc
                .at_derivation_index(0)
                .unwrap()
                .address(*network)
                .unwrap();
            assert_eq!(main_address.to_string(), addresses[0]);

            let change_desc = parsed[1].clone();
            let change_address = change_desc
                .at_derivation_index(0)
                .unwrap()
                .address(*network)
                .unwrap();
            assert_eq!(change_address.to_string(), addresses[1]);
        }
    }

    #[test]
    fn test_parse_xpub_with_correct_network() {
        fn check(xpubs: [&str; 3], network: Network) {
            for xpub in xpubs {
                let parsed = parse_xpubs(&[xpub.to_string()], network);
                assert!(parsed.is_ok());
            }
        }

        check(XPUB_MAINNET, Network::Bitcoin);

        check(XPUB_TESTNET, Network::Regtest);
        check(XPUB_TESTNET, Network::Testnet);
        check(XPUB_TESTNET, Network::Testnet4);
        check(XPUB_TESTNET, Network::Signet);
    }

    #[test]
    fn test_parse_xpub_with_wrong_network() {
        fn check(xpubs: [&str; 3], network: Network) {
            for xpub in xpubs {
                let parsed = parse_xpubs(&[xpub.to_string()], network);
                let err = parsed.err().unwrap();
                assert!(
                    matches!(err, DescriptorError::XpubNetworkMismatch(actual) if actual == xpub),
                    "Expected XpubNetworkMismatch error"
                );
            }
        }

        check(XPUB_MAINNET, Network::Regtest);
        check(XPUB_MAINNET, Network::Testnet);
        check(XPUB_MAINNET, Network::Testnet4);
        check(XPUB_MAINNET, Network::Signet);

        check(XPUB_TESTNET, Network::Bitcoin);
    }

    #[test]
    fn test_descriptor_parsing() {
        // singlesig
        assert_eq!(
            parse_descriptors(&[
                "wpkh([a5b13c0e/84h/0h/0h]xpub6CFy3kRXorC3NMTt8qrsY9ucUfxVLXyFQ49JSLm3iEG5gfAmWewYFzjNYFgRiCjoB9WWEuJQiyYGCdZvUTwPEUPL9pPabT8bkbiD9Po47XG/<0;1>/*)#n8sgapuv".to_owned()
            ]).unwrap(),
            parse_descriptors(&[
                "wpkh([a5b13c0e/84'/0'/0']xpub6CFy3kRXorC3NMTt8qrsY9ucUfxVLXyFQ49JSLm3iEG5gfAmWewYFzjNYFgRiCjoB9WWEuJQiyYGCdZvUTwPEUPL9pPabT8bkbiD9Po47XG/0/*)#wg8dh3s7".to_owned(),
                "wpkh([a5b13c0e/84'/0'/0']xpub6CFy3kRXorC3NMTt8qrsY9ucUfxVLXyFQ49JSLm3iEG5gfAmWewYFzjNYFgRiCjoB9WWEuJQiyYGCdZvUTwPEUPL9pPabT8bkbiD9Po47XG/1/*)#luzv2yqx".to_owned()
            ]).unwrap()
        );
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
}
