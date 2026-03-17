use std::collections::HashMap;
use std::net::IpAddr;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// Default ban duration in seconds
const DEFAULT_BAN_DURATION: u64 = 60 * 60 * 24;

/// Represents a subnet that can be banned.
///
/// A single IP ban is stored as /32 (IPv4) or /128 (IPv6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BanSubnet {
    /// The network address
    pub addr: IpAddr,
    /// The prefix length (e.g. 24 for a /24 subnet)
    pub prefix: u8,
}

/// Centralized manager for tracking banned IP addresses.
#[derive(Debug, Default)]
pub struct BanMan {
    banned: HashMap<BanSubnet, u64>,
}

impl BanSubnet {
    /// Creates a new BanSubnet from an IP address and prefix length
    pub fn new(addr: IpAddr, prefix: u8) -> Self {
        Self { addr, prefix }
    }

    /// Creates a single-host subnet (/32 for IPv4, /128 for IPv6)
    pub fn from_ip(ip: IpAddr) -> Self {
        let prefix = match ip {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        Self::new(ip, prefix)
    }

    /// Returns true if the given IP falls within this subnet.
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(candidate)) => {
                let mask = u32::MAX.checked_shl(32 - self.prefix as u32).unwrap_or(0);
                u32::from(net) & mask == u32::from(candidate) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(candidate)) => {
                let mask = u128::MAX.checked_shl(128 - self.prefix as u32).unwrap_or(0);
                u128::from(net) & mask == u128::from(candidate) & mask
            }
            _ => false,
        }
    }
}

impl BanMan {
    /// Creates a new empty Banman
    pub fn new() -> Self {
        Self {
            banned: HashMap::new(),
        }
    }

    /// Bans an IP subnet for `duration` seconds from now
    ///
    /// If `duration` is 0, the default ban time (24 hours) will be use
    pub fn add_ban(&mut self, subnet: BanSubnet, duration: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let ban_duration = if duration == 0 {
            DEFAULT_BAN_DURATION
        } else {
            duration
        };

        let ban_until = now + ban_duration;
        self.banned.insert(subnet, ban_until);
    }

    /// Returns true if the IP is banned
    ///
    /// Expired bans are removed automatically
    pub fn is_banned(&mut self, ip: IpAddr) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Remove expired bans first
        self.banned.retain(|_, ban_until| *ban_until > now);

        // Check if any subnet contains the IP
        self.banned.keys().any(|subnet| subnet.contains(ip))
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::net::Ipv4Addr;

    use super::*;

    fn test_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))
    }

    #[test]
    fn test_banned_ip_is_detected() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        ban_man.add_ban(BanSubnet::from_ip(ip), 3600);
        assert!(ban_man.is_banned(ip));
    }

    #[test]
    fn test_unbanned_ip_is_not_detected() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        assert!(!ban_man.is_banned(ip));
    }

    #[test]
    fn test_expired_ban_is_cleaned_up() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        // Insert a ban that already expired (ban_until is in the past)
        ban_man.banned.insert(BanSubnet::from_ip(ip), 0);

        assert!(!ban_man.is_banned(ip));
        // Verify it was removed from the map
        assert!(ban_man.banned.is_empty());
    }

    #[test]
    fn test_default_duration_when_zero() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();
        let subnet = BanSubnet::from_ip(ip);

        ban_man.add_ban(subnet.clone(), 0);

        // Should be banned for 24 hours from now
        let ban_until = ban_man.banned.get(&subnet).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let expected = now + DEFAULT_BAN_DURATION;
        // Allow 1 second tolerance for test execution time
        assert!(*ban_until >= expected - 1 && *ban_until <= expected + 1);
    }

    #[test]
    fn test_multiple_ips_banned_independently() {
        let mut ban_man = BanMan::new();
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        ban_man.add_ban(BanSubnet::from_ip(ip1), 3600);

        assert!(ban_man.is_banned(ip1));
        assert!(!ban_man.is_banned(ip2));
    }

    #[test]
    fn test_reban_updates_expiry() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();
        let subnet = BanSubnet::from_ip(ip);

        ban_man.add_ban(subnet.clone(), 100);
        let first_ban = *ban_man.banned.get(&subnet).unwrap();

        ban_man.add_ban(subnet.clone(), 9999);
        let second_ban = *ban_man.banned.get(&subnet).unwrap();

        assert!(second_ban > first_ban);
    }

    #[test]
    fn test_subnet_ban_covers_all_ips_in_range() {
        let mut ban_man = BanMan::new();
        // Ban 192.168.1.0/24 — all of 192.168.1.*
        let subnet = BanSubnet::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0)), 24);
        ban_man.add_ban(subnet, 3600);

        assert!(ban_man.is_banned(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(ban_man.is_banned(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 255))));
        assert!(!ban_man.is_banned(IpAddr::V4(Ipv4Addr::new(192, 168, 2, 1))));
    }
}
