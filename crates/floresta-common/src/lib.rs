// SPDX-License-Identifier: MIT

//! # Floresta Common
//! Provides utility functions, macros and modules to be
//! used in other Floresta crates.

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]
#![no_std]

use bitcoin::consensus::encode;
use bitcoin::consensus::Decodable;
use bitcoin::hashes::sha256;
use bitcoin::hashes::Hash;
use bitcoin::ScriptBuf;
use bitcoin::VarInt;
#[cfg(any(feature = "descriptors-std", feature = "descriptors-no-std"))]
use miniscript::Descriptor;
#[cfg(any(feature = "descriptors-std", feature = "descriptors-no-std"))]
use miniscript::DescriptorPublicKey;
use sha2::Digest;

#[cfg(not(feature = "std"))]
mod error;
pub mod macros;
pub mod spsc;

#[cfg(any(feature = "descriptors-std", feature = "descriptors-no-std"))]
use prelude::*;
pub use spsc::Channel;

/// Computes the SHA-256 digest of the byte slice data and returns a [Hash] from `bitcoin_hashes`.
///
/// [Hash]: https://docs.rs/bitcoin_hashes/latest/bitcoin_hashes/sha256/struct.Hash.html
pub fn get_hash_from_u8(data: &[u8]) -> sha256::Hash {
    let hash = sha2::Sha256::new().chain_update(data).finalize();
    sha256::Hash::from_byte_array(hash.into())
}

/// Computes the SHA-256 digest of a script, reverses its bytes, and returns a [Hash] from
/// `bitcoin_hashes`.
///
/// The source to the specification can be found in the Electrum protocol [documentation], and it is
/// used to identify scripts in the Electrum Protocol.
///
/// [documentation]: https://electrum-protocol.readthedocs.io/en/latest/protocol-basics.html#script-hashes
/// [Hash]: https://docs.rs/bitcoin_hashes/latest/bitcoin_hashes/sha256/struct.Hash.html
pub fn get_spk_hash(spk: &ScriptBuf) -> sha256::Hash {
    let data = spk.as_bytes();
    let mut hash = sha2::Sha256::new().chain_update(data).finalize();
    hash.reverse();
    sha256::Hash::from_byte_array(hash.into())
}

/// Reads a VarInt from the given reader and ensures it is less than or equal to `max`.
///
/// Returns an error if the VarInt is larger than `max`.
pub fn read_bounded_len<R: bitcoin::io::Read + ?Sized>(
    reader: &mut R,
    max: usize,
) -> Result<usize, encode::Error> {
    let n64 = VarInt::consensus_decode(reader)?.0;
    if n64 > max as u64 {
        return Err(encode::Error::OversizedVectorAllocation {
            requested: n64 as usize,
            max,
        });
    }
    Ok(n64 as usize)
}

/// Non-standard service flags that aren't in rust-bitcoin yet.
pub mod service_flags {
    /// This peer supports UTREEXO messages
    pub const UTREEXO: u64 = 1 << 24;

    /// This peer supports UTREEXO filter messages
    pub const UTREEXO_FILTER: u64 = 1 << 25;
}

#[derive(Debug, Clone)]
/// A simple fraction struct that allows adding numbers to the numerator and denominator
///
/// If we want compute a rolling-average, we would naively hold all elements in a list and
/// compute the average from it. This is not efficient, as it requires O(n) memory and O(n)
/// time to compute the average. Instead, we can use a fraction to compute the average in O(1)
/// time and O(1) memory, by keeping track of the sum of all elements and the number of elements.
pub struct FractionAvg {
    numerator: u64,
    denominator: u64,
}

impl FractionAvg {
    /// Creates a new fraction with the given numerator and denominator
    pub fn new(numerator: u64, denominator: u64) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// Adds a number to the numerator and increments the denominator
    pub fn add(&mut self, other: u64) {
        self.numerator += other;
        self.denominator += 1;
    }

    /// Returns the average of the fraction
    pub fn value(&self) -> f64 {
        if self.denominator == 0 {
            return 0.0;
        }

        self.numerator as f64 / self.denominator as f64
    }
}

