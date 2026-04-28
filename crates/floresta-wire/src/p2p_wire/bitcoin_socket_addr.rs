// SPDX-License-Identifier: MIT OR Apache-2.0

use core::error;
/// A port of [`std::net::SocketAddr`] that also supports non-ip based addresses, such as Onion and I2P.
///
/// Inside floresta-wire, we rely heavily on passing addresses around, including several API-facing
/// methods. Each address might be one of several different types, such as IP address, I2P, Onion
/// or a domain that must be resolved. [`AddrV2`] wraps most of those — with the exception of
/// Domain. However, we still need a port to connect with those addresses, and [`AddrV2`] is only
/// the address part. To avoid passing ports as arguments every time, and remove duplicate address
/// parsing logic, we have this [`BitcoinSocketAddr`] struct.
///
/// It contains an inner [`AddrV2`] address and a [`u16`] port, a parsing logic for all supported
/// addresses, including optional DNS resolving.
///
/// The DNS part is handled by an implementation of the [`DnsResolver`] trait, by default we use
/// the [`SystemResolver`] implementation, that uses the OS resolver. If you prefer, you can bring
/// your own resolver.
use core::net::Ipv4Addr;
use core::net::Ipv6Addr;
use std::fmt::Display;
use std::io;
use std::net::IpAddr;
use std::str::FromStr;

use bitcoin::hex::DisplayHex;
use bitcoin::p2p::address::AddrV2;
use bitcoin::Network;
use rand::seq::SliceRandom;
use rand::thread_rng;

#[derive(Debug)]
/// An error returned when trying to parse an address
pub enum InvalidAddressError {
    /// The provided port is invalid
    InvalidPort,

    /// The provided address is invalid
    InvalidAddress,

    /// We've found an extra column character after the address string
    TrailingColumn,

    /// The provided hostname is either malformed or can't be resolved
    InvalidDNSName,

    /// The resolver returned no addresses
    NoAssociatedName,

    /// No port were provided
    MissingPort,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// A port of [`std::net::SocketAddr`] that supports all networks that we support.
///
/// It keeps an address and port, where the address can be anything that we can create a connection
/// with another Bitcoin node, such as IP, I2P and onion.
///
/// It comes with a comprehensive parsing logic that can be used to parse addresses and use them
/// internally and in interfaces.
pub struct BitcoinSocketAddr {
    /// The actual address.
    address: AddrV2,

    /// The port this address is listening to.
    port: u16,
}

/// A resolver that will be used by [`BitcoinSocketAddr`] to resolve hostnames.
pub trait DnsResolver {
    /// The error type returned by this resolver
    type Error: error::Error;

    /// The main method for this trait. It resolves a hostname and returns a list of associated
    /// addresses.
    fn resolve(&self, name: &str) -> Result<Vec<IpAddr>, Self::Error>;
}

/// A simple resolver that uses the default OS resolver as implementation.
pub struct SystemResolver;

impl DnsResolver for SystemResolver {
    type Error = io::Error;

    fn resolve(&self, name: &str) -> Result<Vec<IpAddr>, Self::Error> {
        dns_lookup::lookup_host(name)
    }
}

impl BitcoinSocketAddr {
    /// Returns that network's default port, if present
    ///
    /// Note: it takss an option because it makes the logic inside `parse_port_if_present` easier
    fn get_default_port(network: Option<Network>) -> Option<u16> {
        // TODO: Check these
        let port = match network? {
            Network::Signet => 38333,
            Network::Bitcoin => 8333,
            Network::Testnet => 18333,
            Network::Regtest => 48444,
            Network::Testnet4 => 48333,
        };

        Some(port)
    }

    /// Tries to parse a port, returns an error if nothing can be found.
    ///
    /// This will parsing a port in two ways:
    ///  - Looking for the port field inside the URL, i.e.: `<host>:<port>`
    ///  - Using the default port for a network
    ///
    ///  If neither are provided, this function returns an error. `network` is an [`Option`]
    ///  because some trait implementations such as [`FromStr`] won't allow us to pass a network
    ///  in, and we can't just hard-code mainnet. So you can still use those, but you are forced to
    ///  always explicitly provide a port through the URL.
    fn parse_port_if_present(
        port_str: Option<&str>,
        network: Option<Network>,
    ) -> Result<u16, InvalidAddressError> {
        port_str
            .map(|port| port.parse())
            .transpose()
            .map_err(|_| InvalidAddressError::InvalidPort)?
            .or(Self::get_default_port(network))
            .ok_or(InvalidAddressError::MissingPort)
    }

    /// Returns a reference to this node's address.
    pub const fn as_addrv2(&self) -> &AddrV2 {
        &self.address
    }

    /// Converts this [`BitcoinSocketAddr`] into an [`AddrV2`].
    pub fn to_addrv2(self) -> AddrV2 {
        self.address
    }

