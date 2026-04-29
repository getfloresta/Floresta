// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// Default ban duration in seconds
const DEFAULT_BAN_DURATION: Duration = Duration::from_secs(60 * 60 * 24);

/// Absolute Unix timestamp in seconds.
type BanExpiry = u64;

#[derive(Debug, Default)]
/// Centralized manager for tracking banned IP addresses.
pub struct BanMan {
    banned: HashMap<IpAddr, BanExpiry>,
}

impl BanMan {
    /// Creates a new empty [`BanMan`].
    pub fn new() -> Self {
        Self {
            banned: HashMap::new(),
        }
    }

    /// Bans an IP address for `duration` from now
    ///
    /// If `duration` is None, the default ban time (24 hours) will be used instead.
    pub fn add_ban(&mut self, ip: IpAddr, duration: Option<Duration>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let ban_duration = duration.unwrap_or(DEFAULT_BAN_DURATION);

        let ban_until = now + ban_duration.as_secs();
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

    /// Returns the set of currently banned IPs.
    ///
    /// Only includes IPs whose ban has not yet expired.
    pub fn banned_ips(&self) -> Vec<IpAddr> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.banned
            .iter()
            .filter(|(_, ban_until)| **ban_until > now)
            .map(|(ip, _)| *ip)
            .collect()
    }

    /// Dumps the banned ips to a file on dir `datadir/bans.json` in json format
    ///
    /// The file is created if it doesn't exist, and overwritten if it does.
    pub fn dump_bans(&self, datadir: &str) -> std::io::Result<()> {
        let bans: Result<String, serde_json::Error> = serde_json::to_string(&self.banned);
        if let Ok(bans) = bans {
            std::fs::write(datadir.to_owned() + "/bans.json", bans)?;
        }
        Ok(())
    }

    /// Loads the banned ips from a file on dir `datadir/bans.json
    pub fn load_bans(&mut self, datadir: &str) {
        if let Ok(persisted_bans) = std::fs::read_to_string(format!("{datadir}/bans.json")) {
            if let Ok(bans) = serde_json::from_str::<HashMap<IpAddr, BanExpiry>>(&persisted_bans) {
                self.banned = bans;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::net::Ipv4Addr;
    use std::time::Duration;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use super::BanMan;
    use super::DEFAULT_BAN_DURATION;

    fn test_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))
    }

    #[test]
    fn test_banned_ip_is_detected() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        ban_man.add_ban(ip, Some(Duration::from_secs(3600)));
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
    fn test_default_duration_when_none() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        ban_man.add_ban(ip, None);

        // Should be banned for 24 hours from now
        let ban_until = ban_man.banned.get(&ip).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let expected = now + DEFAULT_BAN_DURATION.as_secs();
        // Allow 1 second tolerance for test execution time
        assert!(*ban_until >= expected - 1 && *ban_until <= expected + 1);
    }

    #[test]
    fn test_multiple_ips_banned_independently() {
        let mut ban_man = BanMan::new();
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        ban_man.add_ban(ip1, Some(Duration::from_secs(3600)));

        assert!(ban_man.is_banned(ip1));
        assert!(!ban_man.is_banned(ip2));
    }

    #[test]
    fn test_reban_updates_expiry() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        ban_man.add_ban(ip, Some(Duration::from_secs(100)));
        let first_ban = *ban_man.banned.get(&ip).unwrap();

        ban_man.add_ban(ip, Some(Duration::from_secs(9999)));
        let second_ban = *ban_man.banned.get(&ip).unwrap();

        assert!(second_ban > first_ban);
    }

    #[test]
    fn test_ban_man_persistence() {
        let datadir = format!(
            "{}/floresta_ban_test_{}",
            std::env::temp_dir().display(),
            rand::random::<u32>()
        );
        std::fs::create_dir_all(&datadir).unwrap();

        let mut original_ban_man = BanMan::new();
        let ip = test_ip();
        original_ban_man.add_ban(ip, Some(Duration::from_secs(3600)));
        assert!(original_ban_man.is_banned(ip));

        original_ban_man
            .dump_bans(&datadir)
            .expect("Failed to dump bans to disk");

        let mut loaded_ban_man = BanMan::new();

        loaded_ban_man.load_bans(&datadir);

        assert!(loaded_ban_man.is_banned(ip));

        std::fs::remove_dir_all(&datadir).unwrap();
    }
}
