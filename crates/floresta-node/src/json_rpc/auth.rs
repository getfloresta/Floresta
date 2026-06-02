// SPDX-License-Identifier: MIT OR Apache-2.0

//! JSON-RPC authentication primitives.
//!
//! Cookie auth mirrors Bitcoin Core's behavior. On startup the server writes a
//! single line of the form `__cookie__:<64-char-hex-token>` to the path the
//! caller passes to [`generate_cookie`].
//!
//! Callers are responsible for resolving the on-disk location. A common
//! convention is Bitcoin Core's `<base>/[<net>/].cookie` layout, where
//! non-mainnet networks nest one level deeper than mainnet.
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

use axum::body::Body;
use axum::extract::Request;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::header::WWW_AUTHENTICATE;
use axum::middleware::Next;
use axum::response::Response;
use bitcoin::hex::DisplayHex;

/// Username literal used for cookie auth (Core convention).
pub(crate) const COOKIE_USER: &str = "__cookie__";

/// Default cookie file name; placed under the net-specific datadir.
pub(crate) const COOKIE_FILE_NAME: &str = ".cookie";

/// Token length in raw random bytes; hex-encoded to 64 ASCII chars.
const COOKIE_TOKEN_BYTES: usize = 32;

/// Generate a fresh cookie, write it to `path` with no trailing newline, and
/// return the `__cookie__:<hex>` line for in-process validation.
///
/// Writes go to `<path>.tmp` first with mode `0600` on Unix, then atomically
/// rename over `path`. A pre-existing cookie file is silently overwritten.
pub(crate) fn generate_cookie(path: &Path) -> io::Result<String> {
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

    Ok(auth)
}

/// `Authorization: Basic` header parsing.
pub(crate) mod basic {
    use core::error::Error;
    use core::fmt;

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;

    /// Upper bound on the inbound header to cap base64 decode allocation.
    pub(super) const MAX_LEN: usize = 16 * 1024;

    #[derive(Debug, PartialEq, Eq)]
    /// Errors produced by [`parse_header`].
    pub(crate) enum HeaderError {
        /// The header value did not start with the literal `"Basic "` prefix.
        MissingBasicPrefix,

        /// The base64 payload could not be decoded.
        InvalidBase64,

        /// The decoded payload contained non-ASCII bytes.
        NonAsciiPayload,

        /// The decoded payload contained no `:` separator.
        MissingColon,

        /// The header value exceeded the inbound length cap.
        PayloadTooLarge,
    }

    impl fmt::Display for HeaderError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::MissingBasicPrefix => {
                    write!(f, "Authorization header must start with 'Basic '")
                }
                Self::InvalidBase64 => write!(f, "Authorization payload is not valid base64"),
                Self::NonAsciiPayload => {
                    write!(f, "Authorization payload contains non-ASCII bytes")
                }
                Self::MissingColon => write!(f, "Authorization payload missing ':' separator"),
                Self::PayloadTooLarge => {
                    write!(f, "Authorization header exceeds {MAX_LEN}-byte cap")
                }
            }
        }
    }

    impl Error for HeaderError {}

    /// Parse an HTTP `Authorization: Basic <b64>` header value into `(user, pass)`.
    ///
    /// Mirrors Bitcoin Core: the `"Basic "` prefix check is case-sensitive, the
    /// base64 payload is whitespace-trimmed before decoding, and the split
    /// between user and password is on the **first** `:`. Passwords may contain
    /// `:`; usernames may not.
    pub(crate) fn parse_header(value: &str) -> Result<(String, String), HeaderError> {
        if value.len() > MAX_LEN {
            return Err(HeaderError::PayloadTooLarge);
        }
        let payload = value
            .strip_prefix("Basic ")
            .ok_or(HeaderError::MissingBasicPrefix)?
            .trim();
        let decoded = BASE64
            .decode(payload)
            .map_err(|_| HeaderError::InvalidBase64)?;
        if !decoded.is_ascii() {
            return Err(HeaderError::NonAsciiPayload);
        }
        let decoded = core::str::from_utf8(&decoded).expect("ASCII implies UTF-8");
        let (user, pass) = decoded.split_once(':').ok_or(HeaderError::MissingColon)?;
        Ok((user.to_string(), pass.to_string()))
    }
}