    /// Return the port for this address
    pub fn get_port(&self) -> u16 {
        self.port
    }

    /// Changes the port for this address
    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    /// Sets this node's address
    pub fn set_address(&mut self, address: AddrV2) {
        self.address = address;
    }

    /// Creates a new [`BitcoinSocketAddr`] giving an [`AddrV2`] and port.
    pub fn new(address: AddrV2, port: u16) -> Self {
        Self { address, port }
    }

    /// Creates a new [`BitcoinSocketAddr`] from an [`IpAddr`] and [`Network`].
    pub fn from_ip_addr(address: IpAddr, network: Network) -> Self {
        let address = match address {
            IpAddr::V4(ipv4) => AddrV2::Ipv4(ipv4),
            IpAddr::V6(ipv6) => AddrV2::Ipv6(ipv6),
        };

        let port = Self::get_default_port(Some(network)).expect("Network is always Some");

        BitcoinSocketAddr { address, port }
    }

    #[inline]
    /// DNS names may only use alphanumeric symbols and dots to represent subdomains.
    /// Anything other than `<name1>...<nameN>.<tld>` or single names such as `localhost`
    /// is forbidden.
    fn check_dns_name(name: &str) -> bool {
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
    }

    #[inline]
    /// Makes sure our address has exactly one `[` and one  `]`, and all the other chars are either
    /// hexadecimal or `:`.
    ///
    /// We don't validate the actual address, because [`Ipv6Addr`]'s parsing already does that.
    fn check_ipv6_chars(ip: &str) -> bool {
        let brackets_count = ip.chars().filter(|&c| c == '[' || c == ']').count();
        if !ip.contains("]") || brackets_count != 2 {
            return false;
        }

        ip.chars()
            .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '[' || c == ']' || c == '.')
    }

    /// Parses an IPv6 address and build a [`BitcoinSocketAddr`] from it.
    ///
    /// The goal here is to avoid using regex or anything more complex. A lot of the heavy-lifting
    /// is done by [`Ipv6Addr`], but it doesn't support ports, only addresses. We also require the
    /// square brackets for both compatibility with other Bitcoin software that requires it, and
    /// also because parsing ports is easier if we can search for a `]:` patter — we don't need to
    /// actually parse the IP address to find it.
    ///
    /// On our side, we just make sure the address follows the form `s/\[([A-Z] | [a - z] | (0 -
    /// 9) | \:]+\](:)?([0-9]+)?". The actual parse is handled by Rust's standard library.
    ///
    /// This function also handles Cjdns addresses, since they are basically Ipv6 addresses that
    /// are reserved by the Ipv6 standard.
    fn parse_v6(address: &str, network: Option<Network>) -> Result<Self, InvalidAddressError> {
        // make sure the address doesn't have any invalid chars
        if !Self::check_ipv6_chars(address) {
            return Err(InvalidAddressError::InvalidAddress);
        }

        // Tries to extract a port by splitting at ']:'. If no port were given, this will simply
        // return back the whole address, and the default port will be used if `network` is Some.
        let mut split = address.split_inclusive("]");
        let address = split
            .next()
            .and_then(|address| {
                // Denies anything after the
                let pos = address.find("]")?;
                if (pos + 1) != address.len() {
                    return None;
                }

                // remove the `[` and `]`
                let address = address.replace("[", "");
                let address = address.replace("]:", "");
                let address = address.replace("]", "");

                address.parse::<Ipv6Addr>().ok()
            })
            .ok_or(InvalidAddressError::InvalidAddress)?;

        // Remove the column if present
        let port = split.next().map(|port| port.replace(":", ""));
        let port = Self::parse_port_if_present(port.as_deref(), network)?;

        // Shouldn't have something like <ip>:<port>]<something>
        if split.next().is_some() {
            return Err(InvalidAddressError::TrailingColumn);
        }

        // CJDNS addresses use a special range for local addresses (FC00::/8)
        // See: https://github.com/cjdelisle/cjdns/tree/master/doc#what-is-notable-about-cjdns-why-should-i-use-it
        let octets = address.octets();
        if octets[0] == 0xFC {
            return Ok(Self {
                address: AddrV2::Cjdns(address),
                port,
            });
        }

        // Regular IPv6 otherwise
        Ok(Self {
            address: AddrV2::Ipv6(address),
            port,
        })
    }

