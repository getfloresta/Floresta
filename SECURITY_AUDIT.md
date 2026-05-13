# Floresta Security Audit Report

## Summary
**Target:** [getfloresta/Floresta](https://github.com/getfloresta/Floresta) - Lightweight Bitcoin Client
**Files audited:** 93 Rust source files across 12 crates
**Vulnerabilities found:** 5 (2 Critical, 1 High, 2 Medium)

---

## 🔴 C-01: Missing Authentication on JSON-RPC Server

**Severity:** Critical
**File:** `crates/floresta-node/src/json_rpc/server.rs`
**Impact:** Any process with network access to the RPC port can fully control the Bitcoin wallet/node

**Description:**
The JSON-RPC server (axum-based, default Bitcoin port 8332) has zero authentication. Unlike Bitcoin Core which requires `rpcauth`/`rpcpassword` or generates a `.cookie` file, Floresta exposes all wallet and node operations to any unauthenticated client.

Methods like `loaddescriptor`, `listdescriptors`, `sendrawtransaction`, `addnode`, `gettransaction`, `getpeerinfo` etc. are all unprotected.

```rust
// Lines 756-773 - No auth middleware
let router = Router::new()
    .route("/", post(json_rpc_request).get(cannot_get))
    .layer(CorsLayer::new()
        .allow_private_network(true)
        .allow_methods([Method::POST, Method::HEAD]))
    .with_state(Arc::new(RpcImpl { ... }));
```

The `Config` struct and `ConfigFile` both lack any authentication field.

**Fix:** Add configurable `rpc_auth_token` with auto-generated cookie file fallback (Bitcoin Core model).

---

## 🔴 C-02: Missing Authentication on Electrum Protocol Server

**Severity:** Critical
**File:** `crates/floresta-electrum/src/electrum_protocol.rs`
**Impact:** Anyone who can connect to the Electrum TCP port can query wallet balances, transaction history, and subscribe to address notifications

**Description:**
The Electrum protocol server accepts raw TCP connections and processes JSON RPC requests without any form of authentication. The `client_accept_loop` directly accepts connections and processes requests.

---

## 🟠 H-01: Hostname Injection / SSRF via JSON-RPC addnode

**Severity:** High
**File:** `crates/floresta-node/src/florestad.rs` (function `resolve_hostname`, line 315-355)
**Impact:** Internal network probing, DNS exfiltration

**Description:**
The `addnode` RPC method accepts an arbitrary hostname string and passes it to `resolve_hostname()`, which performs DNS resolution via `dns_lookup::lookup_host()`. This could be used by an attacker who has RPC access (or if the server is bound to non-localhost) to probe internal networks or exfiltrate data via DNS.

```rust
fn resolve_hostname(hostname: &str, default_port: u16) -> Result<SocketAddr, FlorestadError> {
    // ...
    let ips: Vec<_> = match dns_lookup::lookup_host(hostname) {
        Ok(ips) => ips,
        // ...
    };
```

---

## 🟡 M-01: No Rate Limiting on RPC Endpoints

**Severity:** Medium
**File:** `crates/floresta-node/src/json_rpc/server.rs`
**Impact:** Resource exhaustion, DoS

**Description:**
All RPC methods can be called without rate limiting. The `rescanblockchain` method triggers an expensive blockchain rescan operation. An attacker could repeatedly call resource-intensive methods to degrade node performance.

---

## 🟡 M-02: Unauthenticated Descriptor Loading

**Severity:** Medium
**File:** `crates/floresta-node/src/json_rpc/server.rs` (method `loaddescriptor`, line 104-123)
**Impact:** Unauthorized wallet tracking, disk I/O amplification

**Description:**
The `loaddescriptor` RPC method allows adding watch-only descriptors to the wallet without any authentication. While descriptor validation occurs, there's no limit on how many descriptors can be added.

---

## FIXES

The critical issues are addressed in the PR:
1. **RPC Authentication**: Configurable `rpc_auth_token` with auto-generated cookie file (`.cookie` in data dir)
2. **Electrum Auth**: Optional authentication token for electrum connections
3. **Logging**: Random auth token printed to stdout on first run (like Bitcoin Core)