#[cfg(any(feature = "descriptors-std", feature = "descriptors-no-std"))]
/// Takes an array of descriptors as `String`, performs sanity checks on each one
/// and returns list of parsed descriptors.
pub fn parse_descriptors(
    descriptors: &[String],
) -> Result<Vec<Descriptor<DescriptorPublicKey>>, miniscript::Error> {
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

#[cfg(not(feature = "std"))]
pub mod prelude {
    extern crate alloc;
    pub use alloc::borrow::ToOwned;
    pub use alloc::boxed::Box;
    pub use alloc::format;
    pub use alloc::string::String;
    pub use alloc::string::ToString;
    pub use alloc::vec;
    pub use alloc::vec::Vec;
    pub use core::cmp;
    pub use core::convert;
    pub use core::fmt;
    pub use core::fmt::Display;
    pub use core::iter;
    pub use core::mem;
    pub use core::ops;
    pub use core::ops::Deref;
    pub use core::ops::DerefMut;
    pub use core::option;
    pub use core::result;
    pub use core::slice;
    pub use core::str;
    pub use core::str::FromStr;

    pub use bitcoin::io::Error as ioError;
    pub use bitcoin::io::Read;
    pub use bitcoin::io::Write;
    pub use hashbrown::HashMap;
    pub use hashbrown::HashSet;

    pub use crate::error::Error;
}

#[cfg(feature = "std")]
/// Provides implementation for basic `std` types, without assuming we have a `std` library.
///
/// This module is used to avoid having `#[cfg(feature = "no-std")]` sprinkled
/// around all crates that support `no-std`. It imports all types we would use
/// from the `stdlib`, either from the lib itself, or from other sources in case
/// `stdlib` isn't available.
pub mod prelude {
    extern crate alloc;
    extern crate std;
    pub use alloc::format;
    pub use alloc::string::ToString;
    pub use std::borrow::ToOwned;
    pub use std::boxed::Box;
    pub use std::collections::hash_map::Entry;
    pub use std::collections::HashMap;
    pub use std::collections::HashSet;
    pub use std::error::Error;
    pub use std::fmt::Display;
    pub use std::fmt::Formatter;
    pub use std::fmt::{self};
    pub use std::io::Error as ioError;
    pub use std::io::Read;
    pub use std::io::Write;
    pub use std::ops::Deref;
    pub use std::ops::DerefMut;
    pub use std::result::Result;
    pub use std::str::FromStr;
    pub use std::string::String;
    pub use std::sync;
    pub use std::vec;
    pub use std::vec::Vec;
}

#[cfg(test)]
mod tests {
    use bitcoin::hashes::Hash;
    use bitcoin::hex::DisplayHex;
    use bitcoin::ScriptBuf;

    use super::prelude::*;

    #[test]
    fn test_get_hash_from_u8() {
        let data = b"Hello, world!";
        let hash = super::get_hash_from_u8(data);
        let expected =
            String::from("315f5bdb76d078c43b8ac0064e4a0164612b1fce77c869345bfc94c75894edd3");
        assert_eq!(hash.as_byte_array().to_lower_hex_string(), expected);
    }

    #[test]
    fn test_get_spk_hash() {
        // Example taken from Electrum protocol documentation
        // https://electrum-protocol.readthedocs.io/en/latest/protocol-basics.html#script-hashes

        let spk =
            ScriptBuf::from_hex("76a91462e907b15cbf27d5425399ebf6f0fb50ebb88f1888ac").unwrap(); // P2PKH script
        let hash = super::get_spk_hash(&spk);
        let expected =
            String::from("8b01df4e368ea28f8dc0423bcf7a4923e3a12d307c875e47a0cfbf90b5c39161");

        assert_eq!(hash.as_byte_array().to_lower_hex_string(), expected);
    }

    #[test]
    fn test_fractional_avg() {
        let mut avg = super::FractionAvg::new(0, 0);
        assert_eq!(avg.value(), 0.0); // not initialized, should return 0
        avg.add(10);
        avg.add(20);
        avg.add(30);
        assert_eq!(avg.value(), 20.0);

        avg.add(40);
        assert_eq!(avg.value(), 25.0);

        avg.add(50);
        assert_eq!(avg.value(), 30.0);

        avg.add(1);
        assert_eq!(avg.value(), 25.166_666_666_666_668);

        avg.add(99);
        assert_eq!(avg.value(), 35.714_285_714_285_715);
    }
}