    /// Parses an Ipv4 and returns the corresponding [`BitcoinSocketAddr`].
    ///
    /// This function expects something like 255.255.255.255<:65535>. It will use [`Ipv4Addr`] for
    /// the actual parsing. We don't allow anything after the port. All digits must either be a
    /// period, a column or a number. Everything else is denied.
    fn parse_v4(address: &str, network: Option<Network>) -> Result<Self, InvalidAddressError> {
        let mut split = address.split(":");
        let address = split.next().ok_or(InvalidAddressError::InvalidAddress)?;

        if let Ok(address) = address.parse::<Ipv4Addr>() {
            let port = Self::parse_port_if_present(split.next(), network)?;

            // Shouldn't have something like <ip>:<port>:<something>
            if split.next().is_some() {
                return Err(InvalidAddressError::TrailingColumn);
            }

            return Ok(Self {
                address: AddrV2::Ipv4(address),
                port,
            });
        }

        Err(InvalidAddressError::InvalidDNSName)
    }

    /// Parses and resolves a Dns address into a [`BitcoinSocketAddr`].
    ///
    /// This function uses the [`DnsResolver`] to lookup an address, and resolves it into the
    /// appropriated IP address. If multiple names are returned, a random one will be selected. If no
    /// names are returned, we return an error.
    ///
    /// Names MAY contain subnames, but they SHOULD NOT have non-alphanumeric characters. An MUST have AT
    /// MOST one column characters. URL schemes are not allowed.
    fn parse_dns(
        address: &str,
        network: Option<Network>,
        resolver: impl DnsResolver,
    ) -> Result<Self, InvalidAddressError> {
        let mut split = address.split(":");
        let address = split.next().ok_or(InvalidAddressError::InvalidAddress)?;
        let port = split.next();

        // Shouldn't have something like <host>:<port>:<something>
        if split.next().is_some() {
            return Err(InvalidAddressError::TrailingColumn);
        }

        let port = Self::parse_port_if_present(port, network)?;

        if !Self::check_dns_name(address) {
            return Err(InvalidAddressError::InvalidDNSName);
        }

        let addresses = resolver
            .resolve(address)
            .map_err(|_e| InvalidAddressError::InvalidDNSName)?;

        let selected_addr = addresses
            .choose(&mut thread_rng())
            .ok_or(InvalidAddressError::NoAssociatedName)?;

        let address = match selected_addr {
            IpAddr::V4(v4) => AddrV2::Ipv4(*v4),
            IpAddr::V6(v6) => AddrV2::Ipv6(*v6),
        };

        Ok(Self { address, port })
    }

    /// Parses and address from a [`str`] into a new [`BitcoinSocketAddr`].
    ///
    /// This method implements a very robust parser that can detect and parse any supported address
    /// scheme, including hostnames. You can use it to parse any string that should be treated as a
    /// Bitcoin node address.
    ///
    /// If you wish to parse hostnames, you should provide a [`DnsResolver`], such as
    /// [`SystemResolver`]. Then you can give an string like `www.example.com:8333` and this will
    /// be resolved into the appropriated IP address. If you don't want any resolver, you might
    /// just implement a no-op, but note that if it always returns an empty list of addresses,
    /// this method will always error out.
    ///
    /// The network parameter is used in case the string doesn't have a port, so we will use that
    /// network's default port. It will only be used iff the address doesn't already contain a
    /// port.
    pub fn parse_address(
        address: &str,
        network: Option<Network>,
        resolver: impl DnsResolver,
    ) -> Result<Self, InvalidAddressError> {
        if address.starts_with("[") {
            // IpV6 is very distinct because it starts with a '[' character.
            //
            // Here we try to build one, return an error if we can't
            return Self::parse_v6(address, network);
        }

        // This is an edge-case for some OS-specific behavior. Unix-systems define an empty host
        // as localhost, but windows doesn't. This forces empty strings to be localhost
        // independently for OS resolver.
        if address.is_empty() {
            let port = Self::parse_port_if_present(None, network)?;
            return Ok(Self {
                address: AddrV2::Ipv4(Ipv4Addr::LOCALHOST),
                port,
            });
        }

        // Try either V4 or DNS. It's a bit harder to distinguish between the two, so we try V4
        // first and fallback to DNS on failure. If DNS fail, we return an error.
        Self::parse_v4(address, network).or(Self::parse_dns(address, network, resolver))
    }
}

impl Display for BitcoinSocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let address = match self.address {
            AddrV2::TorV3(address) => address.to_lower_hex_string(),
            AddrV2::I2p(address) => address.to_lower_hex_string(),
            AddrV2::Ipv4(address) => address.to_string(),
            AddrV2::Ipv6(address) => format!("[{}]", address),
            AddrV2::TorV2(address) => address.to_lower_hex_string(),
            AddrV2::Cjdns(address) => format!("[{}]", address),
            AddrV2::Unknown(_, _) => "unknown".to_string(),
        };

        write!(f, "{address}:{}", self.port)
    }
}

impl FromStr for BitcoinSocketAddr {
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_address(s, None, SystemResolver)
    }

    type Err = InvalidAddressError;
}

impl From<BitcoinSocketAddr> for AddrV2 {
    fn from(value: BitcoinSocketAddr) -> Self {
        value.to_addrv2()
    }
}
