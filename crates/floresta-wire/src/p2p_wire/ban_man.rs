// SPDX-License-Identifier: MIT OR Apache-2.0

//! Centralized ban management for misbehaving peers.
//!
//! Tracks banned IP addresses with per-entry expiry, and persists them to
//! `bans.json` under the node's data directory so bans survive restarts.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// Default ban duration in seconds
const DEFAULT_BAN_DURATION: Duration = Duration::from_secs(60 * 60 * 24); // 24 hours

/// Absolute Unix timestamp in seconds.
type BanExpiry = u64;

/// Current Unix time in seconds, or 0 if the clock is before the epoch.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Default)]
/// Centralized manager for tracking banned IP addresses.
pub struct BanMan {
    banned: HashMap<IpAddr, BanExpiry>,
    /// Whether `banned` changed since the last successful dump to disk.
    dirty: bool,
}

impl BanMan {
    /// Creates a new empty [`BanMan`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Bans an IP address for `duration` from now.
    ///
    /// If `duration` is None, the default ban time (24 hours) will be used instead.
    /// An existing ban is only ever extended; a shorter re-ban is a no-op.
    pub fn add_ban(&mut self, ip: IpAddr, duration: Option<Duration>) {
        let ban_duration = duration.unwrap_or(DEFAULT_BAN_DURATION);
        let ban_until = now_secs().saturating_add(ban_duration.as_secs());

        if self
            .banned
            .get(&ip)
            .is_some_and(|&current| current >= ban_until)
        {
            return;
        }

        self.banned.insert(ip, ban_until);
        self.dirty = true;
    }

    /// Returns true if the IP has an unexpired ban.
    pub fn is_banned(&self, ip: IpAddr) -> bool {
        self.banned
            .get(&ip)
            .is_some_and(|&ban_until| ban_until > now_secs())
    }

    /// Returns the set of currently banned IPs, sweeping expired bans first.
    pub fn banned_ips(&mut self) -> Vec<IpAddr> {
        self.sweep_expired();
        self.banned.keys().copied().collect()
    }

    /// Removes every expired ban.
    fn sweep_expired(&mut self) {
        let now = now_secs();
        let before = self.banned.len();

        self.banned.retain(|_, ban_until| *ban_until > now);

        if self.banned.len() != before {
            self.dirty = true;
        }
    }

    /// Dumps the banned ips to `datadir/bans.json`, sweeping expired bans first.
    ///
    /// Skipped when nothing changed since the last dump. The file is created if it
    /// doesn't exist, and overwritten if it does.
    pub fn dump_bans(&mut self, datadir: impl AsRef<Path>) -> io::Result<()> {
        self.sweep_expired();
        if !self.dirty {
            return Ok(());
        }

        let bans = serde_json::to_string(&self.banned)?;
        fs::write(datadir.as_ref().join("bans.json"), bans)?;
        self.dirty = false;

        Ok(())
    }

    /// Loads the banned ips from `datadir/bans.json`, sweeping expired bans.
    ///
    /// A missing file is treated as an empty ban list (first run). An
    /// unreadable or malformed file returns an error so the operator
    /// notices instead of silently starting with no bans.
    pub fn load_bans(&mut self, datadir: impl AsRef<Path>) -> io::Result<()> {
        let path = datadir.as_ref().join("bans.json");
        match fs::read_to_string(&path) {
            Ok(persisted_bans) => {
                self.banned = serde_json::from_str(&persisted_bans)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                self.sweep_expired();
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
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
        let ban_man = BanMan::new();
        let ip = test_ip();

        assert!(!ban_man.is_banned(ip));
    }

    #[test]
    fn test_expired_ban_is_not_detected() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        // Insert a ban that already expired (ban_until is in the past)
        ban_man.banned.insert(ip, 0);

        assert!(!ban_man.is_banned(ip));
        // is_banned is read-only; only a sweep removes the entry
        assert!(ban_man.banned.contains_key(&ip));
    }

    #[test]
    fn test_sweep_removes_only_expired_bans() {
        let mut ban_man = BanMan::new();
        let expired = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let active = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        ban_man.banned.insert(expired, 0);
        ban_man.add_ban(active, Some(Duration::from_secs(3600)));

        assert_eq!(ban_man.banned_ips(), vec![active]);
        assert!(!ban_man.banned.contains_key(&expired));
    }

    #[test]
    fn test_shorter_reban_is_noop() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        ban_man.add_ban(ip, Some(Duration::from_secs(9999)));
        let first_ban = *ban_man.banned.get(&ip).unwrap();

        ban_man.add_ban(ip, Some(Duration::from_secs(100)));
        let second_ban = *ban_man.banned.get(&ip).unwrap();

        assert_eq!(first_ban, second_ban);
    }

