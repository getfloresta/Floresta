// SPDX-License-Identifier: MIT OR Apache-2.0

//! JSON-RPC authentication primitives.
//!
//! Cookie auth mirrors Bitcoin Core's behavior. On startup the server writes a
//! single line of the form `__cookie__:<64-char-hex-token>` to the path the
//! caller passes to [`generate_cookie`].
//!
//! Callers are responsible for resolving the on-disk location. florestad
//! applies a network suffix (mainnet at the base, other networks one level
//! deeper) in `bin/florestad/src/main.rs::datadir_path` to match Core's
//! `<base>/[<net>/].cookie` convention.
//!
//! Clients then send `Authorization: Basic <base64(__cookie__:<token>)>`.
//!
//! The token rotates every restart; any pre-existing `.cookie` is silently
//! overwritten.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use bitcoin::hex::DisplayHex;

/// Username literal used for cookie auth (Core convention).
pub(crate) const COOKIE_USER: &str = "__cookie__";

/// Default cookie file name; placed under the net-specific datadir.
pub(crate) const COOKIE_FILE_NAME: &str = ".cookie";

/// Token length in raw random bytes; hex-encoded to 64 ASCII chars.
const COOKIE_TOKEN_BYTES: usize = 32;

/// Upper bound on the inbound `Authorization` header to cap base64 decode allocation.
const MAX_AUTH_HEADER_LEN: usize = 16 * 1024;

/// Generate a fresh cookie and write it to `path` with no trailing newline.
///
/// Writes go to `<path>.tmp` first with mode `0600` on Unix, then atomically
/// rename over `path`. A pre-existing cookie file is silently overwritten.
pub(crate) fn generate_cookie(path: &Path) -> io::Result<()> {
    let token: [u8; COOKIE_TOKEN_BYTES] = rand::random();
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

/// Errors produced by [`parse_basic_auth_header`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BasicAuthHeaderError {
    /// The header value did not start with the literal `"Basic "` prefix.
    MissingBasicPrefix,
    /// The base64 payload could not be decoded.
    InvalidBase64,
    /// The decoded payload was not valid UTF-8.
    InvalidUtf8,
    /// The decoded payload contained no `:` separator.
    MissingColon,
    /// The header value exceeded the inbound length cap.
    PayloadTooLarge,
}

impl fmt::Display for BasicAuthHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBasicPrefix => write!(f, "Authorization header must start with 'Basic '"),
            Self::InvalidBase64 => write!(f, "Authorization payload is not valid base64"),
            Self::InvalidUtf8 => write!(f, "Authorization payload is not valid UTF-8"),
            Self::MissingColon => write!(f, "Authorization payload missing ':' separator"),
            Self::PayloadTooLarge => write!(
                f,
                "Authorization header exceeds {MAX_AUTH_HEADER_LEN}-byte cap"
            ),
        }
    }
}

impl std::error::Error for BasicAuthHeaderError {}

/// Axum middleware that parses an inbound `Authorization: Basic` header and
/// logs the parsed username at debug level. Requests without the header or
/// with a malformed value are passed through unchanged; this layer does not
/// reject anything.
pub(crate) async fn auth_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if let Some(header) = req.headers().get(axum::http::header::AUTHORIZATION) {
        match header.to_str() {
            Ok(value) => match parse_basic_auth_header(value) {
                Ok((user, _)) => tracing::debug!("rpc auth header parsed for user {user}"),
                Err(e) => tracing::debug!("rpc auth header parse failed: {e}"),
            },
            Err(_) => tracing::debug!("rpc auth header is not valid ascii"),
        }
    }
    next.run(req).await
}

/// Parse an HTTP `Authorization: Basic <b64>` header value into `(user, pass)`.
///
/// Mirrors Bitcoin Core: the `"Basic "` prefix check is case-sensitive, the
/// base64 payload is whitespace-trimmed before decoding, and the split between
/// user and password is on the **first** `:`. Passwords may contain `:`;
/// usernames may not.
pub(crate) fn parse_basic_auth_header(
    value: &str,
) -> Result<(String, String), BasicAuthHeaderError> {
    if value.len() > MAX_AUTH_HEADER_LEN {
        return Err(BasicAuthHeaderError::PayloadTooLarge);
    }
    let payload = value
        .strip_prefix("Basic ")
        .ok_or(BasicAuthHeaderError::MissingBasicPrefix)?
        .trim();
    let decoded = BASE64
        .decode(payload)
        .map_err(|_| BasicAuthHeaderError::InvalidBase64)?;
    let decoded = String::from_utf8(decoded).map_err(|_| BasicAuthHeaderError::InvalidUtf8)?;
    let (user, pass) = decoded
        .split_once(':')
        .ok_or(BasicAuthHeaderError::MissingColon)?;
    Ok((user.to_string(), pass.to_string()))
}

