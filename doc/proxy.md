# Proxy Configuration

Floresta will make some connections with random nodes in the P2P network. You may want to use a proxy to hide your IP address. You can do this by
providing a SOCKS5 socket, with the `--proxy` flag. For example, if you’re running Tor on your local machine, you can start `florestad` with the Tor proxy like this:

```bash
# start the daemon with the Tor proxy
florestad --proxy 127.0.0.1:9050
```

This will route all your connections through the Tor network, effectively masking your IP address.

With `--private-broadcast`, locally submitted transactions (for example via `sendrawtransaction`) are relayed only over short-lived Tor connections to onion peers, instead of being announced to all regular peers. Private broadcast requires a working SOCKS5 proxy (typically Tor on port 9050).

Private-broadcast diagnostics use the same [`tracing`](https://docs.rs/tracing) setup as the rest of Floresta. Enable them with `--debug`, or narrow to private-broadcast modules only:

```bash
RUST_LOG=floresta_wire::p2p_wire::node::private_broadcast_man=debug,floresta_wire::p2p_wire::peer=debug,info florestad ...
```
