// SPDX-License-Identifier: MIT OR Apache-2.0

//! JSON-RPC authentication primitives.
//!
//! Cookie auth mirrors Bitcoin Core's behavior. On startup the server writes a
//! single line of the form `__cookie__:<64-char-hex-token>` to the path the
//! caller passes to [`generate_cookie`]. florestad passes a network-suffixed
//! data directory, so the on-disk layout matches Core's `<base>/[<net>/].cookie`
//! convention (mainnet sits at the root, every other network sits one level
//! deeper).
//!
//! Clients then send `Authorization: Basic <base64(__cookie__:<token>)>`.
//!
//! The token rotates every restart; any pre-existing `.cookie` is silently
//! overwritten.

use std::ffi::OsString;
use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use bitcoin::hex::DisplayHex;
use rand::Rng;

/// Username literal used for cookie auth (Core convention).
pub(crate) const COOKIE_USER: &str = "__cookie__";

/// Default cookie file name; placed under the net-specific datadir.
pub(crate) const COOKIE_FILE_NAME: &str = ".cookie";

/// Token length in raw random bytes; hex-encoded to 64 ASCII chars.
const COOKIE_TOKEN_BYTES: usize = 32;

/// Generate a fresh cookie and write it to `path` with no trailing newline.
///
/// Writes go to `<path>.tmp` first with mode `0600` on Unix, then atomically
/// rename over `path`. A pre-existing cookie file is silently overwritten.
pub(crate) fn generate_cookie(path: &Path) -> io::Result<()> {
    let mut token = [0u8; COOKIE_TOKEN_BYTES];
    rand::rng().fill(&mut token);
    let auth = format!("{COOKIE_USER}:{}", token.to_lower_hex_string());

    let tmp = tmp_path(path);

    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    let mut file = opts.open(&tmp)?;
    file.write_all(auth.as_bytes())?;
    drop(file);

    fs::rename(&tmp, path)?;

    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut buf = OsString::from(path);
    buf.push(".tmp");
    PathBuf::from(buf)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn tmp_cookie_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "floresta_cookie_test_{name}_{}",
            rand::random::<u32>()
        ));
        p
    }

    #[test]
    fn generate_cookie_writes_expected_format() {
        let path = tmp_cookie_path("format");
        generate_cookie(&path).expect("cookie write should succeed in test");

        let written = fs::read_to_string(&path).expect("test wrote this cookie file above");
        assert!(
            written.starts_with("__cookie__:"),
            "cookie file missing prefix: {written}"
        );
        let token = written
            .strip_prefix("__cookie__:")
            .expect("generated cookie always begins with __cookie__:");
        assert_eq!(
            token.len(),
            64,
            "token should be 64 hex chars, got {}",
            token.len()
        );
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "token should be lowercase hex: {token}",
        );

        fs::remove_file(&path).ok();
    }

    #[test]
    fn generate_cookie_writes_no_trailing_newline() {
        let path = tmp_cookie_path("newline");
        generate_cookie(&path).expect("cookie write should succeed in test");

        let bytes = fs::read(&path).expect("test wrote this cookie file above");
        assert!(!bytes.ends_with(b"\n"), "file should not end with newline");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn generate_cookie_produces_distinct_tokens() {
        let path1 = tmp_cookie_path("distinct1");
        let path2 = tmp_cookie_path("distinct2");
        generate_cookie(&path1).expect("cookie write should succeed in test");
        generate_cookie(&path2).expect("cookie write should succeed in test");
        let auth1 = fs::read_to_string(&path1).expect("test wrote path1 above");
        let auth2 = fs::read_to_string(&path2).expect("test wrote path2 above");
        assert_ne!(
            auth1, auth2,
            "two consecutive calls produced identical tokens"
        );

        fs::remove_file(&path1).ok();
        fs::remove_file(&path2).ok();
    }

    #[test]
    fn generate_cookie_overwrites_existing_file() {
        let path = tmp_cookie_path("overwrite");
        fs::write(&path, "stale-content").expect("test temp path should be writable");
        generate_cookie(&path).expect("cookie write should succeed in test");
        let written = fs::read_to_string(&path).expect("test wrote this cookie file above");
        assert_ne!(written, "stale-content", "stale content was not replaced");
        assert!(
            written.starts_with("__cookie__:"),
            "replacement is not a cookie line: {written}"
        );

        fs::remove_file(&path).ok();
    }

    #[test]
    fn generate_cookie_leaves_no_tmp_file() {
        let path = tmp_cookie_path("notmp");
        generate_cookie(&path).expect("cookie write should succeed in test");
        let tmp = tmp_path(&path);
        assert!(
            !tmp.exists(),
            "tmp file should be renamed away, got {tmp:?}"
        );

        fs::remove_file(&path).ok();
    }

    #[test]
    fn generate_cookie_recovers_from_stale_tmp_file() {
        let path = tmp_cookie_path("stale_tmp");
        let tmp = tmp_path(&path);

        // Simulate a previous run that crashed between create and rename:
        // a stale <path>.tmp file lingers with arbitrary content.
        fs::write(&tmp, b"partial-from-crashed-run").expect("test temp path should be writable");
        assert!(tmp.exists(), "precondition: stale tmp must exist");

        generate_cookie(&path).expect("cookie write should succeed in test");

        let written = fs::read_to_string(&path).expect("test wrote this cookie file above");
        assert!(
            written.starts_with("__cookie__:"),
            "cookie file should hold a fresh cookie, got {written}"
        );
        assert!(!tmp.exists(), "stale tmp file should be consumed by rename");

        fs::remove_file(&path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn generate_cookie_sets_owner_only_mode_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let path = tmp_cookie_path("perms");
        generate_cookie(&path).expect("cookie write should succeed in test");
        let mode = fs::metadata(&path)
            .expect("test wrote this cookie file above")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");

        fs::remove_file(&path).ok();
    }
}
