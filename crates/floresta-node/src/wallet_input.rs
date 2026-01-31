//! Handles different inputs, try to make sense out of it and store a sane descriptor at the end

use std::str::FromStr;

use bitcoin::Address;
use bitcoin::Network;
use miniscript::Descriptor;
use miniscript::DescriptorPublicKey;
use tracing::error;

use crate::error::FlorestadError;
use crate::slip132::generate_descriptor_from_xpub;
use crate::slip132::is_xpub_mainnet;

fn parse_xpubs(
    xpubs: &[String],
    network: Network,
) -> Result<Vec<Descriptor<DescriptorPublicKey>>, FlorestadError> {
    let mut descriptors = Vec::new();
    for key in xpubs {
        // Check if the xpub network matches the expected network
        let is_mainnet = is_xpub_mainnet(key.as_str())?;
        if (is_mainnet && network != Network::Bitcoin)
            || (!is_mainnet && network == Network::Bitcoin)
        {
            return Err(FlorestadError::XpubNetworkMismatch(key.clone()));
        }

        // Parses the descriptor and get an external and change descriptors
        let main_desc = generate_descriptor_from_xpub(key.as_str(), false)?;
        let change_desc = generate_descriptor_from_xpub(key.as_str(), true)?;
        descriptors.push(Descriptor::<DescriptorPublicKey>::from_str(&main_desc)?);
        descriptors.push(Descriptor::<DescriptorPublicKey>::from_str(&change_desc)?);
    }
    Ok(descriptors)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InitialWalletSetup {
    pub(crate) descriptors: Vec<Descriptor<DescriptorPublicKey>>,
    pub(crate) addresses: Vec<Address>,
}

impl InitialWalletSetup {
    pub(crate) fn build(
        xpubs: &[String],
        initial_descriptors: &[String],
        addresses: &[String],
        network: Network,
        addresses_per_descriptor: u32,
    ) -> Result<Self, FlorestadError> {
        let mut descriptors = parse_xpubs(xpubs, network)?;
        descriptors.extend(parse_descriptors(initial_descriptors)?);
        descriptors.sort();
        descriptors.dedup();
        let mut addresses = addresses
            .iter()
            .flat_map(|address| match Address::from_str(address) {
                Ok(address) => Ok(address.require_network(network)),
                Err(e) => {
                    error!("Invalid address provided: {address} \nReason: {e:?}");
                    Err(e)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        addresses.extend(descriptors.iter().flat_map(|descriptor| {
            (0..addresses_per_descriptor).map(|index| {
                descriptor
                    .at_derivation_index(index)
                    .expect("Error while deriving address")
                    .address(network)
                    .expect("Error while deriving address. Is this an active descriptor?")
            })
        }));
        addresses.sort();
        addresses.dedup();
        Ok(Self {
            descriptors,
            addresses,
        })
    }
}

pub fn parse_descriptors(
    descriptors: &[String],
) -> Result<Vec<Descriptor<DescriptorPublicKey>>, FlorestadError> {
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
pub mod test {
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
                if let FlorestadError::XpubNetworkMismatch(actual) = err {
                    assert_eq!(actual, xpub.to_string());
                } else {
                    panic!("Expected XpubNetworkMismatch error");
                }
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

    #[test]
    fn test_initial_wallet_build() {
        use pretty_assertions::assert_eq;
        let addresses_per_descriptor = 1;
        let network = Network::Bitcoin;
        // Build wallet from xpub (in this case a zpub from slip132 standard)
        let w1_xpub = InitialWalletSetup::build(&[
            "zpub6qvVf5mN7DH14wr7oZS7xL6cpcFPDmxFEHBk18YpUF1qnroE1yGfW83eafbbi23dzRk7jrVXeJFMyCo3urmQpwkXtVnRmGmaJ3qVvdwx4mB".to_owned()
        ], &[], &[], network, addresses_per_descriptor).unwrap();
        // Build same wallet from output descriptor
        let w1_descriptor = InitialWalletSetup::build(&[], &[
            "wpkh(xpub6CFy3kRXorC3NMTt8qrsY9ucUfxVLXyFQ49JSLm3iEG5gfAmWewYFzjNYFgRiCjoB9WWEuJQiyYGCdZvUTwPEUPL9pPabT8bkbiD9Po47XG/<0;1>/*)".to_owned()
        ], &[], network, addresses_per_descriptor).unwrap();
        // Using both methods the result should be the same
        assert_eq!(w1_xpub, w1_descriptor);
        // Both normal receiving descriptor and change descriptor should be present
        assert_eq!(
            w1_descriptor.descriptors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec![
                "wpkh(xpub6CFy3kRXorC3NMTt8qrsY9ucUfxVLXyFQ49JSLm3iEG5gfAmWewYFzjNYFgRiCjoB9WWEuJQiyYGCdZvUTwPEUPL9pPabT8bkbiD9Po47XG/0/*)#qua4l7ct",
                "wpkh(xpub6CFy3kRXorC3NMTt8qrsY9ucUfxVLXyFQ49JSLm3iEG5gfAmWewYFzjNYFgRiCjoB9WWEuJQiyYGCdZvUTwPEUPL9pPabT8bkbiD9Po47XG/1/*)#3gc5ztgn"
            ]
        );
        // Receiving and change addresses
        let addresses = vec![
            "bc1q88guum89mxwszau37m3y4p24renwlwgtkscl6x".to_owned(),
            "bc1q24629yendf7q0dxnw362dqccn52vuz9s0z59hr".to_owned(),
        ];
        assert_eq!(
            w1_descriptor
                .addresses
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            addresses
        );
        // We can build from these addresses
        let w1_addresses =
            InitialWalletSetup::build(&[], &[], &addresses, network, addresses_per_descriptor)
                .unwrap();
        // And the result will be the same as from xpub/descriptor
        assert_eq!(w1_descriptor.addresses, w1_addresses.addresses);
        // We can also build from xpub, descriptor and addresses, at same time
        let w1_all =
            InitialWalletSetup::build(&[
                "zpub6qvVf5mN7DH14wr7oZS7xL6cpcFPDmxFEHBk18YpUF1qnroE1yGfW83eafbbi23dzRk7jrVXeJFMyCo3urmQpwkXtVnRmGmaJ3qVvdwx4mB".to_owned()
            ], &[
                "wpkh(xpub6CFy3kRXorC3NMTt8qrsY9ucUfxVLXyFQ49JSLm3iEG5gfAmWewYFzjNYFgRiCjoB9WWEuJQiyYGCdZvUTwPEUPL9pPabT8bkbiD9Po47XG/<0;1>/*)".to_owned()
            ], &addresses, network, addresses_per_descriptor).unwrap();
        // And the result should be the same, no duplication will happen
        assert_eq!(w1_descriptor, w1_all);
    }

    #[test]
    fn test_initial_wallet_build_multisig_testnet() {
        use pretty_assertions::assert_eq;
        let addresses_per_descriptor = 1;
        let network = Network::Testnet;
        let w1_descriptor = InitialWalletSetup::build(&[], &[
            "wsh(sortedmulti(1,[54ff5a12/48h/1h/0h/2h]tpubDDw6pwZA3hYxcSN32q7a5ynsKmWr4BbkBNHydHPKkM4BZwUfiK7tQ26h7USm8kA1E2FvCy7f7Er7QXKF8RNptATywydARtzgrxuPDwyYv4x/<0;1>/*,[bcf969c0/48h/1h/0h/2h]tpubDEFdgZdCPgQBTNtGj4h6AehK79Jm4LH54JrYBJjAtHMLEAth7LuY87awx9ZMiCURFzFWhxToRJK6xp39aqeJWrG5nuW3eBnXeMJcvDeDxfp/<0;1>/*))#fuw35j0q".to_owned()
        ], &[], network, addresses_per_descriptor).unwrap();
        let addresses = vec![
            "tb1q2eeqw57e7pmrh5w3wkrshctx2qk80vf4mu7l7ek3ne4hg3lmcrnqcwejgj".to_owned(),
            "tb1q6dpyc3jyqelgfwksedef0k2244rcg4gf6wvqm463lk907es2m08qnrfky7".to_owned(),
        ];
        assert_eq!(
            w1_descriptor
                .addresses
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            addresses
        );
    }
}