/// Remove the cookie file at `path`. Treats `NotFound` as success so shutdown
/// is idempotent. Caller must only invoke this after a successful
/// [`generate_cookie`] in this process.
pub(crate) fn delete_cookie(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
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

    #[test]
    fn delete_cookie_removes_existing_file() {
        let path = tmp_cookie_path("delete_existing");
        generate_cookie(&path).expect("cookie write should succeed in test");
        assert!(path.exists());

        delete_cookie(&path).expect("delete should succeed for an existing file");
        assert!(!path.exists());
    }

    #[test]
    fn delete_cookie_is_idempotent_on_missing_file() {
        let path = tmp_cookie_path("delete_missing");
        assert!(!path.exists());

        delete_cookie(&path).expect("delete is idempotent on missing file");
        delete_cookie(&path).expect("delete is idempotent on missing file");
    }

    #[test]
    fn parse_basic_auth_header_accepts_valid_header() {
        // base64("alice:hunter2") = "YWxpY2U6aHVudGVyMg=="
        let (user, pass) = parse_basic_auth_header("Basic YWxpY2U6aHVudGVyMg==")
            .expect("valid Basic header should parse");
        assert_eq!(user, "alice");
        assert_eq!(pass, "hunter2");
    }

    #[test]
    fn parse_basic_auth_header_trims_whitespace_around_payload() {
        let (user, pass) = parse_basic_auth_header("Basic   YWxpY2U6aHVudGVyMg==  ")
            .expect("valid Basic header with whitespace should parse");
        assert_eq!(user, "alice");
        assert_eq!(pass, "hunter2");
    }

    #[test]
    fn parse_basic_auth_header_allows_colon_in_password() {
        // base64("alice:pass:with:colons") = "YWxpY2U6cGFzczp3aXRoOmNvbG9ucw=="
        let (user, pass) = parse_basic_auth_header("Basic YWxpY2U6cGFzczp3aXRoOmNvbG9ucw==")
            .expect("valid Basic header with colons in pass should parse");
        assert_eq!(user, "alice");
        assert_eq!(pass, "pass:with:colons");
    }

    #[test]
    fn parse_basic_auth_header_rejects_wrong_scheme() {
        // The "Basic " prefix is case-sensitive; reject "basic ", "Bearer ...", and empty.
        assert_eq!(
            parse_basic_auth_header("basic YWxpY2U6aHVudGVyMg=="),
            Err(BasicAuthHeaderError::MissingBasicPrefix),
        );
        assert_eq!(
            parse_basic_auth_header("Bearer YWxpY2U6aHVudGVyMg=="),
            Err(BasicAuthHeaderError::MissingBasicPrefix),
        );
        assert_eq!(
            parse_basic_auth_header(""),
            Err(BasicAuthHeaderError::MissingBasicPrefix),
        );
    }

    #[test]
    fn parse_basic_auth_header_rejects_bad_base64() {
        assert_eq!(
            parse_basic_auth_header("Basic !!!not-base64!!!"),
            Err(BasicAuthHeaderError::InvalidBase64),
        );
    }

    #[test]
    fn parse_basic_auth_header_rejects_missing_colon() {
        // base64("nocolon") = "bm9jb2xvbg=="
        assert_eq!(
            parse_basic_auth_header("Basic bm9jb2xvbg=="),
            Err(BasicAuthHeaderError::MissingColon),
        );
    }

    #[test]
    fn parse_basic_auth_header_rejects_oversized_payload() {
        let oversized = format!("Basic {}", "A".repeat(MAX_AUTH_HEADER_LEN));
        assert_eq!(
            parse_basic_auth_header(&oversized),
            Err(BasicAuthHeaderError::PayloadTooLarge),
        );
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