    /// Creates a fresh temp datadir and returns it with the `bans.json` path inside it.
    fn temp_datadir(tag: &str) -> (String, String) {
        let datadir = format!(
            "{}/floresta_ban_{tag}_{}",
            std::env::temp_dir().display(),
            rand::random::<u32>()
        );
        std::fs::create_dir_all(&datadir).unwrap();
        let path = format!("{datadir}/bans.json");
        (datadir, path)
    }

    #[test]
    fn test_dump_only_when_dirty() {
        let (datadir, path) = temp_datadir("dirty");

        let mut ban_man = BanMan::new();
        ban_man.dump_bans(&datadir).unwrap();
        // Nothing changed since creation, so nothing is written
        assert!(!std::path::Path::new(&path).exists());

        ban_man.add_ban(test_ip(), Some(Duration::from_secs(3600)));
        assert!(ban_man.dirty);
        ban_man.dump_bans(&datadir).unwrap();
        assert!(!ban_man.dirty);
        assert!(std::path::Path::new(&path).exists());

        std::fs::remove_dir_all(&datadir).unwrap();
    }

    #[test]
    fn test_dump_sweeps_expired_entries() {
        let (datadir, path) = temp_datadir("sweep");
        let expired = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        let mut ban_man = BanMan::new();
        ban_man.add_ban(test_ip(), Some(Duration::from_secs(3600)));
        ban_man.banned.insert(expired, 0);
        ban_man.dump_bans(&datadir).unwrap();

        let persisted = std::fs::read_to_string(&path).unwrap();
        assert!(persisted.contains("192.168.1.1"));
        assert!(!persisted.contains("10.0.0.1"));
        assert!(!ban_man.banned.contains_key(&expired));

        std::fs::remove_dir_all(&datadir).unwrap();
    }

    #[test]
    fn test_load_malformed_bans_file_errors() {
        let (datadir, path) = temp_datadir("malformed");
        std::fs::write(&path, "not json").unwrap();

        let mut ban_man = BanMan::new();
        let err = ban_man.load_bans(&datadir).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(ban_man.banned.is_empty());

        std::fs::remove_dir_all(&datadir).unwrap();
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
    fn test_huge_duration_saturates() {
        let mut ban_man = BanMan::new();
        let ip = test_ip();

        ban_man.add_ban(ip, Some(Duration::MAX));

        assert_eq!(*ban_man.banned.get(&ip).unwrap(), u64::MAX);
        assert!(ban_man.is_banned(ip));
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
        let (datadir, _) = temp_datadir("persistence");

        let mut original_ban_man = BanMan::new();
        let ip = test_ip();
        original_ban_man.add_ban(ip, Some(Duration::from_secs(3600)));
        assert!(original_ban_man.is_banned(ip));

        original_ban_man
            .dump_bans(&datadir)
            .expect("Failed to dump bans to disk");

        let mut loaded_ban_man = BanMan::new();

        loaded_ban_man
            .load_bans(&datadir)
            .expect("Failed to load bans from disk");

        assert!(loaded_ban_man.is_banned(ip));

        std::fs::remove_dir_all(&datadir).unwrap();
    }
}
