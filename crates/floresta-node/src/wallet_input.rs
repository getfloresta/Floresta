//! Handles different inputs, try to make sense out of it and store a sane descriptor at the end

use std::str::FromStr;

use bitcoin::Address;
use bitcoin::Network;
use floresta_watch_only::descriptor::parse_descriptors;
use floresta_watch_only::descriptor::parse_xpubs;
use miniscript::Descriptor;
use miniscript::DescriptorPublicKey;
use tracing::error;

use crate::error::FlorestadError;

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

#[cfg(test)]
pub mod test {
    use super::*;

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