/// Axum middleware that gates each request on the configured [`Auth`].
/// Missing, malformed, non-ASCII, or non-matching `Authorization: Basic`
/// headers all return HTTP 401 with `WWW-Authenticate: Basic realm="jsonrpc"`
/// per RFC 7235. Matching requests pass through to the handler.
pub(crate) async fn auth_middleware(
    State(creds): State<std::sync::Arc<Auth>>,
    req: Request,
    next: Next,
) -> Response {
    let Some(header) = req.headers().get(AUTHORIZATION) else {
        tracing::debug!("rpc auth header missing; rejecting");
        return unauthorized();
    };
    let value = match header.to_str() {
        Ok(s) => s,
        Err(_) => {
            tracing::debug!("rpc auth header is not valid ascii; rejecting");
            return unauthorized();
        }
    };
    let (user, pass) = match basic::parse_header(value) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::debug!("rpc auth header parse failed: {e}; rejecting");
            return unauthorized();
        }
    };
    if !creds.matches(&user, &pass) {
        tracing::debug!("rpc auth credentials mismatched for user {user}; rejecting");
        return unauthorized();
    }
    tracing::debug!("rpc auth ok for user {user}");
    next.run(req).await
}

fn unauthorized() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(WWW_AUTHENTICATE, r#"Basic realm="jsonrpc""#)
        .body(Body::empty())
        .expect("static 401 response is always well-formed")
}

/// Configured RPC credentials for this process. The middleware compares each
/// inbound `Authorization: Basic` request against the stored value via
/// [`Auth::matches`].
pub(crate) enum Auth {
    /// Cookie auth. Stores the full `__cookie__:<hex>` line as written to
    /// disk by [`generate_cookie`].
    Cookie(String),
}

impl Auth {
    /// True if the supplied basic-auth `user`/`pass` pair authenticates
    /// against the configured credentials. All comparisons are constant-time.
    pub(crate) fn matches(&self, user: &str, pass: &str) -> bool {
        match self {
            Self::Cookie(expected) => {
                constant_time_eq(format!("{user}:{pass}").as_bytes(), expected.as_bytes())
            }
        }
    }
}

