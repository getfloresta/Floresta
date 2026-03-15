use std::collections::HashMap;
use std::net::IpAddr;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// Default ban duration in seconds
const DEFAULT_BAN_DURATION: u64 = 60 * 60 * 24;

/// Centralized manager for tracking banned IP addresses.
#[derive(Debug, Default)]
pub struct BanMan {
    banned: HashMap<IpAddr, u64>,
}

impl BanMan {
    /// Creates a new empty Banman
    pub fn new() -> Self {
        Self {
            banned: HashMap::new(),
        }
    }

    /// Bans an IP address for `duration` seconds from now
    ///
    /// If `duration` is 0, the default ban time (24 hours) will be use
    pub fn add_ban(&mut self, ip: IpAddr, duration: u64) {
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
        self.banned.insert(ip, ban_until);
    }

    /// Returns true if the IP is banned
    ///
    /// Expired bans are removed automatically
    pub fn is_banned(&mut self, ip: IpAddr) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if let Some(ban_until) = self.banned.get(&ip) {
            if *ban_until > now {
                return true;
            }
            // Ban expired, clean it up
            self.banned.remove(&ip);
        }

        false
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

        ban_man.add_ban(ip, 3600);
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
        ban_man.banned.insert(ip, 0);

        assert!(!ban_man.is_banned(ip));
        // Verify it was removed from the map
        assert!(!ban_man.banned.contains_key(&ip));
    }

    #[test]
    fn test_default_duration_when_zero() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        ban_man.add_ban(ip, 0);

        // Should be banned for 24 hours from now
        let ban_until = ban_man.banned.get(&ip).unwrap();
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

        ban_man.add_ban(ip1, 3600);

        assert!(ban_man.is_banned(ip1));
        assert!(!ban_man.is_banned(ip2));
    }

    #[test]
    fn test_reban_updates_expiry() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        ban_man.add_ban(ip, 100);
        let first_ban = *ban_man.banned.get(&ip).unwrap();

        ban_man.add_ban(ip, 9999);
        let second_ban = *ban_man.banned.get(&ip).unwrap();

        assert!(second_ban > first_ban);
    }
}