/// Constant-time byte slice comparison. Returns `false` immediately on length
/// mismatch (lengths of both comparands are public), then XORs every byte into
/// an accumulator before returning.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
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
        let auth = generate_cookie(&path).expect("cookie write should succeed in test");

        assert!(
            auth.starts_with("__cookie__:"),
            "auth string missing prefix: {auth}"
        );
        let token = auth
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

        let written = fs::read_to_string(&path).expect("test wrote this cookie file above");
        assert_eq!(
            written, auth,
            "file content should match returned auth string"
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
        let auth1 = generate_cookie(&path1).expect("cookie write should succeed in test");
        let auth2 = generate_cookie(&path2).expect("cookie write should succeed in test");
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
        let auth = generate_cookie(&path).expect("cookie write should succeed in test");
        let written = fs::read_to_string(&path).expect("test wrote this cookie file above");
        assert_eq!(
            written, auth,
            "file content should match returned auth string"
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
        let (user, pass) = basic::parse_header("Basic YWxpY2U6aHVudGVyMg==")
            .expect("valid Basic header should parse");
        assert_eq!(user, "alice");
        assert_eq!(pass, "hunter2");
    }

    #[test]
    fn parse_basic_auth_header_trims_whitespace_around_payload() {
        let (user, pass) = basic::parse_header("Basic   YWxpY2U6aHVudGVyMg==  ")
            .expect("valid Basic header with whitespace should parse");
        assert_eq!(user, "alice");
        assert_eq!(pass, "hunter2");
    }

    #[test]
    fn parse_basic_auth_header_allows_colon_in_password() {
        // base64("alice:pass:with:colons") = "YWxpY2U6cGFzczp3aXRoOmNvbG9ucw=="
        let (user, pass) = basic::parse_header("Basic YWxpY2U6cGFzczp3aXRoOmNvbG9ucw==")
            .expect("valid Basic header with colons in pass should parse");
        assert_eq!(user, "alice");
        assert_eq!(pass, "pass:with:colons");
    }

    #[test]
    fn parse_basic_auth_header_rejects_wrong_scheme() {
        // The "Basic " prefix is case-sensitive; reject "basic ", "Bearer ...", and empty.
        assert_eq!(
            basic::parse_header("basic YWxpY2U6aHVudGVyMg=="),
            Err(basic::HeaderError::MissingBasicPrefix),
        );
        assert_eq!(
            basic::parse_header("Bearer YWxpY2U6aHVudGVyMg=="),
            Err(basic::HeaderError::MissingBasicPrefix),
        );
        assert_eq!(
            basic::parse_header(""),
            Err(basic::HeaderError::MissingBasicPrefix),
        );
    }

    #[test]
    fn parse_basic_auth_header_rejects_bad_base64() {
        assert_eq!(
            basic::parse_header("Basic !!!not-base64!!!"),
            Err(basic::HeaderError::InvalidBase64),
        );
    }

    #[test]
    fn parse_basic_auth_header_rejects_missing_colon() {
        // base64("nocolon") = "bm9jb2xvbg=="
        assert_eq!(
            basic::parse_header("Basic bm9jb2xvbg=="),
            Err(basic::HeaderError::MissingColon),
        );
    }

    #[test]
    fn parse_basic_auth_header_rejects_non_ascii_payload() {
        // base64("аlice:hunter2") with a Cyrillic 'а' (U+0430) as the first byte.
        assert_eq!(
            basic::parse_header("Basic 0LBsaWNlOmh1bnRlcjI="),
            Err(basic::HeaderError::NonAsciiPayload),
        );
    }

    #[test]
    fn parse_basic_auth_header_rejects_non_utf8_payload() {
        // base64(0xff 0xfe 0xfd) = "//79". Not valid UTF-8; caught by the
        // ASCII gate before any string conversion happens.
        assert_eq!(
            basic::parse_header("Basic //79"),
            Err(basic::HeaderError::NonAsciiPayload),
        );
    }

    #[test]
    fn parse_basic_auth_header_rejects_oversized_payload() {
        let oversized = format!("Basic {}", "A".repeat(basic::MAX_LEN));
        assert_eq!(
            basic::parse_header(&oversized),
            Err(basic::HeaderError::PayloadTooLarge),
        );
    }

    #[test]
    fn constant_time_eq_returns_true_for_equal_bytes() {
        assert!(constant_time_eq(b"abcdef", b"abcdef"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn constant_time_eq_returns_false_for_different_bytes() {
        assert!(!constant_time_eq(b"abcdef", b"abcdeg"));
        assert!(!constant_time_eq(b"abcdef", b"xbcdef"));
    }

    #[test]
    fn constant_time_eq_returns_false_for_length_mismatch() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
        assert!(!constant_time_eq(b"", b"a"));
    }

    #[test]
    fn cookie_credentials_match_their_own_user_and_pass() {
        let path = tmp_cookie_path("creds_cookie");
        let auth = generate_cookie(&path).expect("cookie write should succeed in test");
        let creds = Auth::Cookie(auth.clone());

        let (user, pass) = auth
            .split_once(':')
            .expect("generated auth string always contains a ':' separator");
        assert!(creds.matches(user, pass));
        assert!(!creds.matches(user, "wrong"));
        assert!(!creds.matches("wronguser", pass));

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
